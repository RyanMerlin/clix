pub mod approval;
pub mod backends;
pub mod broker_client;
pub mod validators;
pub mod worker_protocol;
pub mod worker_registry;

use serde::{Deserialize, Serialize};
use uuid::Uuid;
use chrono::Utc;
use crate::error::{ClixError, Result};
use crate::manifest::capability::Backend;
use crate::manifest::workflow::StepFailurePolicy;
use crate::policy::{evaluate_policy, evaluate::ExecutionContext, Decision, PolicyBundle};
use crate::receipts::{Receipt, ReceiptKind, ReceiptStatus, ReceiptStore};
use crate::registry::{CapabilityRegistry, WorkflowRegistry};
use crate::sandbox::sandbox_enforced;
use crate::schema::validate_input;
use crate::manifest::profile::ProfileSecretBinding;
use crate::secrets::{resolve_credentials, SecretRedactor};
use crate::state::InfisicalConfig;
use crate::template::render_args;
use backends::{builtin_handler, expand_secret_refs, run_isolated, run_remote, run_subprocess};
use sha2::Digest;
use validators::run_validators;
use worker_registry::WorkerRegistry;
use std::sync::Arc;
use tracing::warn;

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct ExecutionOutcome {
    pub ok: bool,
    pub approval_required: bool,
    pub receipt_id: Uuid,
    pub result: Option<serde_json::Value>,
    pub reason: Option<String>,
}

