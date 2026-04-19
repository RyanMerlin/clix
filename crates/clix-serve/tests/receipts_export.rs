//! Receipt export and store tests.
//!
//! Tests the `ReceiptStore::export` API and verifies that receipts written
//! through the dispatch pipeline can be filtered and exported correctly.

use std::sync::{Arc, Mutex};
use clix_testkit::capability::builtin;
use clix_testkit::serve::{call, make_state};
use clix_testkit::{PolicyAction, PolicyBundle, PolicyRule, ReceiptStore};

// ─── export() filter tests ────────────────────────────────────────────────────

/// Receipts can be exported without filter (all statuses).
#[tokio::test]
async fn test_export_all() {
    let serve = make_state(vec![builtin("sys.date")], PolicyBundle::default());
    let _ = call(&serve, "sys.date").await;
    let store = serve.store.lock().unwrap();
    let all = store.export(None, None).unwrap();
    assert_eq!(all.len(), 1);
    assert_eq!(all[0].capability, "sys.date");
}

/// Export with status filter returns only matching receipts.
#[tokio::test]
async fn test_export_status_filter() {
    let mut policy = PolicyBundle::default();
    policy.rules.push(PolicyRule {
        capability: Some("ops.blocked".to_string()),
        action: PolicyAction::Deny,
        reason: None,
        ..Default::default()
    });
    let serve = make_state(
        vec![builtin("sys.date"), builtin("ops.blocked")],
        policy,
    );
    let _ = call(&serve, "sys.date").await;
    let _ = call(&serve, "ops.blocked").await;

    let store = serve.store.lock().unwrap();
    let succeeded = store.export(Some("succeeded"), None).unwrap();
    assert_eq!(succeeded.len(), 1, "only one succeeded receipt");
    assert_eq!(succeeded[0].capability, "sys.date");

    let denied = store.export(Some("denied"), None).unwrap();
    assert_eq!(denied.len(), 1, "only one denied receipt");
    assert_eq!(denied[0].capability, "ops.blocked");
}

/// Exported receipts are in ascending chronological order.
#[tokio::test]
async fn test_export_ascending_order() {
    let serve = make_state(vec![builtin("sys.date")], PolicyBundle::default());
    for _ in 0..3 {
        let _ = call(&serve, "sys.date").await;
    }
    let store = serve.store.lock().unwrap();
    let all = store.export(None, None).unwrap();
    assert_eq!(all.len(), 3);
    for w in all.windows(2) {
        assert!(w[0].created_at <= w[1].created_at, "should be ascending");
    }
}

/// Export serializes to valid JSON.
#[tokio::test]
async fn test_export_json_serializable() {
    let serve = make_state(vec![builtin("sys.date")], PolicyBundle::default());
    let _ = call(&serve, "sys.date").await;
    let store = serve.store.lock().unwrap();
    let all = store.export(None, None).unwrap();
    // Must serialize without error
    let json_array = serde_json::to_string(&all).expect("serialization should succeed");
    assert!(json_array.starts_with('['));

    // Each receipt must also serialize as a single JSONL line (no embedded newlines in fields that matter)
    for r in &all {
        let line = serde_json::to_string(r).expect("single receipt serialization");
        assert!(!line.is_empty());
    }
}

/// count_by_status reflects the actual stored receipts.
#[tokio::test]
async fn test_count_by_status() {
    let mut policy = PolicyBundle::default();
    policy.rules.push(PolicyRule {
        capability: Some("bad.op".to_string()),
        action: PolicyAction::Deny,
        reason: None,
        ..Default::default()
    });
    let serve = make_state(
        vec![builtin("sys.date"), builtin("bad.op")],
        policy,
    );
    let _ = call(&serve, "sys.date").await;
    let _ = call(&serve, "bad.op").await;

    let store = serve.store.lock().unwrap();
    let (total, succeeded, denied, _failed, _pending) = store.count_by_status().unwrap();
    assert_eq!(total, 2);
    assert_eq!(succeeded, 1);
    assert_eq!(denied, 1);
}
