//! Receipt DB schema migration tests.
//!
//! Verifies that the current schema initializes correctly and that
//! all required columns are present. Future schema versions should
//! add assertions here.

use std::path::Path;
use tempfile::tempdir;
use clix_core::receipts::ReceiptStore;

/// Schema initializes on a fresh in-memory DB without errors.
#[test]
fn test_schema_init_in_memory() {
    let store = ReceiptStore::open(Path::new(":memory:"))
        .expect("in-memory store should initialize");
    // If we reach here, the schema SQL executed without error.
    let _ = store;
}

/// Schema initializes on a fresh on-disk DB without errors.
#[test]
fn test_schema_init_on_disk() {
    let tmp = tempdir().unwrap();
    let db_path = tmp.path().join("receipts.db");
    let store = ReceiptStore::open(&db_path).expect("on-disk store should initialize");
    let _ = store;
}

/// Opening an existing DB twice (schema re-applied) doesn't cause errors.
/// This simulates an upgrade scenario where schema is applied via CREATE TABLE IF NOT EXISTS.
#[test]
fn test_schema_idempotent() {
    let tmp = tempdir().unwrap();
    let db_path = tmp.path().join("receipts.db");
    {
        let _s = ReceiptStore::open(&db_path).unwrap();
    }
    // Open again — schema re-application should be idempotent
    let _s2 = ReceiptStore::open(&db_path).expect("second open should succeed");
}

/// A newly opened store starts empty.
#[test]
fn test_empty_store_list() {
    let store = ReceiptStore::open(Path::new(":memory:")).unwrap();
    let list = store.list(100, None).unwrap();
    assert!(list.is_empty(), "new store should have no receipts");
    let (total, ..) = store.count_by_status().unwrap();
    assert_eq!(total, 0);
}

/// Columns introduced in the extended schema (isolation_tier, binary_sha256, etc.) are present.
#[test]
fn test_extended_columns_present() {
    use uuid::Uuid;
    use chrono::Utc;
    use clix_core::receipts::{Receipt, ReceiptKind, ReceiptStatus};

    let store = ReceiptStore::open(Path::new(":memory:")).unwrap();
    let r = Receipt {
        id: Uuid::new_v4(),
        kind: ReceiptKind::Capability,
        capability: "test.cap".to_string(),
        created_at: Utc::now(),
        status: ReceiptStatus::Succeeded,
        decision: "allow".to_string(),
        reason: None,
        input: serde_json::json!({}),
        context: serde_json::json!({}),
        execution: None,
        approval: None,
        sandbox_enforced: true,
        isolation_tier: Some("warm_worker".to_string()),
        binary_sha256: Some("deadbeef".to_string()),
        token_mint_id: Some("mint-123".to_string()),
        jail_config_digest: Some("cafebabe".to_string()),
    };
    store.write(&r).expect("write receipt with extended columns");

    let list = store.list(1, None).unwrap();
    assert_eq!(list.len(), 1);
    assert_eq!(list[0].isolation_tier.as_deref(), Some("warm_worker"));
    assert_eq!(list[0].binary_sha256.as_deref(), Some("deadbeef"));
    assert_eq!(list[0].token_mint_id.as_deref(), Some("mint-123"));
    assert_eq!(list[0].jail_config_digest.as_deref(), Some("cafebabe"));
}
