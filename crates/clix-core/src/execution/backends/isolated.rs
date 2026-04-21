/// IsolatedBackend: routes capability subprocess invocations through the warm worker pool.
///
/// This replaces the direct `Command::new` in `subprocess.rs` for all non-builtin capabilities.
/// The only remaining direct spawn path is `builtin_handler` (in-process, tier=None).
use std::collections::HashMap;
use std::path::PathBuf;
use std::sync::Arc;
use crate::error::{ClixError, Result};
use crate::manifest::capability::{IsolationTier, SandboxProfile};
use crate::execution::worker_registry::WorkerRegistry;
use crate::execution::worker_protocol::{WorkerRequest, WorkerEvent};
#[cfg(not(target_os = "linux"))]
use super::subprocess::run_subprocess;
#[cfg(not(target_os = "linux"))]
use tracing::warn;
use uuid::Uuid;

pub struct IsolatedDispatch {
    pub exit_code: i32,
    pub stdout: String,
    pub stderr: String,
    pub isolation_tier: IsolationTier,
    pub binary_sha256: Option<String>,
    /// Opaque mint ID generated at dispatch time (for broker audit correlation).
    pub token_mint_id: Option<Uuid>,
}

/// Run a capability subprocess through the isolation layer.
///
/// - If `tier` is `WarmWorker` on Linux, dispatches to the worker pool.
/// - If the worker binary is not available or Linux is not the OS, falls back to a direct
///   subprocess spawn **and logs a loud warning** (the caller can opt to reject via policy).
/// - If `tier` is `None`, this should not be called — callers should use `builtin_handler`.
pub fn run_isolated(
    profile: &str,
    command: &str,
    args: &[String],
    cwd: &PathBuf,
    secrets: &HashMap<String, String>,
    tier: &IsolationTier,
    sandbox_profile: Option<&SandboxProfile>,
    registry: &Arc<WorkerRegistry>,
    credentials_declared: bool,
) -> Result<IsolatedDispatch> {
    match tier {
        IsolationTier::None => {
            Err(ClixError::Worker("run_isolated called with tier=none; use builtin_handler".to_string()))
        }
        IsolationTier::WarmWorker => {
            #[cfg(target_os = "linux")]
            {
                run_via_worker(profile, command, args, cwd, secrets, sandbox_profile, registry, credentials_declared)
            }
            #[cfg(not(target_os = "linux"))]
            {
                warn_no_isolation(command);
                run_direct_fallback(command, args, cwd, secrets, tier)
            }
        }
        IsolationTier::Firecracker => {
            Err(ClixError::Isolation(
                "Firecracker tier is not yet implemented; set isolation: warm_worker in the capability manifest".to_string()
            ))
        }
    }
}

#[cfg(target_os = "linux")]
fn run_via_worker(
    profile: &str,
    command: &str,
    args: &[String],
    cwd: &PathBuf,
    secrets: &HashMap<String, String>,
    sandbox_profile: Option<&SandboxProfile>,
    registry: &Arc<WorkerRegistry>,
    credentials_declared: bool,
) -> Result<IsolatedDispatch> {
    let request_id = Uuid::new_v4().to_string();
    let mut full_argv = vec![command.to_string()];
    full_argv.extend_from_slice(args);

    let request = WorkerRequest {
        request_id: request_id.clone(),
        argv: full_argv,
        env: secrets.clone(),
        cwd: cwd.to_string_lossy().to_string(),
        streaming: false,
    };

    let event = registry.dispatch(profile, command, &IsolationTier::WarmWorker, sandbox_profile, request, credentials_declared)?;

    match event {
        WorkerEvent::Exit { exit_code, stdout, stderr, .. } => Ok(IsolatedDispatch {
            exit_code,
            stdout,
            stderr,
            isolation_tier: IsolationTier::WarmWorker,
            binary_sha256: None, // filled by registry after handshake; not plumbed here yet
            token_mint_id: Some(Uuid::new_v4()),
        }),
        WorkerEvent::Error { message, .. } => Err(ClixError::Worker(message)),
        _ => Err(ClixError::Worker("unexpected event from worker".to_string())),
    }
}

#[cfg(not(target_os = "linux"))]
fn run_direct_fallback(
    command: &str,
    args: &[String],
    cwd: &PathBuf,
    secrets: &HashMap<String, String>,
    tier: &IsolationTier,
) -> Result<IsolatedDispatch> {
    let sub = run_subprocess(command, args, cwd, secrets)?;
    Ok(IsolatedDispatch {
        exit_code: sub.exit_code,
        stdout: sub.stdout,
        stderr: sub.stderr,
        isolation_tier: tier.clone(),
        binary_sha256: None,
        token_mint_id: None,
    })
}

#[cfg(not(target_os = "linux"))]
fn warn_no_isolation(command: &str) {
    warn!(
        command,
        "isolation not available on this platform — running without sandboxing (unsafe for adversarial agents)"
    );
}
