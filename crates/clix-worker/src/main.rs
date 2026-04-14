/// clix-worker: the jailed subprocess worker.
///
/// Lifecycle:
///   1. Started by clix-gateway with CLIX_JAIL_* env vars + CLIX_WORKER_FD=<fd>.
///   2. Calls `enter_jail()` to set up Linux namespaces, Landlock, seccomp, etc.
///   3. Performs a handshake with the gateway over the inherited socket.
///   4. Loops, reading `WorkerRequest` messages and executing the pinned CLI binary.
///
/// The binary must NOT call `exit()` on individual request failures — it should return an
/// `WorkerEvent::Error` and continue looping. It only exits when the socket closes or on a
/// fatal unrecoverable condition.
use std::io::{BufRead, BufReader, Write};
use std::os::unix::net::UnixStream;
use std::os::unix::io::FromRawFd;
use std::process::Stdio;
use clix_core::sandbox::jail::{JailConfig, verify_binary_hash, env_keys};
use clix_core::execution::worker_protocol::{WorkerHandshake, WorkerReady, WorkerRequest, WorkerEvent};

fn main() {
    // Load the jail config from env vars
    let config = match JailConfig::from_env() {
        Some(c) => c,
        None => {
            eprintln!("[clix-worker] FATAL: missing jail config env vars");
            std::process::exit(1);
        }
    };

    // Get the socket fd
    let fd: i32 = std::env::var(env_keys::WORKER_SOCKET_FD)
        .ok()
        .and_then(|s| s.parse().ok())
        .unwrap_or(-1);
    if fd < 0 {
        eprintln!("[clix-worker] FATAL: CLIX_WORKER_FD not set or invalid");
        std::process::exit(1);
    }

    // Enter the jail (sets up namespaces, landlock, seccomp, etc.)
    if let Err(e) = clix_core::sandbox::jail::enter_jail(&config) {
        eprintln!("[clix-worker] FATAL: jail setup failed: {e}");
        std::process::exit(1);
    }

    // Take ownership of the socket fd
    let socket = unsafe { UnixStream::from_raw_fd(fd) };

    // Run the main loop
    if let Err(e) = worker_loop(socket, &config) {
        eprintln!("[clix-worker] FATAL: {e}");
        std::process::exit(1);
    }
}

fn worker_loop(socket: UnixStream, config: &JailConfig) -> std::io::Result<()> {
    use std::io::ErrorKind;

    let mut writer = socket.try_clone()?;
    let reader = BufReader::new(socket);
    let mut lines = reader.lines();

    // Read handshake from gateway
    let handshake_line = match lines.next() {
        Some(Ok(l)) => l,
        Some(Err(e)) => return Err(e),
        None => return Ok(()), // gateway closed before handshake
    };
    let handshake: WorkerHandshake = match serde_json::from_str(&handshake_line) {
        Ok(h) => h,
        Err(e) => {
            let ready = WorkerReady { ok: false, error: Some(format!("bad handshake: {e}")) };
            let _ = write_json(&mut writer, &ready);
            return Ok(());
        }
    };

    // Verify binary integrity using the in-jail path (/bin/<name>), since we are now
    // inside the mount namespace where the host path no longer exists.
    let bin_name = config.pinned_binary.file_name()
        .map(|n| std::path::PathBuf::from("/bin").join(n))
        .unwrap_or_else(|| config.pinned_binary.clone());
    if let Err(e) = verify_binary_hash(&bin_name, &handshake.binary_sha256) {
        let ready = WorkerReady { ok: false, error: Some(e.to_string()) };
        let _ = write_json(&mut writer, &ready);
        return Ok(());
    }

    // Confirm ready
    let ready = WorkerReady { ok: true, error: None };
    write_json(&mut writer, &ready)?;

    // Main dispatch loop
    for line in lines {
        let line = match line {
            Ok(l) => l,
            Err(e) if e.kind() == ErrorKind::UnexpectedEof || e.kind() == ErrorKind::BrokenPipe => break,
            Err(e) => return Err(e),
        };
        let request: WorkerRequest = match serde_json::from_str(&line) {
            Ok(r) => r,
            Err(e) => {
                let event = WorkerEvent::Error {
                    request_id: "unknown".to_string(),
                    message: format!("bad request: {e}"),
                };
                let _ = write_json(&mut writer, &event);
                continue;
            }
        };

        let event = execute_request(&request, config);
        if write_json(&mut writer, &event).is_err() {
            break; // gateway closed
        }
    }

    Ok(())
}

fn execute_request(request: &WorkerRequest, config: &JailConfig) -> WorkerEvent {
    let Some(binary_name) = config.pinned_binary.file_name() else {
        return WorkerEvent::Error {
            request_id: request.request_id.clone(),
            message: "invalid binary path in jail config".to_string(),
        };
    };

    // Inside the jail, the binary lives at /bin/<name>
    let jail_binary = std::path::PathBuf::from("/bin").join(binary_name);

    // argv[0] is the command name; subsequent args are the actual arguments
    let args: Vec<&str> = if request.argv.len() > 1 {
        request.argv[1..].iter().map(String::as_str).collect()
    } else {
        vec![]
    };

    // Use the requested cwd if it exists inside the jail; fall back to /home/clix.
    // The jail has a tmpfs root so host paths (e.g. /mnt/...) don't exist inside.
    let effective_cwd = {
        let p = std::path::Path::new(&request.cwd);
        if p.exists() { p.to_path_buf() } else { std::path::PathBuf::from("/home/clix") }
    };

    // Build a clean environment: only what the gateway provided (ephemeral creds + minimal vars)
    let mut cmd = std::process::Command::new(&jail_binary);
    cmd.args(&args)
        .current_dir(&effective_cwd)
        .stdin(Stdio::null())
        .stdout(Stdio::piped())
        .stderr(Stdio::piped())
        .env_clear();

    for (k, v) in &request.env {
        cmd.env(k, v);
    }
    // Minimal baseline env that CLI tools may need
    cmd.env("HOME", "/home/clix");
    cmd.env("PATH", "/bin:/usr/bin");
    cmd.env("TERM", "dumb");

    match cmd.output() {
        Ok(output) => WorkerEvent::Exit {
            request_id: request.request_id.clone(),
            exit_code: output.status.code().unwrap_or(1),
            stdout: String::from_utf8_lossy(&output.stdout).to_string(),
            stderr: String::from_utf8_lossy(&output.stderr).to_string(),
        },
        Err(e) => WorkerEvent::Error {
            request_id: request.request_id.clone(),
            message: format!("exec failed: {e}"),
        },
    }
}

fn write_json<T: serde::Serialize>(writer: &mut impl Write, value: &T) -> std::io::Result<()> {
    let line = serde_json::to_string(value).map_err(|e| std::io::Error::new(std::io::ErrorKind::Other, e))?;
    writer.write_all(line.as_bytes())?;
    writer.write_all(b"\n")?;
    writer.flush()
}
