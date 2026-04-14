use std::sync::Arc;
use clix_core::execution::run_workflow;
use clix_core::packs::onboard_cli;
use clix_core::policy::evaluate::ExecutionContext;
use clix_core::sandbox::sandbox_enforced;
use crate::dispatch::ServeState;

type MethodResult = std::result::Result<serde_json::Value, String>;

pub fn workflows_list(serve: &Arc<ServeState>) -> MethodResult {
    let workflows: Vec<serde_json::Value> = serve.wf_registry.all().into_iter().map(|wf| serde_json::json!({
        "name": wf.name, "description": wf.description.as_deref().unwrap_or(""), "stepCount": wf.steps.len()
    })).collect();
    Ok(serde_json::json!({"workflows": workflows}))
}

pub async fn workflows_run(serve: &Arc<ServeState>, params: &serde_json::Value) -> MethodResult {
    let name = params["name"].as_str().ok_or("workflows/run: missing 'name'")?;
    let arguments = params.get("arguments").cloned().unwrap_or(serde_json::json!({}));
    let ctx = ExecutionContext {
        env:     serve.state.config.default_env.clone(),
        cwd:     serve.state.config.workspace_root.clone().unwrap_or_else(|| std::path::PathBuf::from(".")),
        user:    "agent".to_string(),
        profile: serve.state.config.active_profiles.first().cloned().unwrap_or_else(|| "default".to_string()),
        approver: None,
    };
    let serve_clone = Arc::clone(serve);
    let name = name.to_string();
    let outcomes = tokio::task::spawn_blocking(move || {
        let store = serve_clone.store.lock().unwrap();
        run_workflow(&serve_clone.cap_registry, &serve_clone.wf_registry, &serve_clone.policy, serve_clone.state.config.infisical.as_ref(), &store, serve_clone.worker_registry.as_ref(), &name, arguments, ctx)
    }).await.map_err(|e| format!("task join: {e}"))?.map_err(|e| e.to_string())?;
    Ok(serde_json::json!({"outcomes": outcomes}))
}

pub fn onboard_probe(params: &serde_json::Value) -> MethodResult {
    let name    = params["name"].as_str().ok_or("onboard/probe: missing 'name'")?;
    let command = params["command"].as_str().ok_or("onboard/probe: missing 'command'")?;
    let tmp = std::env::temp_dir().join(format!("clix-onboard-{name}"));
    std::fs::create_dir_all(&tmp).map_err(|e| e.to_string())?;
    let report = onboard_cli(name, command, &tmp).map_err(|e| e.to_string())?;
    serde_json::to_value(report).map_err(|e| e.to_string())
}

pub fn packs_list(serve: &Arc<ServeState>) -> MethodResult {
    let mut packs = vec![];
    if serve.state.packs_dir.exists() {
        if let Ok(entries) = std::fs::read_dir(&serve.state.packs_dir) {
            for entry in entries.flatten() {
                if !entry.file_type().map(|t| t.is_dir()).unwrap_or(false) { continue; }
                let pack_file = entry.path().join("pack.yaml");
                if pack_file.exists() {
                    if let Ok(content) = std::fs::read_to_string(&pack_file) {
                        if let Ok(p) = serde_yaml::from_str::<clix_core::manifest::pack::PackManifest>(&content) {
                            packs.push(serde_json::json!({"name":p.name,"version":p.version,"description":p.description}));
                        }
                    }
                }
            }
        }
    }
    packs.sort_by(|a, b| a["name"].as_str().cmp(&b["name"].as_str()));
    Ok(serde_json::json!({"packs": packs}))
}

pub fn status_get(serve: &Arc<ServeState>) -> MethodResult {
    Ok(serde_json::json!({
        "home":            serve.state.home,
        "activeProfiles":  serve.state.config.active_profiles,
        "defaultEnv":      serve.state.config.default_env,
        "approvalMode":    format!("{:?}", serve.state.config.approval_mode),
        "sandboxEnforced": sandbox_enforced(),
        "capabilityCount": serve.cap_registry.all().len(),
        "workflowCount":   serve.wf_registry.all().len(),
    }))
}
