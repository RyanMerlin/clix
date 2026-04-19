//! `ServeState` bootstrapper.

use std::sync::{Arc, Mutex};
use clix_core::manifest::capability::CapabilityManifest;
use clix_core::policy::PolicyBundle;
use clix_core::receipts::ReceiptStore;
use clix_core::registry::{CapabilityRegistry, WorkflowRegistry};
use clix_core::state::ClixState;
use clix_serve::dispatch::ServeState;

/// Build a `ServeState` backed by a fresh temp dir, in-memory receipt store,
/// and no warm worker registry.
pub fn make_state(caps: Vec<CapabilityManifest>, policy: PolicyBundle) -> Arc<ServeState> {
    let id = std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .unwrap()
        .subsec_nanos();
    let home = std::env::temp_dir().join(format!("clix-tk-{id}"));
    std::fs::create_dir_all(&home).unwrap();
    Arc::new(ServeState {
        cap_registry:    CapabilityRegistry::from_vec(caps),
        wf_registry:     WorkflowRegistry::from_vec(vec![]),
        policy,
        store:           Mutex::new(ReceiptStore::open(&home.join("r.db")).unwrap()),
        state:           ClixState::from_home(home),
        worker_registry: None,
    })
}

/// Issue a `tools/call` JSON-RPC over the stdio transport and return the parsed response.
pub async fn call(serve: &Arc<ServeState>, name: &str) -> serde_json::Value {
    let req = serde_json::json!({
        "jsonrpc": "2.0", "id": 1,
        "method": "tools/call",
        "params": {"name": name, "arguments": {}}
    });
    let line = serde_json::to_string(&req).unwrap();
    let resp = clix_serve::transport::stdio::process_line(Arc::clone(serve), &line)
        .await
        .unwrap();
    serde_json::from_str(&resp).unwrap()
}
