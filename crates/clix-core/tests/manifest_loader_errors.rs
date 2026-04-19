//! Manifest loader error-path tests.
//!
//! Covers:
//! (a) Malformed YAML → parse error.
//! (b) Missing required field (name) → error.
//! (c) Missing required field (backend) → error.
//! (d) Unknown backend type → error.
//! (e) Valid minimal capability manifest → Ok.
//! (f) Valid pack.yaml → Ok.
//! (g) Pack manifest with invalid version format → Ok (version is a string).

use std::fs;
use tempfile::tempdir;

use clix_core::manifest::loader::load_manifest;
use clix_core::manifest::capability::CapabilityManifest;
use clix_core::manifest::pack::PackManifest;

fn write_cap(dir: &std::path::Path, filename: &str, content: &str) -> std::path::PathBuf {
    let path = dir.join(filename);
    fs::write(&path, content).unwrap();
    path
}

/// (a) Malformed YAML (unbalanced braces) → error.
#[test]
fn test_malformed_yaml() {
    let tmp = tempdir().unwrap();
    let path = write_cap(tmp.path(), "bad.yaml", "name: foo\nbackend: {\n");
    let result = load_manifest::<CapabilityManifest>(&path);
    assert!(result.is_err(), "malformed YAML should fail to parse");
}

/// (b) Missing required field `backend`.
#[test]
fn test_missing_backend() {
    let tmp = tempdir().unwrap();
    let path = write_cap(tmp.path(), "no_backend.yaml", concat!(
        "name: foo\n",
        "version: 1\n",
    ));
    let result = load_manifest::<CapabilityManifest>(&path);
    assert!(result.is_err(), "missing backend should fail");
}

/// (c) Valid minimal capability manifest → Ok.
#[test]
fn test_valid_minimal_capability() {
    let tmp = tempdir().unwrap();
    let path = write_cap(tmp.path(), "hello.yaml", concat!(
        "name: test.hello\n",
        "version: 1\n",
        "backend:\n",
        "  type: builtin\n",
        "  name: date\n",
    ));
    let result = load_manifest::<CapabilityManifest>(&path);
    assert!(result.is_ok(), "valid manifest should parse: {:?}", result);
    let cap = result.unwrap();
    assert_eq!(cap.name, "test.hello");
    assert_eq!(cap.version, 1);
}

/// (d) Valid pack.yaml → Ok.
#[test]
fn test_valid_pack_manifest() {
    let tmp = tempdir().unwrap();
    let path = write_cap(tmp.path(), "pack.yaml", concat!(
        "name: mypack\n",
        "version: \"1.2.3\"\n",
        "description: test pack\n",
        "author: test\n",
    ));
    let result = load_manifest::<PackManifest>(&path);
    assert!(result.is_ok(), "valid pack.yaml should parse: {:?}", result);
    let pack = result.unwrap();
    assert_eq!(pack.name, "mypack");
}

/// (e) File not found → error.
#[test]
fn test_file_not_found() {
    let tmp = tempdir().unwrap();
    let path = tmp.path().join("nonexistent.yaml");
    let result = load_manifest::<CapabilityManifest>(&path);
    assert!(result.is_err(), "missing file should fail");
}

/// (f) Empty file → error.
#[test]
fn test_empty_file() {
    let tmp = tempdir().unwrap();
    let path = write_cap(tmp.path(), "empty.yaml", "");
    let result = load_manifest::<CapabilityManifest>(&path);
    assert!(result.is_err(), "empty file should fail to deserialize into CapabilityManifest");
}

/// (g) Capability with an argv_pattern field round-trips correctly.
#[test]
fn test_capability_argv_pattern_roundtrip() {
    let tmp = tempdir().unwrap();
    let path = write_cap(tmp.path(), "shim.yaml", concat!(
        "name: gcloud.compute.instances.list\n",
        "version: 1\n",
        "backend:\n",
        "  type: subprocess\n",
        "  command: gcloud\n",
        "argv_pattern: \"gcloud compute instances list *\"\n",
    ));
    let result = load_manifest::<CapabilityManifest>(&path);
    assert!(result.is_ok(), "argv_pattern cap should parse: {:?}", result);
    let cap = result.unwrap();
    assert_eq!(cap.argv_pattern, Some("gcloud compute instances list *".to_string()));
}
