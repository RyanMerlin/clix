pub mod dispatch;
pub mod methods;
pub mod transport;
pub use dispatch::{dispatch, ServeState};

use std::sync::{Arc, Mutex};
use anyhow::Result;
use clix_core::loader::{build_registry, build_workflow_registry, load_policy};
use clix_core::receipts::ReceiptStore;
use clix_core::state::{home_dir, ClixState};

pub fn build_serve_state() -> Result<Arc<ServeState>> {
    let state = ClixState::load(home_dir())?;
    state.ensure_dirs()?;
    let cap_registry = build_registry(&state)?;
    let wf_registry  = build_workflow_registry(&state)?;
    let policy       = load_policy(&state)?;
    let store        = Mutex::new(ReceiptStore::open(&state.receipts_db)?);
    Ok(Arc::new(ServeState { cap_registry, wf_registry, policy, store, state }))
}
