//! CLI smoke tests — exercise the `clix` binary via `assert_cmd`.
//!
//! These tests verify:
//! (a) `clix --help` exits 0 and mentions expected commands.
//! (b) `clix profile list` works without a configured home.
//! (c) Bad subcommands produce a non-zero exit and helpful stderr.
//! (f) `clix secrets test` with a bogus Infisical URL returns within 20s (reqwest timeout).

use assert_cmd::Command;
use predicates::prelude::*;
use std::time::{Duration, Instant};
use tempfile::tempdir;

fn clix() -> Command {
    Command::cargo_bin("clix").expect("clix binary")
}

/// (a) `clix --help` exits 0 and mentions core subcommands.
#[test]
fn test_help_exits_zero() {
    clix()
        .arg("--help")
        .assert()
        .success()
        .stdout(predicate::str::contains("run"))
        .stdout(predicate::str::contains("profile"))
        .stdout(predicate::str::contains("pack"));
}

/// (b) `clix profile list` with an empty CLIX_HOME exits 0.
#[test]
fn test_profile_list_empty_home() {
    let home = tempdir().unwrap();
    clix()
        .arg("profile")
        .arg("list")
        .env("CLIX_HOME", home.path())
        .assert()
        .success();
}

/// (c) Unknown subcommand produces non-zero exit.
#[test]
fn test_unknown_subcommand_fails() {
    clix()
        .arg("definitely-not-a-real-command")
        .assert()
        .failure();
}

/// (d) `clix capabilities list` with an empty home exits 0 (empty output).
#[test]
fn test_capabilities_list_empty_home() {
    let home = tempdir().unwrap();
    clix()
        .arg("capabilities")
        .arg("list")
        .env("CLIX_HOME", home.path())
        .assert()
        .success();
}

/// (e) `clix receipts list` with an empty home exits 0.
#[test]
fn test_receipts_list_empty_home() {
    let home = tempdir().unwrap();
    clix()
        .arg("receipts")
        .arg("list")
        .env("CLIX_HOME", home.path())
        .assert()
        .success();
}

/// (f) `clix secrets test` with a bogus Infisical URL must complete within 20s
/// (reqwest has a 10s connect + 5s timeout). This guards against the TUI hanging
/// on the Secrets screen when connectivity probes block the event loop.
#[test]
fn test_secrets_test_bad_url_completes_within_timeout() {
    let home = tempdir().unwrap();
    // Seed a minimal config with an unreachable Infisical URL
    let config_yaml = r#"schemaVersion: 1
defaultEnv: dev
infisicalProfiles:
  default:
    siteUrl: "http://127.0.0.1:19998"
    defaultEnvironment: dev
activeInfisical: default
"#;
    std::fs::write(home.path().join("config.yaml"), config_yaml).unwrap();

    let start = Instant::now();
    clix()
        .arg("secrets")
        .arg("test")
        .env("CLIX_HOME", home.path())
        .timeout(Duration::from_secs(25))
        .assert()
        .failure(); // non-zero because the URL is unreachable
    assert!(
        start.elapsed() < Duration::from_secs(20),
        "secrets test took too long — likely blocked on network call: {:?}",
        start.elapsed()
    );
}
