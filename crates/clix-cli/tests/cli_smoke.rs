//! CLI smoke tests — exercise the `clix` binary via `std::process::Command`.
//!
//! These tests verify:
//! (a) `clix --help` exits 0 and mentions expected commands.
//! (b) `clix profile list` works without a configured home.
//! (c) Bad subcommands produce a non-zero exit and helpful stderr.
//! (f) `clix secrets test` with a bogus Infisical URL returns within 20s (reqwest timeout).

use std::process::{Command, Stdio};
use std::time::{Duration, Instant};
use tempfile::tempdir;

fn clix() -> Command {
    if let Some(exe) = std::env::var_os("CARGO_BIN_EXE_clix") {
        return Command::new(exe);
    }
    if let Some(target_dir) = std::env::var_os("CARGO_TARGET_DIR") {
        let target = std::path::PathBuf::from(target_dir);
        let debug = target.join("debug").join("clix");
        if debug.exists() {
            return Command::new(debug);
        }
        let release = target.join("release").join("clix");
        if release.exists() {
            return Command::new(release);
        }
    }

    let mut path = std::env::current_exe().expect("current_exe");
    loop {
        if path.join("target").exists() {
            break;
        }
        if !path.pop() {
            break;
        }
    }
    let debug = path.join("target/debug/clix");
    if debug.exists() {
        return Command::new(debug);
    }
    Command::new(path.join("target/release/clix"))
}

fn output(args: &[&str], envs: &[(&str, &std::path::Path)]) -> std::process::Output {
    let mut cmd = clix();
    cmd.args(args);
    for (k, v) in envs {
        cmd.env(k, v);
    }
    cmd.output().expect("run clix")
}

/// (a) `clix --help` exits 0 and mentions core subcommands.
#[test]
fn test_help_exits_zero() {
    let out = output(&["--help"], &[]);
    assert!(out.status.success(), "help failed: {:?}", out);
    let stdout = String::from_utf8_lossy(&out.stdout);
    assert!(stdout.contains("run"));
    assert!(stdout.contains("profile"));
    assert!(stdout.contains("pack"));
}

/// (b) `clix profile list` with an empty CLIX_HOME exits 0.
#[test]
fn test_profile_list_empty_home() {
    let home = tempdir().unwrap();
    let out = output(&["profile", "list"], &[("CLIX_HOME", home.path())]);
    assert!(out.status.success(), "profile list failed: {:?}", out);
}

/// (c) Unknown subcommand produces non-zero exit.
#[test]
fn test_unknown_subcommand_fails() {
    let out = output(&["definitely-not-a-real-command"], &[]);
    assert!(!out.status.success(), "unexpected success: {:?}", out);
}

/// (d) `clix capabilities list` with an empty home exits 0 (empty output).
#[test]
fn test_capabilities_list_empty_home() {
    let home = tempdir().unwrap();
    let out = output(&["capabilities", "list"], &[("CLIX_HOME", home.path())]);
    assert!(out.status.success(), "capabilities list failed: {:?}", out);
}

/// (e) `clix receipts list` with an empty home exits 0.
#[test]
fn test_receipts_list_empty_home() {
    let home = tempdir().unwrap();
    let out = output(&["receipts", "list"], &[("CLIX_HOME", home.path())]);
    assert!(out.status.success(), "receipts list failed: {:?}", out);
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
    let mut child = clix()
        .args(["secrets", "test"])
        .env("CLIX_HOME", home.path())
        .stderr(Stdio::piped())
        .stdout(Stdio::piped())
        .spawn()
        .expect("spawn secrets test");

    let deadline = Instant::now() + Duration::from_secs(25);
    loop {
        if let Some(status) = child.try_wait().expect("poll child") {
            assert!(!status.success(), "unexpected success");
            break;
        }
        if Instant::now() > deadline {
            let _ = child.kill();
            let _ = child.wait();
            panic!("secrets test exceeded timeout");
        }
        std::thread::sleep(Duration::from_millis(50));
    }
    assert!(
        start.elapsed() < Duration::from_secs(20),
        "secrets test took too long — likely blocked on network call: {:?}",
        start.elapsed()
    );
}
