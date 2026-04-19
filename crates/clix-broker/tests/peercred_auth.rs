//! Broker SO_PEERCRED authentication tests.
//!
//! These tests verify that the broker socket only accepts connections from the
//! expected UID (the user running the process). They spawn the broker binary,
//! connect as the current user (allowed), and document the rejection path.
//!
//! Note: UID-spoofing to test the denial path requires either running as root or
//! using Linux user namespaces. That scenario is marked `#[ignore]` and documented
//! in CONTRIBUTING.md for manual verification.

use std::fs;
use std::io::{BufRead, BufReader, Write};
use std::os::unix::net::UnixStream;
use std::time::Duration;
use tempfile::tempdir;
use clix_core::execution::worker_protocol::{BrokerMintRequest, BrokerMintResponse};

fn broker_bin() -> std::path::PathBuf {
    let mut path = std::env::current_exe().expect("current_exe");
    loop {
        if path.join("target").exists() { break; }
        if !path.pop() { break; }
    }
    let debug = path.join("target/debug/clix-broker");
    if debug.exists() { return debug; }
    path.join("target/release/clix-broker")
}

fn spawn_broker(socket_path: &std::path::Path, creds_dir: &std::path::Path) -> std::process::Child {
    let bin = broker_bin();
    if !bin.exists() {
        panic!("clix-broker binary not found at {}. Run `cargo build -p clix-broker` first.", bin.display());
    }
    std::process::Command::new(&bin)
        .env("CLIX_BROKER_SOCKET", socket_path)
        .env("CLIX_BROKER_HOME", creds_dir)
        .stderr(std::process::Stdio::null())
        .spawn()
        .expect("spawn broker")
}

fn wait_for_socket(socket_path: &std::path::Path) {
    for _ in 0..50 {
        if UnixStream::connect(socket_path).is_ok() { return; }
        std::thread::sleep(Duration::from_millis(100));
    }
    panic!("broker socket did not appear");
}

/// Same-user connection succeeds (ping-pong works).
/// This verifies the SO_PEERCRED happy path: current process UID matches broker's expected UID.
#[test]
fn test_peercred_same_user_accepted() {
    let tmp = tempdir().unwrap();
    let socket = tmp.path().join("broker.sock");
    let creds = tmp.path().join("creds");
    fs::create_dir_all(&creds).unwrap();

    let mut child = spawn_broker(&socket, &creds);
    wait_for_socket(&socket);

    let stream = UnixStream::connect(&socket).expect("connect");
    let mut writer = stream.try_clone().unwrap();
    let mut reader = BufReader::new(stream);

    let req = serde_json::to_string(&BrokerMintRequest::Ping).unwrap() + "\n";
    writer.write_all(req.as_bytes()).unwrap();
    let mut resp_line = String::new();
    reader.read_line(&mut resp_line).unwrap();

    let resp: BrokerMintResponse = serde_json::from_str(&resp_line).expect("parse response");
    assert!(
        matches!(resp, BrokerMintResponse::Pong { .. }),
        "same-user connection should be accepted: {:?}", resp
    );

    child.kill().ok();
    child.wait().ok();
}

/// Documents the UID-spoofing denial scenario.
///
/// This test is `#[ignore]` because it requires running as root or inside a user
/// namespace where we can setuid to a different UID, neither of which is available
/// in most CI environments.
///
/// Manual verification: run the broker as a non-root user, then in a separate
/// terminal as a different user (or via `nsenter`), attempt to connect.
/// The broker should print `[clix-broker] rejected connection: UID mismatch`.
///
/// Example: `sudo -u nobody socat - UNIX-CONNECT:/tmp/clix-broker.sock`
#[test]
#[ignore = "requires root or user-namespace privilege escalation"]
fn test_peercred_different_user_rejected() {
    // In a real environment: spawn broker as user A, connect as user B, assert connection is dropped.
    // The broker closes the stream without writing a response when SO_PEERCRED check fails.
    // This is documented here as a manual test scenario.
    unimplemented!("manual test — see docstring");
}