pub fn run_capability(registry: &CapabilityRegistry, policy: &PolicyBundle, infisical: Option<&InfisicalConfig>, store: &ReceiptStore, worker_registry: Option<&Arc<WorkerRegistry>>, name: &str, input: serde_json::Value, ctx: ExecutionContext, profile_bindings: &[ProfileSecretBinding]) -> Result<ExecutionOutcome> {
    let cap = registry.get(name).ok_or_else(|| ClixError::CapabilityNotFound(name.to_string()))?.clone();
    validate_input(&cap.input_schema, &input)?;
    let decision = evaluate_policy(policy, &ctx, &cap);
    let receipt_id = Uuid::new_v4();

    match &decision {
        Decision::Deny { reason } => {
            store.write(&Receipt { id: receipt_id, kind: ReceiptKind::Capability, capability: cap.name.clone(), created_at: Utc::now(), status: ReceiptStatus::Denied, decision: "deny".to_string(), reason: Some(reason.clone()), input: input.clone(), context: serde_json::to_value(&ctx).unwrap_or_default(), execution: None, approval: None, sandbox_enforced: sandbox_enforced(), isolation_tier: None, binary_sha256: None, token_mint_id: None, jail_config_digest: None })?;
            return Ok(ExecutionOutcome { ok: false, approval_required: false, receipt_id, result: None, reason: Some(reason.clone()) });
        }
        Decision::RequireApproval { reason } => {
            store.write(&Receipt { id: receipt_id, kind: ReceiptKind::Capability, capability: cap.name.clone(), created_at: Utc::now(), status: ReceiptStatus::PendingApproval, decision: "require_approval".to_string(), reason: Some(reason.clone()), input: input.clone(), context: serde_json::to_value(&ctx).unwrap_or_default(), execution: None, approval: None, sandbox_enforced: sandbox_enforced(), isolation_tier: None, binary_sha256: None, token_mint_id: None, jail_config_digest: None })?;
            // Try broker-based approval first
            if worker_registry.is_some() {
                let ctx_value = serde_json::to_value(&ctx).unwrap_or_default();
                match approval::wait_for_broker_approval(receipt_id, &cap.name, &input, &ctx_value, reason) {
                    Ok(outcome) => return Ok(outcome),
                    Err(e) => {
                        warn!(error = %e, "broker approval unavailable — returning pending status");
                    }
                }
            }
            return Ok(ExecutionOutcome { ok: false, approval_required: true, receipt_id, result: None, reason: Some(reason.clone()) });
        }
        Decision::Allow => {}
    }

    let template_ctx = serde_json::json!({"input": &input, "context": {"env": &ctx.env, "cwd": ctx.cwd.to_string_lossy(), "user": &ctx.user}});
    let rendered_args = match &cap.backend {
        Backend::Subprocess { args, .. } => render_args(args, &template_ctx)?,
        _ => vec![],
    };
    let val_errors = run_validators(&cap.validators, &input, &ctx.cwd, &rendered_args);
    if !val_errors.is_empty() {
        let reason = val_errors[0].clone();
        store.write(&Receipt { id: receipt_id, kind: ReceiptKind::Capability, capability: cap.name.clone(), created_at: Utc::now(), status: ReceiptStatus::Denied, decision: "deny".to_string(), reason: Some(reason.clone()), input: input.clone(), context: serde_json::to_value(&ctx).unwrap_or_default(), execution: None, approval: None, sandbox_enforced: sandbox_enforced(), isolation_tier: None, binary_sha256: None, token_mint_id: None, jail_config_digest: None })?;
        return Ok(ExecutionOutcome { ok: false, approval_required: false, receipt_id, result: None, reason: Some(reason) });
    }
    let secrets = resolve_credentials(&cap.credentials, infisical, profile_bindings, &[])?;
    let redactor = SecretRedactor::new(secrets.clone());
    let mut binary_sha256: Option<String> = None;
    let mut token_mint_id: Option<String> = None;
    let exec_result = match &cap.backend {
        Backend::Builtin { name } => builtin_handler(name, &input)?,
        Backend::Subprocess { command, cwd_from_input, .. } => {
            let cwd = if let Some(key) = cwd_from_input {
                input[key].as_str().map(std::path::PathBuf::from).unwrap_or_else(|| ctx.cwd.clone())
            } else { ctx.cwd.clone() };
            binary_sha256 = hash_binary(command);
            let expanded = expand_secret_refs(&rendered_args, &secrets);
            let (exit_code, stdout, stderr, isolation_tier) = if let Some(reg) = worker_registry {
                let dispatch = run_isolated(
                    &ctx.profile,
                    command,
                    &expanded,
                    &cwd,
                    &secrets,
                    &cap.isolation,
                    cap.sandbox_profile.as_ref(),
                    reg,
                    !cap.credentials.is_empty(),
                )?;
                token_mint_id = dispatch.token_mint_id.map(|u| u.to_string());
                (dispatch.exit_code, dispatch.stdout, dispatch.stderr, dispatch.isolation_tier)
            } else {
                let sub = run_subprocess(command, &expanded, &cwd, &secrets)?;
                (sub.exit_code, sub.stdout, sub.stderr, crate::manifest::capability::IsolationTier::None)
            };
            serde_json::json!({"exitCode": exit_code, "stdout": redactor.redact(&stdout), "stderr": redactor.redact(&stderr), "isolationTier": serde_json::to_value(&isolation_tier).unwrap_or_default()})
        }
        Backend::Remote { url } => {
            let addr = if url.is_empty() { std::env::var("CLIX_SOCKET").unwrap_or_default() } else { url.clone() };
            run_remote(&addr, &cap.name, &input)?
        }
    };
    let ok = exec_result["exitCode"].as_i64().unwrap_or(0) == 0;
    let status = if ok { ReceiptStatus::Succeeded } else { ReceiptStatus::Failed };
    let isolation_tier_str = exec_result["isolationTier"].as_str().map(String::from);
    store.write(&Receipt { id: receipt_id, kind: ReceiptKind::Capability, capability: cap.name.clone(), created_at: Utc::now(), status, decision: "allow".to_string(), reason: None, input: input.clone(), context: serde_json::to_value(&ctx).unwrap_or_default(), execution: Some(exec_result.clone()), approval: None, sandbox_enforced: sandbox_enforced(), isolation_tier: isolation_tier_str, binary_sha256, token_mint_id, jail_config_digest: None })?;
    Ok(ExecutionOutcome { ok, approval_required: false, receipt_id, result: Some(exec_result), reason: None })
}

