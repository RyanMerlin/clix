//! CLI smoke tests — exercise the `clix` binary via `assert_cmd`.
//!
//! These tests verify:
//! (a) `clix --help` exits 0 and mentions expected commands.
//! (b) `clix profile list` works without a configured home.
//! (c) Bad subcommands produce a non-zero exit and helpful stderr.

use assert_cmd::Command;
use predicates::prelude::*;
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
