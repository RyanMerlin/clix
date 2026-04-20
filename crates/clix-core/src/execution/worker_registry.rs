/// Warm worker pool: manages long-lived jailed worker processes, keyed by (profile, binary_path).
///
/// Each entry is a `WorkerHandle` — a live child process with a connected unix socket for
/// dispatching capability invocations. Workers are spawned lazily on first use and reaped
/// after `idle_ttl_secs` of inactivity.
///
/// On non-Linux platforms the registry returns an `Err(ClixError::Isolation)` with a
/// descriptive message if isolation ≠ `none`. Builtins always run in-process.
use std::collections::HashMap;
use std::path::PathBuf;
use std::sync::{Arc, Mutex};
use std::time::{Duration, Instant};
use crate::error::{ClixError, Result};
use crate::manifest::capability::{IsolationTier, SandboxProfile};
use super::worker_protocol::{WorkerRequest, WorkerEvent, WorkerHandshake, WorkerReady};
use crate::sandbox::jail::{JailConfig, resolve_and_hash_binary, discover_lib_deps};

/// Unique key for a worker slot.
#[derive(Debug, Clone, PartialEq, Eq, Hash)]
pub struct WorkerKey {
    pub profile: String,
    pub binary: String, // absolute path
    pub tier: IsolationTier,
}

/// A live connection to a warm worker process.
pub struct WorkerHandle {
    pub key: WorkerKey,
    /// The OS socket connected to the worker's control socket.
    pub socket: std::os::unix::net::UnixStream,
    /// Time of last successful dispatch (used for idle TTL reaping).
    pub last_used: Instant,
    /// The child process (kept alive as long as the handle is alive).
    pub child: std::process::Child,
    /// Binary SHA-256 recorded at spawn time.
    pub binary_sha256: String,
}

/// The registry. Designed to be shared as `Arc<WorkerRegistry>`.
pub struct WorkerRegistry {
    workers: Mutex<HashMap<WorkerKey, WorkerHandle>>,
    /// Path to the `clix-worker` binary.
    worker_binary: PathBuf,
    idle_ttl: Duration,
    /// Unix socket path for the clix-broker credential daemon.
    /// If `None`, no broker minting is attempted (static secrets only).
    broker_socket: Option<PathBuf>,
}

impl WorkerRegistry {
    /// Create a new registry.
    ///
    /// `worker_binary` is the absolute path to the `clix-worker` executable.
    /// `idle_ttl_secs` is how long a worker may sit idle before being reaped.
    /// `broker_socket` is the path to the broker Unix socket (None = no broker).
    pub fn new(worker_binary: PathBuf, idle_ttl_secs: u64) -> Arc<Self> {
        let broker_socket = {
            use super::broker_client::broker_socket_path;
            let p = broker_socket_path();
            if p.exists() { Some(p) } else { None }
        };
        Arc::new(WorkerRegistry {
            workers: Mutex::new(HashMap::new()),
            worker_binary,
            idle_ttl: Duration::from_secs(idle_ttl_secs),
            broker_socket,
        })
    }

    /// Create a registry with an explicit broker socket path (useful for testing).
    pub fn new_with_broker(worker_binary: PathBuf, idle_ttl_secs: u64, broker_socket: Option<PathBuf>) -> Arc<Self> {
        Arc::new(WorkerRegistry {
            workers: Mutex::new(HashMap::new()),
            worker_binary,
            idle_ttl: Duration::from_secs(idle_ttl_secs),
            broker_socket,
        })
    }

    /// Locate the `clix-worker` binary next to the current executable, or on PATH.
    pub fn locate_worker_binary() -> PathBuf {
        // First try alongside the current executable
        if let Ok(exe) = std::env::current_exe() {
            let candidate = exe.parent().map(|d| d.join("clix-worker")).unwrap_or_default();
            if candidate.exists() {
                return candidate;
            }
        }
        // Fall back to PATH lookup
        PathBuf::from("clix-worker")
    }

    /// Dispatch a capability execution to the appropriate worker. Spawns a new worker if none
    /// exists for the key, or if the existing worker has died.
    ///
    /// `credentials_declared`: true if the capability manifest declares at least one credential.
    /// When true, broker unavailability is a hard error rather than a silent fallback.
    pub fn dispatch(
        self: &Arc<Self>,
        profile: &str,
        binary_command: &str,
        tier: &IsolationTier,
        sandbox_profile: Option<&SandboxProfile>,
        request: WorkerRequest,
        credentials_declared: bool,
    ) -> Result<WorkerEvent> {
        if matches!(tier, IsolationTier::None) {
            return Err(ClixError::Worker("dispatch called with tier=none; use builtin handler".to_string()));
        }

        #[cfg(not(target_os = "linux"))]
        {
            return Err(ClixError::Isolation(
                "isolation is only supported on Linux; set CLIX_ISOLATION_REQUIRE=none to disable (unsafe)".to_string()
            ));
        }

        #[cfg(target_os = "linux")]
        {
            if matches!(tier, IsolationTier::Firecracker) {
                return Err(ClixError::Isolation(
                    "Firecracker tier is not yet implemented; use warm_worker".to_string()
                ));
            }

            let key = WorkerKey {
                profile: profile.to_string(),
                binary: binary_command.to_string(),
                tier: tier.clone(),
            };

            // Mint ephemeral credentials from the broker and merge into request env.
            // Fatal when the capability declares credentials (credentials_declared=true) and
            // the broker is unreachable — running without expected credentials is unsafe.
            let mut request = request;
            if let Some(broker_path) = &self.broker_socket {
                let cli = super::broker_client::cli_name_from_command(binary_command);
                let broker_env = super::broker_client::mint_credentials(broker_path, cli, credentials_declared)?;
                for (k, v) in broker_env {
                    request.env.insert(k, v);
                }
            }

            self.ensure_worker(&key, sandbox_profile)?;
            self.send_request(&key, request)
        }
    }

