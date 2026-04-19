//! Pack signing + bundle + verify integration tests.
//!
//! Covers:
//! (a) Generate a keypair.
//! (b) Bundle a pack without signing → .zip + .sha256 present, no .sig.
//! (c) Bundle a pack with signing → .zip + .sha256 + .sig + .fingerprint present.
//! (d) Sign, trust the key, verify via verify_signature → Ok.
//! (e) Tamper with the zip bytes → sha256 mismatch.
//! (f) Sign with key A, trust key B → verify_signature rejects.

use std::fs;
use std::path::Path;
use tempfile::tempdir;

use clix_core::packs::bundle::{bundle_pack, bundle_pack_signed};
use clix_core::packs::signing::{
    generate_keypair, key_fingerprint, trust_key, verify_signature, verifying_key_from_private,
};

// ─── Helpers ─────────────────────────────────────────────────────────────────

/// Minimal valid pack directory.
fn write_test_pack(dir: &Path) {
    fs::create_dir_all(dir.join("capabilities")).unwrap();
    fs::write(dir.join("pack.yaml"), concat!(
        "name: testpack\n",
        "version: \"1.0.0\"\n",
        "description: A test pack\n",
        "author: test\n",
    )).unwrap();
    fs::write(dir.join("capabilities").join("hello.yaml"), concat!(
        "name: testpack.hello\n",
        "version: 1\n",
        "description: hello\n",
        "backend:\n",
        "  type: builtin\n",
        "  name: date\n",
        "inputSchema: {\"type\":\"object\",\"properties\":{}}\n",
    )).unwrap();
}

/// Read the signature from a .sig sidecar file as a 64-byte array.
fn read_sig(zip_path: &Path) -> [u8; 64] {
    let sig_path = format!("{}.sig", zip_path.display());
    let hex_str = fs::read_to_string(&sig_path).unwrap();
    let bytes = hex::decode(hex_str.trim()).unwrap();
    let mut arr = [0u8; 64];
    arr.copy_from_slice(&bytes);
    arr
}

/// Read the sha256 of the zip (used as the signed payload).
fn zip_sha256(zip_path: &Path) -> [u8; 32] {
    use sha2::{Sha256, Digest};
    let data = fs::read(zip_path).unwrap();
    let mut h = Sha256::new();
    h.update(&data);
    h.finalize().into()
}

// ─── Tests ────────────────────────────────────────────────────────────────────

/// (a) Keypair generation creates private and public key files.
#[test]
fn test_generate_keypair() {
    let dir = tempdir().unwrap();
    let priv_path = dir.path().join("key.pem");
    let pub_path  = dir.path().join("key.pub");
    let fp = generate_keypair(&priv_path, &pub_path, false).unwrap();
    assert!(priv_path.exists(), "private key should exist");
    assert!(pub_path.exists(),  "public key should exist");
    assert_eq!(fp.len(), 16, "fingerprint should be 16 hex chars");
}

/// Generating twice without force fails.
#[test]
fn test_generate_keypair_no_overwrite() {
    let dir = tempdir().unwrap();
    let priv_path = dir.path().join("key.pem");
    let pub_path  = dir.path().join("key.pub");
    generate_keypair(&priv_path, &pub_path, false).unwrap();
    assert!(generate_keypair(&priv_path, &pub_path, false).is_err());
}

/// (b) Unsigned bundle creates .zip and .sha256, no .sig.
#[test]
fn test_bundle_unsigned() {
    let tmp = tempdir().unwrap();
    let pack_dir = tmp.path().join("pack");
    write_test_pack(&pack_dir);
    let out_dir = tmp.path().join("out");

    let zip_path = bundle_pack(&pack_dir, &out_dir).unwrap();
    assert!(zip_path.exists(), "zip should exist");
    let sha_path = zip_path.with_extension("clixpack.sha256");
    assert!(sha_path.exists(), "sha256 sidecar should exist");
    let sig_path = format!("{}.sig", zip_path.display());
    assert!(!Path::new(&sig_path).exists(), "no .sig for unsigned bundle");
}

