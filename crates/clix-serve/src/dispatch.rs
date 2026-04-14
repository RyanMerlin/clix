use std::sync::{Arc, Mutex};
use clix_core::execution::worker_registry::WorkerRegistry;
use clix_core::policy::PolicyBundle;
use clix_core::receipts::ReceiptStore;
use clix_core::registry::{CapabilityRegistry, WorkflowRegistry};
use clix_core::state::ClixState;

pub struct ServeState {
    pub cap_registry:    CapabilityRegistry,
    pub wf_registry:     WorkflowRegistry,
    pub policy:          PolicyBundle,
    pub store:           Mutex<ReceiptStore>,
    pub state:           ClixState,
    /// Warm worker pool for jailed subprocess execution. `None` if the clix-worker binary
    /// is not available (falls back to direct spawn with a loud warning).
    pub worker_registry: Option<Arc<WorkerRegistry>>,
}

pub async fn dispatch(serve: Arc<ServeState>, req: serde_json::Value) -> serde_json::Value {
    let id     = req.get("id").cloned().unwrap_or(serde_json::Value::Null);
    let method = req["method"].as_str().unwrap_or("").to_string();
    let params = req.get("params").and_then(|p| p.as_object()).cloned()
        .map(serde_json::Value::Object).unwrap_or(serde_json::json!({}));

    let result = match method.as_str() {
        "initialize"     => crate::methods::mcp::initialize(&serve),
        "tools/list"     => crate::methods::mcp::tools_list(&serve, &params),
        "tools/call"     => crate::methods::mcp::tools_call(&serve, &params).await,
        "resources/list" => crate::methods::mcp::resources_list(&serve),
        "workflows/list" => crate::methods::extensions::workflows_list(&serve),
        "workflows/run"  => crate::methods::extensions::workflows_run(&serve, &params).await,
        "onboard/probe"  => crate::methods::extensions::onboard_probe(&params),
        "packs/list"     => crate::methods::extensions::packs_list(&serve),
        "status/get"     => crate::methods::extensions::status_get(&serve),
        "shim/call"      => crate::methods::extensions::shim_call(&serve, &params).await,
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
    use clix_core::manifest::capability::{Backend, CapabilityManifest, RiskLevel, SideEffectClass};
    use clix_core::policy::PolicyBundle;
    use clix_core::receipts::ReceiptStore;
    use clix_core::registry::{CapabilityRegistry, WorkflowRegistry};
    use clix_core::state::ClixState;

    fn make_cap(name: &str, desc: &str) -> CapabilityManifest {
        CapabilityManifest {
            name: name.to_string(), version: 1, description: Some(desc.to_string()),
            backend: Backend::Builtin { name: "date".to_string() },
            risk: RiskLevel::Low, side_effect_class: SideEffectClass::ReadOnly,
            sandbox_profile: None, isolation: Default::default(), approval_policy: None,
            input_schema: serde_json::json!({}), validators: vec![], credentials: vec![], argv_pattern: None,
        }
    }

    fn test_state() -> Arc<ServeState> {
        let home = std::env::temp_dir().join("clix-test-serve");
        std::fs::create_dir_all(&home).unwrap();
        Arc::new(ServeState {
            cap_registry:    CapabilityRegistry::from_vec(vec![]),
            wf_registry:     WorkflowRegistry::from_vec(vec![]),
            policy:          PolicyBundle::default(),
            store:           Mutex::new(ReceiptStore::open(&home.join("receipts.db")).unwrap()),
            state:           ClixState::from_home(home),
            worker_registry: None,
        })
    }

    fn test_state_with_caps() -> Arc<ServeState> {
        let home = std::env::temp_dir().join("clix-test-serve-ns");
        std::fs::create_dir_all(&home).unwrap();
        let mut reg = CapabilityRegistry::from_vec(vec![]);
        reg.insert(make_cap("gcloud.aiplatform.models.list", "List Vertex AI models"));
        reg.insert(make_cap("gcloud.aiplatform.endpoints.list", "List Vertex AI endpoints"));
        reg.insert(make_cap("system.date", "Return UTC date"));
        Arc::new(ServeState {
            cap_registry:    reg,
            wf_registry:     WorkflowRegistry::from_vec(vec![]),
            policy:          PolicyBundle::default(),
            store:           Mutex::new(ReceiptStore::open(&home.join("receipts.db")).unwrap()),
            state:           ClixState::from_home(home),
            worker_registry: None,
        })
    }

    #[tokio::test]
    async fn test_initialize() {
        let s = test_state();
        let req = serde_json::json!({"jsonrpc":"2.0","id":1,"method":"initialize","params":{}});
        let resp = dispatch(s, req).await;
        assert_eq!(resp["result"]["serverInfo"]["name"], "clix");
        assert_eq!(resp["result"]["capabilities"]["extensions"]["clix"]["namespaces"], true);
    }

    #[tokio::test]
    async fn test_tools_list_empty() {
        let s = test_state();
        let req = serde_json::json!({"jsonrpc":"2.0","id":2,"method":"tools/list","params":{}});
        let resp = dispatch(s, req).await;
        assert!(resp["result"]["tools"].as_array().is_some());
    }

    #[tokio::test]
    async fn test_tools_list_stub_view() {
        let s = test_state_with_caps();

        // Default (no params) → stub view: 2 namespace stubs
        let req = serde_json::json!({"jsonrpc":"2.0","id":10,"method":"tools/list","params":{}});
        let resp = dispatch(Arc::clone(&s), req).await;
        let tools = resp["result"]["tools"].as_array().unwrap();
        assert_eq!(tools.len(), 2, "expected 2 namespace stubs");
        assert!(tools.iter().all(|t| t["type"] == "namespace"));

        // namespace drill-in → full tool descriptors for gcloud.aiplatform
        let req2 = serde_json::json!({"jsonrpc":"2.0","id":11,"method":"tools/list","params":{"namespace":"gcloud.aiplatform"}});
        let resp2 = dispatch(Arc::clone(&s), req2).await;
        let tools2 = resp2["result"]["tools"].as_array().unwrap();
        assert_eq!(tools2.len(), 2);
        assert!(tools2.iter().all(|t| t["type"] != "namespace"));

        // all: true → flat list of 3
        let req3 = serde_json::json!({"jsonrpc":"2.0","id":12,"method":"tools/list","params":{"all":true}});
        let resp3 = dispatch(Arc::clone(&s), req3).await;
        let tools3 = resp3["result"]["tools"].as_array().unwrap();
        assert_eq!(tools3.len(), 3);
    }

    #[tokio::test]
    async fn test_unknown_method() {
        let s = test_state();
        let req = serde_json::json!({"jsonrpc":"2.0","id":3,"method":"nope","params":{}});
        let resp = dispatch(s, req).await;
        assert_eq!(resp["error"]["code"], -32601);
    }
}