    /// Return the number of live workers currently in the pool (for testing / metrics).
    pub fn worker_count(&self) -> usize {
        self.workers.lock().unwrap().len()
    }

    /// Reap workers that have exceeded their idle TTL. Call periodically.
    pub fn reap_idle(&self) {
        let mut workers = self.workers.lock().unwrap();
        let ttl = self.idle_ttl;
        workers.retain(|_, handle| {
            handle.last_used.elapsed() < ttl
        });
    }

    /// Shut down all workers gracefully.
    pub fn shutdown(&self) {
        let mut workers = self.workers.lock().unwrap();
        for (_, mut handle) in workers.drain() {
            let _ = handle.child.kill();
            let _ = handle.child.wait();
        }
    }
}

// ── Internal implementation ───────────────────────────────────────────────────

impl WorkerRegistry {
    #[cfg(target_os = "linux")]
    fn ensure_worker(&self, key: &WorkerKey, sandbox_profile: Option<&SandboxProfile>) -> Result<()> {
        let mut workers = self.workers.lock().unwrap();
        // Check if existing worker is still alive
        if let Some(handle) = workers.get_mut(key) {
            // Try a quick health check by attempting a zero-byte peek on the socket
            let alive = handle.child.try_wait().map(|s| s.is_none()).unwrap_or(false);
            if alive {
                return Ok(());
            }
            // Worker died — remove and respawn
            workers.remove(key);
        }
        drop(workers); // release lock before spawning

        let handle = self.spawn_worker(key, sandbox_profile)?;
        self.workers.lock().unwrap().insert(key.clone(), handle);
        Ok(())
    }

