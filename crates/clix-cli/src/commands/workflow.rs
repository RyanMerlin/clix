use crate::commands::run::{build_worker_registry, parse_input_pairs};
use crate::output::print_json;
use anyhow::{anyhow, Result};
use clix_core::execution::run_workflow;
use clix_core::loader::{
    active_profile_bindings, build_registry, build_workflow_registry, load_policy,
};
use clix_core::policy::evaluate::ExecutionContext;
use clix_core::receipts::ReceiptStore;
use clix_core::state::{home_dir, ClixState};

pub fn list(json: bool) -> Result<()> {
    let state = ClixState::load(home_dir())?;
    let registry = build_workflow_registry(&state)?;
    let wfs: Vec<_> = registry.all().into_iter().collect();
    if json {
        print_json(&wfs);
    } else {
        for wf in &wfs {
            println!(
                "{:<40} {}",
                wf.name,
                wf.description.as_deref().unwrap_or("")
            );
        }
    }
    Ok(())
}

pub fn run_wf(name: &str, input_pairs: &[String], json: bool) -> Result<()> {
    let state = ClixState::load(home_dir())?;
    let cap_reg = build_registry(&state)?;
    let wf_reg = build_workflow_registry(&state)?;
    let policy = load_policy(&state)?;
    let store = ReceiptStore::open(&state.receipts_db)?;
    let input = parse_input_pairs(input_pairs)?;
    let (profile_secret_bindings, profile_folder_bindings) = active_profile_bindings(&state)?;
    let worker_registry = build_worker_registry(std::env::var("CLIX_ALLOW_UNSANDBOXED").is_ok())?;
    let ctx = ExecutionContext {
        env: state.config.default_env.clone(),
        cwd: state.config.workspace_root.clone().unwrap_or_else(|| {
            std::env::current_dir().unwrap_or_else(|_| std::path::PathBuf::from("."))
        }),
        user: whoami::username(),
        profile: state
            .config
            .active_profiles
            .first()
            .cloned()
            .unwrap_or_else(|| "default".to_string()),
        approver: None,
    };
    let outcomes = run_workflow(
        &cap_reg,
        &wf_reg,
        &policy,
        &state.config.infisical(),
        &store,
        worker_registry.as_ref(),
        name,
        input,
        ctx,
        &profile_secret_bindings,
        &profile_folder_bindings,
    )
    .map_err(|e| anyhow!("{e}"))?;
    if json {
        print_json(&outcomes);
    } else {
        for (i, o) in outcomes.iter().enumerate() {
            println!(
                "step {}: {} — receipt {}",
                i + 1,
                if o.ok { "ok" } else { "failed" },
                o.receipt_id
            );
        }
    }
    Ok(())
}
