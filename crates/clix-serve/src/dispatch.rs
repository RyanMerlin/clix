use std::sync::{Arc, Mutex};
use clix_core::policy::PolicyBundle;
use clix_core::receipts::ReceiptStore;
use clix_core::registry::{CapabilityRegistry, WorkflowRegistry};
use clix_core::state::ClixState;

pub struct ServeState {
    pub cap_registry: CapabilityRegistry,
    pub wf_registry:  WorkflowRegistry,
    pub policy:       PolicyBundle,
    pub store:        Mutex<ReceiptStore>,
    pub state:        ClixState,
}

pub async fn dispatch(serve: Arc<ServeState>, req: serde_json::Value) -> serde_json::Value {
    let id     = req.get("id").cloned().unwrap_or(serde_json::Value::Null);
    let method = req["method"].as_str().unwrap_or("").to_string();
    let params = req.get("params").and_then(|p| p.as_object()).cloned()
        .map(serde_json::Value::Object).unwrap_or(serde_json::json!({}));

    let result = match method.as_str() {
        "initialize"     => crate::methods::mcp::initialize(&serve),
        "tools/list"     => crate::methods::mcp::tools_list(&serve),
        "tools/call"     => crate::methods::mcp::tools_call(&serve, &params).await,
        "resources/list" => crate::methods::mcp::resources_list(&serve),
        "workflows/list" => crate::methods::extensions::workflows_list(&serve),
        "workflows/run"  => crate::methods::extensions::workflows_run(&serve, &params).await,
        "onboard/probe"  => crate::methods::extensions::onboard_probe(&params),
        "packs/list"     => crate::methods::extensions::packs_list(&serve),
        "status/get"     => crate::methods::extensions::status_get(&serve),
        _ => return rpc_error(id, -32601, format!("method not found: {method}")),
    };

    match result {
        Ok(value) => rpc_ok(id, value),
        Err(e)    => rpc_error(id, -32000, e),
    }
}

pub fn rpc_ok(id: serde_json::Value, result: serde_json::Value) -> serde_json::Value {
    serde_json::json!({"jsonrpc":"2.0","id":id,"result":result})
}

pub fn rpc_error(id: serde_json::Value, code: i32, message: String) -> serde_json::Value {
    serde_json::json!({"jsonrpc":"2.0","id":id,"error":{"code":code,"message":message}})
}

#[cfg(test)]
mod tests {
    use super::*;
    use clix_core::policy::PolicyBundle;
    use clix_core::receipts::ReceiptStore;
    use clix_core::registry::{CapabilityRegistry, WorkflowRegistry};
    use clix_core::state::ClixState;

    fn test_state() -> Arc<ServeState> {
        let home = std::env::temp_dir().join("clix-test-serve");
        std::fs::create_dir_all(&home).unwrap();
        Arc::new(ServeState {
            cap_registry: CapabilityRegistry::from_vec(vec![]),
            wf_registry:  WorkflowRegistry::from_vec(vec![]),
            policy:       PolicyBundle::default(),
            store:        Mutex::new(ReceiptStore::open(&home.join("receipts.db")).unwrap()),
            state:        ClixState::from_home(home),
        })
    }

    #[tokio::test]
    async fn test_initialize() {
        let s = test_state();
        let req = serde_json::json!({"jsonrpc":"2.0","id":1,"method":"initialize","params":{}});
        let resp = dispatch(s, req).await;
        assert_eq!(resp["result"]["serverInfo"]["name"], "clix");
    }

    #[tokio::test]
    async fn test_tools_list_empty() {
        let s = test_state();
        let req = serde_json::json!({"jsonrpc":"2.0","id":2,"method":"tools/list","params":{}});
        let resp = dispatch(s, req).await;
        assert!(resp["result"]["tools"].as_array().is_some());
    }

    #[tokio::test]
    async fn test_unknown_method() {
        let s = test_state();
        let req = serde_json::json!({"jsonrpc":"2.0","id":3,"method":"nope","params":{}});
        let resp = dispatch(s, req).await;
        assert_eq!(resp["error"]["code"], -32601);
    }
}
