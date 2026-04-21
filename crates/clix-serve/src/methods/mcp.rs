use std::sync::Arc;
use clix_core::execution::run_capability;
use clix_core::policy::evaluate::ExecutionContext;
use clix_core::sandbox::sandbox_enforced;
use crate::dispatch::ServeState;

type MethodResult = std::result::Result<serde_json::Value, String>;

pub fn initialize(_serve: &Arc<ServeState>) -> MethodResult {
    Ok(serde_json::json!({
        "serverInfo": {"name":"clix","version":env!("CARGO_PKG_VERSION")},
        "protocolVersion": "2024-11-05",
        "capabilities": {
            "tools": {"listChanged": false},
            "resources": {"listChanged": false},
            "extensions": {"clix": {"namespaces": true, "workflows": true, "onboard": true}}
        },
        "sandboxEnforced": sandbox_enforced()
    }))
}

pub fn tools_list(serve: &Arc<ServeState>, params: &serde_json::Value) -> MethodResult {
    // "all: true" → flat list (backward compat)
    if params.get("all").and_then(|v| v.as_bool()).unwrap_or(false) {
        let tools: Vec<serde_json::Value> = serve.cap_registry.all().iter().map(|cap| serde_json::json!({
            "name": cap.name,
            "description": cap.description.as_deref().unwrap_or(""),
            "inputSchema": cap.input_schema
        })).collect();
        return Ok(serde_json::json!({"tools": tools}));
    }

    // "namespace: X" → drill-in: return full tool descriptors for that namespace group
    if let Some(ns) = params.get("namespace").and_then(|v| v.as_str()) {
        let tools: Vec<serde_json::Value> = serve.cap_registry.by_namespace(ns).iter().map(|cap| serde_json::json!({
            "name": cap.name,
            "description": cap.description.as_deref().unwrap_or(""),
            "inputSchema": cap.input_schema
        })).collect();
        return Ok(serde_json::json!({"tools": tools}));
    }

    // Default → namespace stub view
    let tools: Vec<serde_json::Value> = serve.cap_registry.namespaces().iter().map(|stub| serde_json::json!({
        "name": stub.key,
        "type": "namespace",
        "count": stub.count
    })).collect();
    Ok(serde_json::json!({"tools": tools}))
}

pub async fn tools_call(serve: &Arc<ServeState>, params: &serde_json::Value) -> MethodResult {
    let name = params["name"].as_str().ok_or("tools/call: missing 'name'")?;
    let arguments = params.get("arguments").cloned().unwrap_or(serde_json::json!({}));
    let ctx = ExecutionContext {
        env:     serve.state.config.default_env.clone(),
        cwd:     serve.state.config.workspace_root.clone()
                     .unwrap_or_else(|| std::env::current_dir().unwrap_or_else(|_| std::path::PathBuf::from("/"))),
        user:    "agent".to_string(),
        profile: serve.state.config.active_profiles.first().cloned().unwrap_or_else(|| "default".to_string()),
        approver: None,
    };
    let serve_clone = Arc::clone(serve);
    let name = name.to_string();
    let cap_name_for_metrics = name.clone();
    let outcome = tokio::task::spawn_blocking(move || {
        let store = serve_clone.store.lock().unwrap();
        run_capability(&serve_clone.cap_registry, &serve_clone.policy, &serve_clone.state.config.infisical(), &store, serve_clone.worker_registry.as_ref(), &name, arguments, ctx, &[])
    }).await.map_err(|e| format!("task join: {e}"))?;
    let outcome = match outcome {
        Ok(o) => {
            let status = if o.ok { "succeeded" } else if o.approval_required { "pending_approval" } else { "denied" };
            crate::metrics::record_call(&cap_name_for_metrics, status);
            if !o.ok && !o.approval_required { crate::metrics::record_denial(&cap_name_for_metrics); }
            o
        }
        Err(e) => {
            crate::metrics::record_call(&cap_name_for_metrics, "error");
            crate::metrics::record_error(&cap_name_for_metrics);
            return Err(e.to_string());
        }
    };
    let content = if let Some(result) = &outcome.result {
        if let Some(stdout) = result["stdout"].as_str() {
            vec![serde_json::json!({"type":"text","text":stdout})]
        } else {
            vec![serde_json::json!({"type":"text","text":serde_json::to_string(result).unwrap_or_default()})]
        }
    } else {
        vec![serde_json::json!({"type":"text","text":outcome.reason.as_deref().unwrap_or("")})]
    };
    Ok(serde_json::json!({
        "content": content,
        "isError": !outcome.ok,
        "_clix": {"receiptId": outcome.receipt_id, "approvalRequired": outcome.approval_required}
    }))
}

pub fn resources_list(serve: &Arc<ServeState>) -> MethodResult {
    let mut resources = vec![];
    if serve.state.packs_dir.exists() {
        if let Ok(entries) = std::fs::read_dir(&serve.state.packs_dir) {
            for entry in entries.flatten() {
                if entry.file_type().map(|t| t.is_dir()).unwrap_or(false) {
                    let name = entry.file_name().to_string_lossy().to_string();
                    resources.push(serde_json::json!({"uri":format!("clix://packs/{name}"),"name":name,"mimeType":"application/json"}));
                }
            }
        }
    }
    Ok(serde_json::json!({"resources": resources}))
}
