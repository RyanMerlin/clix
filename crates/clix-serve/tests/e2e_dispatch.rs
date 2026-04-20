/// Test 6 — End-to-end dispatch verification.
///
/// Exercises the full `tools/call` → policy → execution → receipt pipeline without
/// starting a real OS process (uses the `builtin` backend so the test is hermetic).
///
/// What this proves:
/// (a) A `readonly` profile can call a `ReadOnly` side-effect capability → success.
/// (b) A `deny` policy rule blocks execution → `isError: true`, receipt has status=denied.
/// (c) A `require_approval` rule blocks without approver → `approvalRequired: true`.
/// (d) Switching to a different profile (via ctx) that has no deny rule → success.
/// (e) Receipts are written with the correct status and capability name.
use std::sync::{Arc, Mutex};
use clix_core::manifest::capability::{Backend, CapabilityManifest, IsolationTier, RiskLevel, SideEffectClass};
use clix_core::policy::{PolicyAction, PolicyBundle, PolicyRule};
use clix_core::receipts::ReceiptStore;
use clix_core::registry::{CapabilityRegistry, WorkflowRegistry};
use clix_core::state::ClixState;
use clix_serve::dispatch::ServeState;
use clix_serve::transport::stdio::process_line;

// ── Helpers ───────────────────────────────────────────────────────────────────

fn make_cap(name: &str, side_effect: SideEffectClass) -> CapabilityManifest {
    CapabilityManifest {
        name:             name.to_string(),
        version:          1,
        description:      Some(format!("Test capability {name}")),
        backend:          Backend::Builtin { name: "date".to_string() },
        risk:             RiskLevel::Low,
        side_effect_class: side_effect,
        sandbox_profile:  None,
        isolation:        IsolationTier::None,
        approval_policy:  None,
        input_schema:     serde_json::json!({"type":"object","properties":{}}),
        validators:       vec![],
        credentials:      vec![],
        argv_pattern:     None,
    }
}

