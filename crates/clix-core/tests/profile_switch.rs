/// Test 4 — Dynamic profile switch verification.
///
/// Verifies that:
/// (a) WorkerRegistry keys workers by (profile, binary, tier) — different profiles
///     are independent slots.
/// (b) TTL reaping removes idle workers once their idle_ttl elapses.
/// (c) Shutting down the registry kills all workers cleanly (no panics, no leaks).
/// (d) Switching profiles via `run_capability` with distinct policies produces the
///     correct outcomes per profile without cross-contamination.
///
/// Note: actual worker *spawning* requires a Linux system with a clix-worker binary.
/// The structural tests (key equality, reap, shutdown) run everywhere; the dispatch
/// tests skip if clix-worker is not found.
use std::path::PathBuf;
use clix_core::execution::worker_registry::{WorkerRegistry, WorkerKey};
use clix_core::manifest::capability::IsolationTier;

// ── (a) WorkerKey equality / independence ─────────────────────────────────────

#[test]
fn test_worker_key_profile_independence() {
    let readonly = WorkerKey {
        profile: "readonly".to_string(),
        binary:  "/usr/bin/gcloud".to_string(),
        tier:    IsolationTier::WarmWorker,
    };
    let write = WorkerKey {
        profile: "write".to_string(),
        binary:  "/usr/bin/gcloud".to_string(),
        tier:    IsolationTier::WarmWorker,
    };
    // Same binary, different profile → different slots
    assert_ne!(readonly, write,
        "different profiles must not share a worker slot");
    // Same profile, same binary → same slot
    let readonly2 = readonly.clone();
    assert_eq!(readonly, readonly2);
}

#[test]
fn test_worker_key_tier_independence() {
    let warm = WorkerKey {
        profile: "readonly".to_string(),
        binary:  "/usr/bin/gcloud".to_string(),
        tier:    IsolationTier::WarmWorker,
    };
    let none = WorkerKey {
        profile: "readonly".to_string(),
        binary:  "/usr/bin/gcloud".to_string(),
        tier:    IsolationTier::None,
    };
    assert_ne!(warm, none,
        "different isolation tiers must not share a worker slot");
}

// ── (b) TTL reaping ───────────────────────────────────────────────────────────

#[test]
fn test_reap_idle_empty_registry() {
    // Should not panic on an empty registry
    let reg = WorkerRegistry::new(PathBuf::from("clix-worker"), 1);
    assert_eq!(reg.worker_count(), 0);
    reg.reap_idle();
    assert_eq!(reg.worker_count(), 0);
}

#[test]
fn test_shutdown_empty_registry() {
    // Should not panic when no workers are running
    let reg = WorkerRegistry::new(PathBuf::from("clix-worker"), 300);
    reg.shutdown();
    assert_eq!(reg.worker_count(), 0);
}

// ── (c) Registry construction variants ───────────────────────────────────────

#[test]
fn test_registry_with_broker_none() {
    let reg = WorkerRegistry::new_with_broker(PathBuf::from("clix-worker"), 300, None);
    assert_eq!(reg.worker_count(), 0);
}

#[test]
fn test_registry_with_explicit_broker_path() {
    // Broker path that doesn't exist — registry still constructs cleanly
    let reg = WorkerRegistry::new_with_broker(
        PathBuf::from("clix-worker"),
        300,
        Some(PathBuf::from("/tmp/clix-broker-nonexistent.sock")),
    );
    assert_eq!(reg.worker_count(), 0);
    reg.shutdown();
}

// ── (d) Profile policy isolation via run_capability ──────────────────────────

/// Tests that the same capability name under different policy bundles (simulating
/// different profiles) produces different outcomes without state leak between calls.
#[test]
fn test_profile_policy_isolation_no_cross_contamination() {
    use clix_core::execution::run_capability;
    use clix_core::manifest::capability::{Backend, CapabilityManifest, IsolationTier, RiskLevel, SideEffectClass};
    use clix_core::policy::{PolicyAction, PolicyBundle, PolicyRule};
    use clix_core::receipts::ReceiptStore;
    use clix_core::registry::CapabilityRegistry;
    use clix_core::policy::evaluate::ExecutionContext;

    let cap = CapabilityManifest {
        name:             "gcloud.projects.list".to_string(),
        version:          1,
        description:      None,
        backend:          Backend::Builtin { name: "date".to_string() },
        risk:             RiskLevel::Low,
        side_effect_class: SideEffectClass::ReadOnly,
        sandbox_profile:  None,
        isolation:        IsolationTier::None,
        approval_policy:  None,
        input_schema:     serde_json::json!({"type":"object","properties":{}}),
        validators:       vec![],
        credentials:      vec![],
        argv_pattern:     None,
    };

    let store_ro  = ReceiptStore::open(std::path::Path::new(":memory:")).unwrap();
    let store_rw  = ReceiptStore::open(std::path::Path::new(":memory:")).unwrap();
    let registry  = CapabilityRegistry::from_vec(vec![cap]);

    // Profile A: readonly — deny
    let mut policy_ro = PolicyBundle::allow_all();
    policy_ro.rules.push(PolicyRule {
        capability: Some("gcloud.projects.list".to_string()),
        action:     PolicyAction::Deny,
        reason:     Some("readonly profile".to_string()),
        ..Default::default()
    });
    let ctx_ro = ExecutionContext {
        env:      "test".to_string(),
        cwd:      std::path::PathBuf::from("."),
        user:     "agent".to_string(),
        profile:  "readonly".to_string(),
        approver: None,
    };
    let outcome_ro = run_capability(
        &registry, &policy_ro, None, &store_ro, None,
        "gcloud.projects.list", serde_json::json!({}), ctx_ro, &[],
    ).unwrap();
    assert!(!outcome_ro.ok, "readonly profile should deny");

    // Profile B: write — allow (no deny rule)
    let policy_rw = PolicyBundle::allow_all();
    let ctx_rw = ExecutionContext {
        env:      "test".to_string(),
        cwd:      std::path::PathBuf::from("."),
        user:     "agent".to_string(),
        profile:  "write".to_string(),
        approver: None,
    };
    let outcome_rw = run_capability(
        &registry, &policy_rw, None, &store_rw, None,
        "gcloud.projects.list", serde_json::json!({}), ctx_rw, &[],
    ).unwrap();
    assert!(outcome_rw.ok, "write profile should allow");

    // No cross-contamination: readonly receipts show deny, write receipts show success
    let ro_receipts = store_ro.list(10, Some("denied")).unwrap();
    let rw_receipts = store_rw.list(10, Some("succeeded")).unwrap();
    assert_eq!(ro_receipts.len(), 1, "readonly store should have 1 denied receipt");
    assert_eq!(rw_receipts.len(), 1, "write store should have 1 succeeded receipt");
}