    #[cfg(target_os = "linux")]
    fn spawn_worker(&self, key: &WorkerKey, sandbox_profile: Option<&SandboxProfile>) -> Result<WorkerHandle> {
        use std::os::unix::net::UnixStream;
        use std::io::{BufRead, BufReader, Write};

        // Resolve and hash the binary
        let (binary_path, binary_sha256) = resolve_and_hash_binary(&key.binary)?;
        let lib_deps = discover_lib_deps(&binary_path);

        // Build jail config, merging in the sandbox profile's extra bind mounts.
        // We also add the current working directory as a read-write extra mount so that
        // commands like `git status` can operate on the actual workspace.
        let sp = sandbox_profile.cloned().unwrap_or_default();
        let mut extra_rw_bind = sp.fs.extra_rw_bind.clone();
        if let Ok(cwd) = std::env::current_dir() {
            let cwd_str = cwd.to_string_lossy().to_string();
            if !extra_rw_bind.contains(&cwd_str) {
                extra_rw_bind.push(cwd_str);
            }
        }
        let jail_config = JailConfig {
            pinned_binary: binary_path.clone(),
            binary_sha256: binary_sha256.clone(),
            lib_paths: lib_deps,
            fs_policy: crate::manifest::capability::FsPolicy {
                extra_ro_bind: sp.fs.extra_ro_bind.clone(),
                extra_rw_bind,
                share_host_tmp: sp.fs.share_host_tmp,
            },
            network_policy: sp.network.clone(),
            limits: sp.limits.clone(),
            extra_deny_syscalls: sp.extra_syscalls.clone(),
        };

        // Create a socketpair: gateway holds sock_a, worker gets sock_b fd
        let (sock_a, sock_b) = UnixStream::pair()
            .map_err(|e| ClixError::Worker(format!("socketpair: {e}")))?;

        use std::os::unix::io::IntoRawFd;
        let worker_fd = sock_b.into_raw_fd();

        // Build environment for the worker
        let mut env_vars = jail_config.to_env();
        env_vars.push((crate::sandbox::jail::env_keys::WORKER_SOCKET_FD.to_string(), worker_fd.to_string()));

        // Spawn the worker binary.
        // IMPORTANT: UnixStream::pair() creates sockets with SOCK_CLOEXEC, meaning the
        // worker-side fd would be automatically closed on exec before the worker could use it.
        // We clear O_CLOEXEC on the worker fd in a pre_exec hook so it survives the exec.
        use std::os::unix::process::CommandExt;
        let mut cmd = std::process::Command::new(&self.worker_binary);
        cmd.env_clear()
            .envs(env_vars)
            .stdin(std::process::Stdio::null())
            .stdout(std::process::Stdio::null())
            .stderr(std::process::Stdio::inherit());
        unsafe {
            cmd.pre_exec(move || {
                // Clear CLOEXEC so the socket fd is inherited across exec into clix-worker
                let ret = libc::fcntl(worker_fd, libc::F_SETFD, 0);
                if ret < 0 {
                    return Err(std::io::Error::last_os_error());
                }
                Ok(())
            });
        }
        let child = cmd.spawn()
            .map_err(|e| ClixError::Worker(format!("spawn worker {}: {e}", self.worker_binary.display())))?;

        // Close our copy of the worker-side fd (gateway side no longer needs it)
        unsafe { libc::close(worker_fd) };

        // Perform the handshake: send WorkerHandshake, expect WorkerReady
        let handshake = WorkerHandshake {
            pinned_binary: binary_path.to_string_lossy().to_string(),
            binary_sha256: binary_sha256.clone(),
        };
        let mut sock_a_writer = sock_a.try_clone()
            .map_err(|e| ClixError::Worker(format!("clone socket: {e}")))?;
        let sock_a_reader = sock_a.try_clone()
            .map_err(|e| ClixError::Worker(format!("clone socket for read: {e}")))?;

        let msg = serde_json::to_string(&handshake)? + "\n";
        sock_a_writer.write_all(msg.as_bytes())
            .map_err(|e| ClixError::Worker(format!("write handshake: {e}")))?;

        // Read WorkerReady (with timeout)
        sock_a_writer.set_read_timeout(Some(Duration::from_secs(30)))
            .map_err(|e| ClixError::Worker(format!("set timeout: {e}")))?;
        let mut reader = BufReader::new(sock_a_reader);
        let mut line = String::new();
        reader.read_line(&mut line)
            .map_err(|e| ClixError::Worker(format!("read ready: {e}")))?;
        let ready: WorkerReady = serde_json::from_str(line.trim())
            .map_err(|e| ClixError::Worker(format!("parse ready: {e}")))?;
        if !ready.ok {
            return Err(ClixError::Worker(format!(
                "worker failed to initialize: {}",
                ready.error.unwrap_or_else(|| "unknown".to_string())
            )));
        }

        Ok(WorkerHandle {
            key: key.clone(),
            socket: sock_a,
            last_used: Instant::now(),
            child,
            binary_sha256,
        })
    }

    #[cfg(target_os = "linux")]
    fn send_request(&self, key: &WorkerKey, request: WorkerRequest) -> Result<WorkerEvent> {
        use std::io::{BufRead, BufReader, Write};

        let mut workers = self.workers.lock().unwrap();
        let handle = workers.get_mut(key)
            .ok_or_else(|| ClixError::Worker("worker not found after ensure".to_string()))?;

        // Send the request
        let msg = serde_json::to_string(&request)? + "\n";
        handle.socket.write_all(msg.as_bytes())
            .map_err(|e| ClixError::Worker(format!("write request: {e}")))?;

        // Read response events until Exit or Error
        let sock_clone = handle.socket.try_clone()
            .map_err(|e| ClixError::Worker(format!("clone socket: {e}")))?;
        handle.last_used = Instant::now();
        drop(workers); // release lock while waiting for response

        let mut reader = BufReader::new(sock_clone);
        loop {
            let mut line = String::new();
            reader.read_line(&mut line)
                .map_err(|e| ClixError::Worker(format!("read event: {e}")))?;
            if line.is_empty() {
                return Err(ClixError::Worker("worker closed connection unexpectedly".to_string()));
            }
            let event: WorkerEvent = serde_json::from_str(line.trim())
                .map_err(|e| ClixError::Worker(format!("parse event: {e}")))?;
            match event {
                WorkerEvent::Exit { .. } | WorkerEvent::Error { .. } => return Ok(event),
                // For streaming events, accumulate (simplified: we just return on Exit/Error)
                WorkerEvent::Stdout { .. } | WorkerEvent::Stderr { .. } => {
                    // In the current non-streaming mode these shouldn't appear;
                    // if they do, continue reading until Exit/Error
                    continue;
                }
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_worker_key_eq() {
        let a = WorkerKey { profile: "readonly".to_string(), binary: "/usr/bin/gcloud".to_string(), tier: IsolationTier::WarmWorker };
        let b = WorkerKey { profile: "readonly".to_string(), binary: "/usr/bin/gcloud".to_string(), tier: IsolationTier::WarmWorker };
        assert_eq!(a, b);
    }

    #[test]
    fn test_registry_new() {
        let reg = WorkerRegistry::new(PathBuf::from("clix-worker"), 300);
        reg.reap_idle(); // should not panic on empty registry
        reg.shutdown();
    }
}