pub fn run_workflow(cap_registry: &CapabilityRegistry, wf_registry: &WorkflowRegistry, policy: &PolicyBundle, infisical: Option<&InfisicalConfig>, store: &ReceiptStore, worker_registry: Option<&Arc<WorkerRegistry>>, name: &str, input: serde_json::Value, ctx: ExecutionContext) -> Result<Vec<ExecutionOutcome>> {
    let wf = wf_registry.get(name).ok_or_else(|| ClixError::WorkflowNotFound(name.to_string()))?.clone();
    let mut outcomes = vec![];
    for step in &wf.steps {
        let step_input = merge_inputs(&input, &step.input);
        let outcome = run_capability(cap_registry, policy, infisical, store, worker_registry, &step.capability, step_input, ctx.clone(), &[])?;
        let failed = !outcome.ok;
        outcomes.push(outcome);
        if failed { match step.on_failure { StepFailurePolicy::Abort => break, StepFailurePolicy::Continue => {} } }
    }
    Ok(outcomes)
}

fn hash_binary(command: &str) -> Option<String> {
    let path = which::which(command).ok()?;
    let bytes = std::fs::read(&path).ok()?;
    let hash = sha2::Sha256::digest(&bytes);
    Some(hex::encode(hash))
}

fn merge_inputs(base: &serde_json::Value, step: &serde_json::Value) -> serde_json::Value {
    match (base, step) {
        (serde_json::Value::Object(b), serde_json::Value::Object(s)) => {
            let mut merged = b.clone();
            for (k, v) in s { merged.insert(k.clone(), v.clone()); }
            serde_json::Value::Object(merged)
        }
        (_, s) if !s.is_null() => s.clone(),
        (b, _) => b.clone(),
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::manifest::capability::{Backend, CapabilityManifest, IsolationTier, RiskLevel, SideEffectClass};
    use crate::policy::{PolicyAction, PolicyBundle, PolicyRule};
    use std::path::PathBuf;

    fn allow_all() -> PolicyBundle {
        PolicyBundle { default_action: PolicyAction::Allow, ..Default::default() }
    }

    fn store() -> ReceiptStore { ReceiptStore::open(std::path::Path::new(":memory:")).unwrap() }

    fn ctx() -> ExecutionContext {
        ExecutionContext { env: "test".to_string(), cwd: PathBuf::from("."), user: "tester".to_string(), profile: "base".to_string(), approver: None }
    }

    fn date_cap() -> CapabilityManifest {
        CapabilityManifest { name: "sys.date".to_string(), version: 1, description: None, backend: Backend::Builtin { name: "date".to_string() }, risk: RiskLevel::Low, side_effect_class: SideEffectClass::None, sandbox_profile: None, isolation: Default::default(), approval_policy: None, input_schema: serde_json::json!({"type":"object","properties":{}}), validators: vec![], credentials: vec![], argv_pattern: None }
    }

    #[test]
    fn test_run_builtin() {
        let reg = CapabilityRegistry::from_vec(vec![date_cap()]);
        let outcome = run_capability(&reg, &allow_all(), None, &store(), None, "sys.date", serde_json::json!({}), ctx(), &[]).unwrap();
        assert!(outcome.ok);
    }

    #[test]
    fn test_unknown_capability_errors() {
        let reg = CapabilityRegistry::from_vec(vec![]);
        assert!(run_capability(&reg, &PolicyBundle::default(), None, &store(), None, "nope", serde_json::json!({}), ctx(), &[]).is_err());
    }

    #[test]
    fn test_denied_writes_receipt() {
        let reg = CapabilityRegistry::from_vec(vec![date_cap()]);
        let mut policy = PolicyBundle::default();
        policy.rules.push(PolicyRule { capability: Some("sys.date".to_string()), action: PolicyAction::Deny, reason: Some("test".to_string()), ..Default::default() });
        let store = store();
        let outcome = run_capability(&reg, &policy, None, &store, None, "sys.date", serde_json::json!({}), ctx(), &[]).unwrap();
        assert!(!outcome.ok);
        assert_eq!(store.list(10, Some("denied")).unwrap().len(), 1);
    }
}
