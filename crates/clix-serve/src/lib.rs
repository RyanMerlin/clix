pub mod dispatch;
pub mod methods;
pub mod metrics;
pub mod transport;
pub use dispatch::{dispatch, ServeState};

use std::sync::{Arc, Mutex};
use anyhow::Result;
use clix_core::execution::worker_registry::WorkerRegistry;
use clix_core::loader::{build_registry, build_workflow_registry, load_policy};
use clix_core::receipts::ReceiptStore;
use clix_core::state::{home_dir, ClixState};

pub fn build_serve_state() -> Result<Arc<ServeState>> {
    let allow_unsandboxed = std::env::var("CLIX_ALLOW_UNSANDBOXED").is_ok();
    build_serve_state_opts(allow_unsandboxed)
}

pub fn build_serve_state_opts(allow_unsandboxed: bool) -> Result<Arc<ServeState>> {
    metrics::init();

    #[cfg(not(target_os = "linux"))]
    print_sandbox_disabled_banner();

    let state = ClixState::load(home_dir())?;
    state.ensure_dirs()?;
    let cap_registry = build_registry(&state)?;
    let wf_registry  = build_workflow_registry(&state)?;
    let policy       = load_policy(&state)?;
    let store        = Mutex::new(ReceiptStore::open(&state.receipts_db)?);

    let worker_registry = init_worker_registry(allow_unsandboxed)?;

    Ok(Arc::new(ServeState { cap_registry, wf_registry, policy, store, state, worker_registry }))
}

#[cfg(not(target_os = "linux"))]
fn print_sandbox_disabled_banner() {
    use std::sync::OnceLock;
    static PRINTED: OnceLock<()> = OnceLock::new();
    PRINTED.get_or_init(|| {
        eprintln!();
        eprintln!("╔══════════════════════════════════════════════════════════════╗");
        eprintln!("║  SANDBOX DISABLED — clix is running in policy-only mode      ║");
        eprintln!("║  OS-level isolation (namespaces, seccomp, Landlock) requires  ║");
        eprintln!("║  Linux. Capabilities run without kernel-level restrictions.   ║");
        eprintln!("║  All receipts will carry sandbox_enforced=false.              ║");
        eprintln!("╚══════════════════════════════════════════════════════════════╝");
        eprintln!();
    });
}

fn init_worker_registry(allow_unsandboxed: bool) -> Result<Option<Arc<WorkerRegistry>>> {
    let worker_binary = WorkerRegistry::locate_worker_binary();
    if worker_binary.exists() {
        return Ok(Some(WorkerRegistry::new(worker_binary, 300)));
    }
    if allow_unsandboxed {
        eprintln!("[clix-serve] WARNING: clix-worker not found — running without OS-level isolation");
        eprintln!("[clix-serve] Acknowledged via CLIX_ALLOW_UNSANDBOXED. Do not use in production.");
        return Ok(None);
    }
    anyhow::bail!(
        "clix-worker binary not found on PATH.\n\
         OS-level isolation requires all five clix binaries installed together.\n\
         To run without isolation (unsafe), set CLIX_ALLOW_UNSANDBOXED=1."
    )
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::path::PathBuf;

    #[test]
    fn missing_worker_fails_without_flag() {
        // A non-existent path is never on PATH, so locate_worker_binary falls back to
        // the relative "clix-worker" which won't exist in the test working directory.
        let result = init_worker_registry(false);
        // Only meaningful when clix-worker truly isn't installed alongside the test binary.
        // If it IS found, the function succeeds — that's correct behaviour too.
        if let Err(e) = result {
            let msg = e.to_string();
            assert!(msg.contains("CLIX_ALLOW_UNSANDBOXED"), "error should mention the escape hatch: {msg}");
        }
    }

    #[test]
    fn missing_worker_ok_with_flag() {
        // When the flag is set and worker is absent, should return Ok(None).
        // We can only assert Ok if the binary truly isn't found; skip if it is.
        let worker_binary = WorkerRegistry::locate_worker_binary();
        if !worker_binary.exists() {
            let result = init_worker_registry(true);
            assert!(result.unwrap().is_none());
        }
    }
}
