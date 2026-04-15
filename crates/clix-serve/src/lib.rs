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
    metrics::init();
    let state = ClixState::load(home_dir())?;
    state.ensure_dirs()?;
    let cap_registry = build_registry(&state)?;
    let wf_registry  = build_workflow_registry(&state)?;
    let policy       = load_policy(&state)?;
    let store        = Mutex::new(ReceiptStore::open(&state.receipts_db)?);

    // Locate clix-worker binary and create warm worker pool (300s idle TTL).
    // Falls back to direct spawn with a warning if clix-worker is not installed.
    let worker_binary = WorkerRegistry::locate_worker_binary();
    let worker_registry = if worker_binary.exists() {
        Some(WorkerRegistry::new(worker_binary, 300))
    } else {
        eprintln!("[clix-serve] clix-worker not found — subprocess capabilities will run without isolation");
        None
    };

    Ok(Arc::new(ServeState { cap_registry, wf_registry, policy, store, state, worker_registry }))
}
