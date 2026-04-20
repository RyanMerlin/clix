//! Dispatch matrix test: cross-product of PolicyAction × backend × secrets.
//!
//! Uses rstest parametrize over {Allow, Deny, RequireApproval} scenarios.
//! All use the `builtin` backend (hermetic) with and without a policy override.

use std::sync::Arc;
use clix_testkit::capability::{builtin, with_side_effect};
use clix_testkit::serve::{call, make_state};
use clix_testkit::{PolicyAction, PolicyBundle, PolicyRule, SideEffectClass};
use rstest::rstest;

// ─── Allow paths ─────────────────────────────────────────────────────────────

/// A ReadOnly capability with default policy → succeeds.
#[tokio::test]
async fn test_allow_readonly() {
    let serve = make_state(vec![builtin("sys.date")], PolicyBundle::allow_all());
    let resp = call(&serve, "sys.date").await;
    assert!(!resp["result"]["isError"].as_bool().unwrap_or(true), "expected success: {resp}");
}

/// A Mutating capability with no deny rule → succeeds.
#[tokio::test]
async fn test_allow_mutating_no_rule() {
    let serve = make_state(
        vec![with_side_effect("k8s.pods.delete", SideEffectClass::Mutating)],
        PolicyBundle::allow_all(),
    );
    let resp = call(&serve, "k8s.pods.delete").await;
    assert!(!resp["result"]["isError"].as_bool().unwrap_or(true), "expected success: {resp}");
}

// ─── Deny paths ──────────────────────────────────────────────────────────────

/// Deny by capability name.
#[tokio::test]
async fn test_deny_by_name() {
    let mut policy = PolicyBundle::allow_all();
    policy.rules.push(PolicyRule {
        capability: Some("gcloud.projects.list".to_string()),
        action:     PolicyAction::Deny,
        reason:     Some("blocked".to_string()),
        ..Default::default()
    });
    let serve = make_state(
        vec![builtin("gcloud.projects.list")],
        policy,
    );
    let resp = call(&serve, "gcloud.projects.list").await;
    assert!(resp["result"]["isError"].as_bool().unwrap_or(false), "expected denied: {resp}");
}

/// Deny by side_effect_class (Mutating).
#[tokio::test]
async fn test_deny_by_side_effect() {
    let mut policy = PolicyBundle::allow_all();
    policy.rules.push(PolicyRule {
        side_effect_class: Some(SideEffectClass::Mutating),
        action:            PolicyAction::Deny,
        reason:            Some("readonly profile".to_string()),
        ..Default::default()
    });
    let serve = make_state(
        vec![
            with_side_effect("k8s.pods.list",   SideEffectClass::ReadOnly),
            with_side_effect("k8s.pods.delete", SideEffectClass::Mutating),
        ],
        policy,
    );
    let ro_resp = call(&serve, "k8s.pods.list").await;
    assert!(!ro_resp["result"]["isError"].as_bool().unwrap_or(true), "ReadOnly should succeed");
    let mt_resp = call(&serve, "k8s.pods.delete").await;
    assert!(mt_resp["result"]["isError"].as_bool().unwrap_or(false), "Mutating should be denied");
}

/// Deny Destructive side_effect_class.
#[tokio::test]
async fn test_deny_destructive() {
    let mut policy = PolicyBundle::allow_all();
    policy.rules.push(PolicyRule {
        side_effect_class: Some(SideEffectClass::Destructive),
        action:            PolicyAction::Deny,
        reason:            Some("destructive ops forbidden".to_string()),
        ..Default::default()
    });
    let serve = make_state(
        vec![with_side_effect("db.drop", SideEffectClass::Destructive)],
        policy,
    );
    let resp = call(&serve, "db.drop").await;
    assert!(resp["result"]["isError"].as_bool().unwrap_or(false), "expected denied: {resp}");
}

// ─── RequireApproval paths ────────────────────────────────────────────────────

/// RequireApproval without a connected approver → approvalRequired=true, isError=true.
#[rstest]
#[case("k8s.deploy.rollout", SideEffectClass::Mutating)]
#[case("gcloud.compute.instances.delete", SideEffectClass::Destructive)]
#[tokio::test]
async fn test_require_approval_blocks(
    #[case] cap_name: &str,
    #[case] side_effect: SideEffectClass,
) {
    let mut policy = PolicyBundle::allow_all();
    policy.rules.push(PolicyRule {
        capability: Some(cap_name.to_string()),
        action:     PolicyAction::RequireApproval,
        reason:     Some("requires human sign-off".to_string()),
        ..Default::default()
    });
    let serve = make_state(
        vec![with_side_effect(cap_name, side_effect)],
        policy,
    );
    let resp = call(&serve, cap_name).await;
    assert!(resp["result"]["isError"].as_bool().unwrap_or(false), "expected blocked: {resp}");
    assert!(
        resp["result"]["_clix"]["approvalRequired"].as_bool().unwrap_or(false),
        "expected approvalRequired=true: {resp}"
    );
}

// ─── Unknown capability ───────────────────────────────────────────────────────

/// Calling an unregistered capability returns a JSON-RPC error.
#[tokio::test]
async fn test_unknown_capability_returns_error() {
    let serve = make_state(vec![], PolicyBundle::allow_all());
    let resp = call(&serve, "does.not.exist").await;
    // Either a JSON-RPC -32000 error or isError in the result
    let is_error = resp.get("error").is_some()
        || resp["result"]["isError"].as_bool().unwrap_or(false);
    assert!(is_error, "unknown capability should produce an error: {resp}");
}

// ─── Receipt written on success ───────────────────────────────────────────────

/// Successful call writes a receipt with status=succeeded.
#[tokio::test]
async fn test_receipt_on_success() {
    let serve = make_state(vec![builtin("sys.date")], PolicyBundle::allow_all());
    let resp = call(&serve, "sys.date").await;
    assert!(!resp["result"]["isError"].as_bool().unwrap_or(true));
    let store = serve.store.lock().unwrap();
    let receipts = store.list(10, None).unwrap();
    let r = receipts.iter().find(|r| r.capability == "sys.date").expect("receipt");
    assert_eq!(format!("{:?}", r.status).to_lowercase(), "succeeded");
}

/// Denied call writes a receipt with status=denied.
#[tokio::test]
async fn test_receipt_on_deny() {
    let mut policy = PolicyBundle::allow_all();
    policy.rules.push(PolicyRule {
        capability: Some("ops.nuke".to_string()),
        action:     PolicyAction::Deny,
        reason:     None,
        ..Default::default()
    });
    let serve = make_state(vec![builtin("ops.nuke")], policy);
    let _ = call(&serve, "ops.nuke").await;
    let store = serve.store.lock().unwrap();
    let receipts = store.list(10, Some("denied")).unwrap();
    assert!(!receipts.is_empty(), "denied receipt should exist");
}
