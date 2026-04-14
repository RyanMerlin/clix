/// Test 3 — Binary pinning and hash verification.
///
/// Verifies that:
/// (a) `resolve_and_hash_binary` finds a known binary and returns a 64-char hex SHA-256.
/// (b) `verify_binary_hash` passes when the hash is correct.
/// (c) `verify_binary_hash` returns `IntegrityFailure` when the hash is wrong.
/// (d) Swapping the binary content (simulate a supply-chain tamper) causes a mismatch.
use clix_core::sandbox::jail::{resolve_and_hash_binary, verify_binary_hash};
use clix_core::error::ClixError;
use std::io::Write;

#[test]
fn test_resolve_and_hash_known_binary() {
    let (path, hash) = resolve_and_hash_binary("true").expect("resolve true");
    assert!(path.is_absolute(), "path should be absolute");
    assert!(path.exists(), "binary should exist");
    assert_eq!(hash.len(), 64, "SHA-256 hex should be 64 chars");
    // All hex digits
    assert!(hash.chars().all(|c| c.is_ascii_hexdigit()), "hash should be hex");
}

#[test]
fn test_verify_hash_correct() {
    let (path, hash) = resolve_and_hash_binary("true").expect("resolve true");
    verify_binary_hash(&path, &hash).expect("correct hash should pass");
}

#[test]
fn test_verify_hash_wrong_hash_returns_integrity_failure() {
    let (path, _) = resolve_and_hash_binary("true").expect("resolve true");
    let bad_hash = "a".repeat(64);
    let err = verify_binary_hash(&path, &bad_hash).expect_err("wrong hash should fail");
    assert!(
        matches!(err, ClixError::IntegrityFailure(_)),
        "expected IntegrityFailure, got: {err}"
    );
}

#[test]
fn test_verify_hash_nonexistent_binary() {
    use std::path::PathBuf;
    let err = verify_binary_hash(
        &PathBuf::from("/nonexistent/binary/that/does/not/exist"),
        &"a".repeat(64),
    ).expect_err("nonexistent binary should fail");
    assert!(
        matches!(err, ClixError::Isolation(_)),
        "expected Isolation error, got: {err}"
    );
}

#[test]
fn test_tampered_binary_detected() {
    // Write a known binary to a temp file, hash it, then modify it.
    // Simulates a supply-chain substitution (binary swapped after registration).
    let mut tmp = tempfile::NamedTempFile::new().expect("tempfile");
    tmp.write_all(b"#!/bin/sh\necho original\n").expect("write");
    tmp.flush().expect("flush");

    let path = tmp.path().to_path_buf();
    let (_, original_hash) = resolve_and_hash_binary(path.to_str().unwrap())
        .expect("hash original");

    // Overwrite with different content ("tampered")
    let mut f = std::fs::OpenOptions::new().write(true).truncate(true).open(&path).expect("open for write");
    f.write_all(b"#!/bin/sh\necho tampered\n").expect("write tampered");
    drop(f);

    let err = verify_binary_hash(&path, &original_hash)
        .expect_err("tampered binary should fail");
    assert!(
        matches!(err, ClixError::IntegrityFailure(_)),
        "expected IntegrityFailure after tamper, got: {err}"
    );
}

#[test]
fn test_resolve_unknown_binary_returns_error() {
    let err = resolve_and_hash_binary("this_binary_definitely_does_not_exist_clix_test")
        .expect_err("unknown binary should fail");
    assert!(
        matches!(err, ClixError::Isolation(_)),
        "expected Isolation error, got: {err}"
    );
}