/// (c) Signed bundle creates .zip + .sha256 + .sig + .fingerprint.
#[test]
fn test_bundle_signed() {
    let tmp = tempdir().unwrap();
    let pack_dir = tmp.path().join("pack");
    write_test_pack(&pack_dir);
    let priv_path = tmp.path().join("signing.pem");
    let pub_path  = tmp.path().join("signing.pub");
    generate_keypair(&priv_path, &pub_path, false).unwrap();

    let out_dir = tmp.path().join("out");
    let zip_path = bundle_pack_signed(&pack_dir, &out_dir, Some(&priv_path)).unwrap();
    assert!(zip_path.exists());
    assert!(Path::new(&format!("{}.sig", zip_path.display())).exists(), ".sig should exist");
    assert!(Path::new(&format!("{}.fingerprint", zip_path.display())).exists(), ".fingerprint should exist");
}

/// (d) Sign, trust the key, verify via verify_signature → Ok.
#[test]
fn test_verify_valid_signature() {
    let tmp = tempdir().unwrap();
    let pack_dir = tmp.path().join("pack");
    write_test_pack(&pack_dir);
    let priv_path = tmp.path().join("signing.pem");
    let pub_path  = tmp.path().join("signing.pub");
    generate_keypair(&priv_path, &pub_path, false).unwrap();

    let out_dir = tmp.path().join("out");
    let zip_path = bundle_pack_signed(&pack_dir, &out_dir, Some(&priv_path)).unwrap();

    let trusted_dir = tmp.path().join("trusted-keys");
    trust_key(&pub_path, &trusted_dir).unwrap();

    let sha = zip_sha256(&zip_path);
    let sig = read_sig(&zip_path);
    verify_signature(&sha, &sig, &trusted_dir).expect("valid signature should verify OK");
}

/// (e) Tamper with the zip bytes → sha256 changes → verify fails.
#[test]
fn test_verify_tampered_bundle() {
    let tmp = tempdir().unwrap();
    let pack_dir = tmp.path().join("pack");
    write_test_pack(&pack_dir);
    let priv_path = tmp.path().join("signing.pem");
    let pub_path  = tmp.path().join("signing.pub");
    generate_keypair(&priv_path, &pub_path, false).unwrap();

    let out_dir = tmp.path().join("out");
    let zip_path = bundle_pack_signed(&pack_dir, &out_dir, Some(&priv_path)).unwrap();

    let sig = read_sig(&zip_path);

    // Tamper: flip a byte near the end of the zip
    let mut bytes = fs::read(&zip_path).unwrap();
    let last = bytes.len() - 1;
    bytes[last] ^= 0xff;
    fs::write(&zip_path, &bytes).unwrap();

    let trusted_dir = tmp.path().join("trusted-keys");
    trust_key(&pub_path, &trusted_dir).unwrap();

    // The sha256 of the tampered zip no longer matches what was signed
    let sha = zip_sha256(&zip_path);
    assert!(
        verify_signature(&sha, &sig, &trusted_dir).is_err(),
        "tampered bundle should fail signature check"
    );
}

/// (f) Sign with key A, trust key B → verify rejects.
#[test]
fn test_verify_wrong_key() {
    let tmp = tempdir().unwrap();
    let pack_dir = tmp.path().join("pack");
    write_test_pack(&pack_dir);

    let priv_a = tmp.path().join("a.pem");
    let pub_a  = tmp.path().join("a.pub");
    let priv_b = tmp.path().join("b.pem");
    let pub_b  = tmp.path().join("b.pub");
    generate_keypair(&priv_a, &pub_a, false).unwrap();
    generate_keypair(&priv_b, &pub_b, false).unwrap();

    let out_dir = tmp.path().join("out");
    let zip_path = bundle_pack_signed(&pack_dir, &out_dir, Some(&priv_a)).unwrap();
    let sig = read_sig(&zip_path);
    let sha = zip_sha256(&zip_path);

    // Only trust key B
    let trusted_dir = tmp.path().join("trusted-keys");
    trust_key(&pub_b, &trusted_dir).unwrap();

    assert!(
        verify_signature(&sha, &sig, &trusted_dir).is_err(),
        "signature from key A should not verify with only key B trusted"
    );
}