fn make_state(caps: Vec<CapabilityManifest>, policy: PolicyBundle) -> Arc<ServeState> {
    let id = std::time::SystemTime::now().duration_since(std::time::UNIX_EPOCH).unwrap().subsec_nanos();
    let home = std::env::temp_dir().join(format!("clix-e2e-{id}"));
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

async fn call(serve: &Arc<ServeState>, name: &str) -> serde_json::Value {
    let req = serde_json::json!({
        "jsonrpc": "2.0", "id": 1,
        "method": "tools/call",
        "params": {"name": name, "arguments": {}}
    });
    let line = serde_json::to_string(&req).unwrap();
    let resp_line = process_line(Arc::clone(serve), &line).await.unwrap();
    serde_json::from_str(&resp_line).unwrap()
}

// ── Tests ─────────────────────────────────────────────────────────────────────

/// (a) A read-only capability executes successfully with a permissive policy.
#[tokio::test]
async fn test_readonly_cap_succeeds() {
    let caps = vec![make_cap("sys.date", SideEffectClass::ReadOnly)];
    let serve = make_state(caps, PolicyBundle::allow_all());

    let resp = call(&serve, "sys.date").await;
    assert!(!resp["result"]["isError"].as_bool().unwrap_or(true),
        "expected success, got: {resp}");
    assert!(resp["result"]["content"][0]["text"].as_str().is_some(),
        "expected text content in result");
}

/// (b) A deny policy rule blocks execution; receipt is written with status=denied.
#[tokio::test]
async fn test_deny_policy_blocks() {
    let caps = vec![make_cap("gcloud.projects.list", SideEffectClass::ReadOnly)];
    let mut policy = PolicyBundle::allow_all();
    policy.rules.push(PolicyRule {
        capability: Some("gcloud.projects.list".to_string()),
        action:     PolicyAction::Deny,
        reason:     Some("forbidden in this profile".to_string()),
        ..Default::default()
    });
    let serve = make_state(caps, policy);

    let resp = call(&serve, "gcloud.projects.list").await;
    assert!(resp["result"]["isError"].as_bool().unwrap_or(false),
        "expected isError=true, got: {resp}");

    // Receipt should exist with status denied
    let store = serve.store.lock().unwrap();
    let receipts = store.list(10, Some("denied")).unwrap();
    assert!(!receipts.is_empty(), "expected denied receipt");
    assert_eq!(receipts[0].capability, "gcloud.projects.list");
}

/// (c) A require_approval rule blocks without an approver; approvalRequired=true.
#[tokio::test]
async fn test_require_approval_blocks() {
    let caps = vec![make_cap("gcloud.compute.instances.delete", SideEffectClass::Destructive)];
    let mut policy = PolicyBundle::allow_all();
    policy.rules.push(PolicyRule {
        capability: Some("gcloud.compute.instances.delete".to_string()),
        action:     PolicyAction::RequireApproval,
        reason:     Some("destructive operation requires approval".to_string()),
        ..Default::default()
    });
    let serve = make_state(caps, policy);

    let resp = call(&serve, "gcloud.compute.instances.delete").await;
    assert!(resp["result"]["isError"].as_bool().unwrap_or(false),
        "expected isError=true for unapproved destructive op, got: {resp}");
    assert!(resp["result"]["_clix"]["approvalRequired"].as_bool().unwrap_or(false),
        "expected approvalRequired=true, got: {resp}");
}

/// (d) Same capability succeeds when there is no deny rule (e.g. a write-capable profile).
#[tokio::test]
async fn test_write_profile_allows_mutating_cap() {
    // Mutating cap, but NO deny rule — should succeed
    let caps = vec![make_cap("gcloud.compute.instances.list", SideEffectClass::Mutating)];
    let serve = make_state(caps, PolicyBundle::allow_all());

    let resp = call(&serve, "gcloud.compute.instances.list").await;
    assert!(!resp["result"]["isError"].as_bool().unwrap_or(true),
        "expected success with no deny rule, got: {resp}");
}

/// (e) Side-effect-based policy rule: deny Mutating caps by side-effect class.
#[tokio::test]
async fn test_side_effect_policy_denies_mutating_caps() {
    let caps = vec![
        make_cap("gcloud.compute.instances.list",   SideEffectClass::ReadOnly),
        make_cap("gcloud.compute.instances.create", SideEffectClass::Mutating),
    ];
    let mut policy = PolicyBundle::allow_all();
    // Deny anything with side_effect_class = Mutating
    policy.rules.push(PolicyRule {
        side_effect_class: Some(SideEffectClass::Mutating),
        action:            PolicyAction::Deny,
        reason:            Some("readonly profile disallows mutating operations".to_string()),
        ..Default::default()
    });
    let serve = make_state(caps, policy);

    // ReadOnly cap → succeeds
    let resp_ro = call(&serve, "gcloud.compute.instances.list").await;
    assert!(!resp_ro["result"]["isError"].as_bool().unwrap_or(true),
        "ReadOnly cap should succeed");

    // Mutating cap → denied
    let resp_wr = call(&serve, "gcloud.compute.instances.create").await;
    assert!(resp_wr["result"]["isError"].as_bool().unwrap_or(false),
        "Mutating cap should be denied, got: {resp_wr}");
}

/// (f) Receipt is written with the correct capability name and status for a successful call.
#[tokio::test]
async fn test_receipt_written_on_success() {
    let caps = vec![make_cap("sys.date", SideEffectClass::None)];
    let serve = make_state(caps, PolicyBundle::allow_all());

    let resp = call(&serve, "sys.date").await;
    assert!(!resp["result"]["isError"].as_bool().unwrap_or(true));

    let store = serve.store.lock().unwrap();
    let receipts = store.list(10, None).unwrap();
    assert!(!receipts.is_empty(), "receipt should have been written");
    let r = receipts.iter().find(|r| r.capability == "sys.date").expect("sys.date receipt");
    assert_eq!(format!("{:?}", r.status).to_lowercase(), "succeeded");
}

/// (g) M4 gate: unmatched capability with the default policy → Denied (fail-closed).
#[tokio::test]
async fn test_unmatched_cap_denied_by_default_policy() {
    let caps = vec![make_cap("sys.date", SideEffectClass::None)];
    // PolicyBundle::default() has default_action=Deny — no rules → everything denied
    let serve = make_state(caps, PolicyBundle::default());

    let resp = call(&serve, "sys.date").await;
    assert!(resp["result"]["isError"].as_bool().unwrap_or(false),
        "expected Denied with default policy, got: {resp}");

    let store = serve.store.lock().unwrap();
    let denied = store.list(10, Some("denied")).unwrap();
    assert!(!denied.is_empty(), "expected a denied receipt in the store");
}
