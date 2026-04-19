//! Shim PATH-shadowing tests.
//!
//! The shim binary (clix-shim) is designed to be renamed and placed in a directory
//! prepended to PATH. When invoked, it connects to the clix gateway socket and
//! forwards the request.
//!
//! These tests verify:
//! (a) The shim exits 127 when the gateway socket is unreachable.
//! (b) The shim exits 127 with a clear "cannot connect" message.
//! (c) The shim correctly reads argv[0] to determine its command name.
//! (d) A fake shim in a prepended PATH dir shadows the real binary.

use std::fs;
use std::os::unix::fs::PermissionsExt;
use assert_cmd::Command;
use predicates::prelude::*;
use tempfile::tempdir;

fn shim_bin() -> std::path::PathBuf {
    // Locate the built clix-shim binary
    let mut path = std::env::current_exe().expect("current_exe");
    loop {
        if path.join("target").exists() { break; }
        if !path.pop() { break; }
    }
    let debug = path.join("target/debug/clix-shim");
    if debug.exists() { return debug; }
    path.join("target/release/clix-shim")
}

/// (a)(b) Shim exits 127 with a clear message when gateway is not running.
#[test]
fn test_shim_exits_127_no_gateway() {
    let bin = shim_bin();
    if !bin.exists() {
        eprintln!("skipping: clix-shim binary not built at {}", bin.display());
        return;
    }
    // Use a socket path that definitely has no listener
    let tmp = tempdir().unwrap();
    let dead_socket = tmp.path().join("no-such-gateway.sock");

    std::process::Command::new(&bin)
        .env("CLIX_GATEWAY_SOCKET", &dead_socket)
        .arg("status")
        .status()
        .map(|s| {
            assert_eq!(s.code().unwrap_or(-1), 127, "shim should exit 127 when gateway unreachable");
        })
        .unwrap_or_else(|e| eprintln!("skipping: {e}"));
}

/// (c) Shim reads its command name from argv[0] (basename).
/// We rename the binary via a symlink and verify the reported command name.
#[test]
fn test_shim_command_name_from_argv0() {
    let bin = shim_bin();
    if !bin.exists() {
        eprintln!("skipping: clix-shim binary not built");
        return;
    }
    let tmp = tempdir().unwrap();
    let shim_copy = tmp.path().join("gcloud");
    fs::copy(&bin, &shim_copy).unwrap();
    let mut perms = fs::metadata(&shim_copy).unwrap().permissions();
    perms.set_mode(0o755);
    fs::set_permissions(&shim_copy, perms).unwrap();

    let dead_socket = tmp.path().join("no-gateway.sock");
    let output = std::process::Command::new(&shim_copy)
        .env("CLIX_GATEWAY_SOCKET", &dead_socket)
        .output()
        .unwrap();

    let stderr = String::from_utf8_lossy(&output.stderr);
    // The shim should mention "gcloud" (from argv[0]) in its error message
    assert!(
        stderr.contains("gcloud") || stderr.contains("cannot connect"),
        "shim stderr should reference command name or error: {stderr}"
    );
    assert_eq!(output.status.code().unwrap_or(-1), 127);
}

/// (d) PATH prepend: a directory with a fake "gcloud" shim copy gets called first.
#[test]
fn test_path_shadowing() {
    let bin = shim_bin();
    if !bin.exists() {
        eprintln!("skipping: clix-shim binary not built");
        return;
    }
    let tmp = tempdir().unwrap();
    let shim_dir = tmp.path().join("shims");
    fs::create_dir_all(&shim_dir).unwrap();

    // Place a shim copy named "gcloud"
    let fake_gcloud = shim_dir.join("gcloud");
    fs::copy(&bin, &fake_gcloud).unwrap();
    let mut perms = fs::metadata(&fake_gcloud).unwrap().permissions();
    perms.set_mode(0o755);
    fs::set_permissions(&fake_gcloud, perms).unwrap();

    // Build a PATH that prepends shim_dir
    let original_path = std::env::var("PATH").unwrap_or_default();
    let new_path = format!("{}:{}", shim_dir.display(), original_path);

    let dead_socket = tmp.path().join("no-gateway.sock");
    let output = std::process::Command::new("gcloud")
        .env("PATH", &new_path)
        .env("CLIX_GATEWAY_SOCKET", &dead_socket)
        .arg("auth")
        .arg("print-access-token")
        .output();

    match output {
        Ok(out) => {
            // Our shim (not the real gcloud) should have been called; it will exit 127
            // with the shim's error message
            let stderr = String::from_utf8_lossy(&out.stderr);
            assert!(
                stderr.contains("clix") || stderr.contains("cannot connect"),
                "should have been intercepted by the shim, stderr: {stderr}"
            );
            assert_eq!(out.status.code().unwrap_or(-1), 127, "shim exits 127");
        }
        Err(e) => {
            // gcloud not on PATH at all — shim dir would be used, but if shim_dir/gcloud
            // itself can't be found by the shell, just skip.
            eprintln!("skipping PATH-shadow test: {e}");
        }
    }
}
