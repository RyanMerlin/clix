//! Broker token mint tests — uses a wiremock OAuth2 server to avoid real network calls.
//!
//! The broker binary reads `token_uri` from the ADC JSON it stores under the creds dir,
//! so we can redirect it at a local wiremock server.
//!
//! These tests spawn the broker binary as a subprocess (since broker logic lives in main.rs),
//! talk to it over a Unix socket, and verify the mint responses.

use std::fs;
use std::io::{BufRead, BufReader, Write};
use std::os::unix::net::UnixStream;
use std::path::Path;
use std::time::Duration;
use tempfile::tempdir;
use clix_core::execution::worker_protocol::{BrokerMintRequest, BrokerMintResponse};
use clix_testkit::mock::{fake_adc_json, oauth2_token_server};

// ─── Helpers ─────────────────────────────────────────────────────────────────

fn broker_bin() -> std::path::PathBuf {
    // Built by `cargo test --package clix-broker` — use the target dir binary
    let mut path = std::env::current_exe().expect("current_exe");
    loop {
        if path.join("target").exists() { break; }
        if !path.pop() { break; }
    }
    // Try debug then release
    let debug = path.join("target/debug/clix-broker");
    if debug.exists() { return debug; }
    path.join("target/release/clix-broker")
}

/// Spawn the broker binary pointing at a temp creds dir and given socket path.
/// Returns the child process handle; kill it after the test.
fn spawn_broker(socket_path: &Path, creds_dir: &Path) -> std::process::Child {
    let bin = broker_bin();
    if !bin.exists() {
        panic!(
            "clix-broker binary not found at {}. Run `cargo build -p clix-broker` first.",
            bin.display()
        );
    }
    std::process::Command::new(&bin)
        .env("CLIX_BROKER_SOCKET", socket_path)
        .env("CLIX_BROKER_HOME", creds_dir)
        .stderr(std::process::Stdio::null())
        .spawn()
        .expect("spawn broker")
}

fn wait_for_socket(socket_path: &Path) {
    for _ in 0..50 {
        if UnixStream::connect(socket_path).is_ok() { return; }
        std::thread::sleep(Duration::from_millis(100));
    }
    panic!("broker socket did not appear at {}", socket_path.display());
}

fn send_recv(socket_path: &Path, req: &BrokerMintRequest) -> BrokerMintResponse {
    let stream = UnixStream::connect(socket_path).expect("connect");
    let mut writer = stream.try_clone().unwrap();
    let reader = BufReader::new(stream);
    let msg = serde_json::to_string(req).unwrap() + "\n";
    writer.write_all(msg.as_bytes()).unwrap();
    let mut line = String::new();
    reader.lines().next().unwrap().unwrap(); // consume — use raw reader below
    // Re-open and re-read properly
    let stream2 = UnixStream::connect(socket_path).expect("connect2");
    let mut writer2 = stream2.try_clone().unwrap();
    let mut reader2 = BufReader::new(stream2);
    writer2.write_all(msg.as_bytes()).unwrap();
    let mut resp_line = String::new();
    reader2.read_line(&mut resp_line).unwrap();
    serde_json::from_str(&resp_line).expect("parse response")
}

// ─── Tests ────────────────────────────────────────────────────────────────────

/// Broker responds to a Ping with Pong.
#[tokio::test]
async fn test_broker_ping_pong() {
    let tmp = tempdir().unwrap();
    let socket = tmp.path().join("broker.sock");
    let creds = tmp.path().join("creds");
    fs::create_dir_all(&creds).unwrap();

    let mut child = spawn_broker(&socket, &creds);
    wait_for_socket(&socket);

    let resp = send_recv(&socket, &BrokerMintRequest::Ping);
    assert!(matches!(resp, BrokerMintResponse::Pong { .. }), "expected Pong: {:?}", resp);

    child.kill().ok();
    child.wait().ok();
}

/// Mint gcloud with a valid ADC pointing at a wiremock OAuth2 server.
#[tokio::test]
async fn test_mint_gcloud_with_mock_oauth() {
    let (_server, token_uri) = oauth2_token_server().await;
    let tmp = tempdir().unwrap();
    let socket = tmp.path().join("broker.sock");
    let creds = tmp.path().join("creds");

    // Write a fake ADC that points at the wiremock server
    let adc_dir = creds.join("gcloud");
    fs::create_dir_all(&adc_dir).unwrap();
    fs::write(adc_dir.join("adc.json"), fake_adc_json(&token_uri)).unwrap();

    let mut child = spawn_broker(&socket, &creds);
    wait_for_socket(&socket);

    let resp = send_recv(&socket, &BrokerMintRequest::Mint { cli: "gcloud".to_string(), duration_secs: 3600 });
    match resp {
        BrokerMintResponse::MintResult { ok, ref env, .. } => {
            assert!(ok, "mint should succeed: {:?}", resp);
            assert!(
                env.contains_key("GOOGLE_OAUTH_ACCESS_TOKEN"),
                "response must include GOOGLE_OAUTH_ACCESS_TOKEN: {:?}", env
            );
        }
        other => panic!("expected MintResult, got: {:?}", other),
    }

    child.kill().ok();
    child.wait().ok();
}

/// Mint gcloud with no ADC → MintResult { ok: false }.
#[tokio::test]
async fn test_mint_gcloud_missing_adc() {
    let tmp = tempdir().unwrap();
    let socket = tmp.path().join("broker.sock");
    let creds = tmp.path().join("creds");
    fs::create_dir_all(&creds).unwrap();
    // No ADC written

    let mut child = spawn_broker(&socket, &creds);
    wait_for_socket(&socket);

    let resp = send_recv(&socket, &BrokerMintRequest::Mint { cli: "gcloud".to_string(), duration_secs: 3600 });
    match resp {
        BrokerMintResponse::MintResult { ok, error, .. } => {
            assert!(!ok, "mint should fail without ADC");
            assert!(error.is_some(), "should have an error message");
        }
        other => panic!("expected MintResult, got: {:?}", other),
    }

    child.kill().ok();
    child.wait().ok();
}

/// Mint a cached (non-expired) token returns without hitting the token_uri.
#[tokio::test]
async fn test_mint_gcloud_cached_token() {
    let tmp = tempdir().unwrap();
    let socket = tmp.path().join("broker.sock");
    let creds = tmp.path().join("creds");
    let adc_dir = creds.join("gcloud");
    fs::create_dir_all(&adc_dir).unwrap();

    // ADC with a non-expired access_token (far future expiry)
    let adc = serde_json::json!({
        "type": "authorized_user",
        "client_id": "id",
        "client_secret": "secret",
        "refresh_token": "tok",
        "token_uri": "http://127.0.0.1:1/nonexistent",  // should NOT be called
        "access_token": "ya29.cached-token",
        "token_expiry": "2099-01-01T00:00:00Z"
    });
    fs::write(adc_dir.join("adc.json"), adc.to_string()).unwrap();

    let mut child = spawn_broker(&socket, &creds);
    wait_for_socket(&socket);

    let resp = send_recv(&socket, &BrokerMintRequest::Mint { cli: "gcloud".to_string(), duration_secs: 3600 });
    match resp {
        BrokerMintResponse::MintResult { ok, env, .. } => {
            assert!(ok);
            assert_eq!(env.get("GOOGLE_OAUTH_ACCESS_TOKEN").map(String::as_str), Some("ya29.cached-token"));
        }
        other => panic!("expected MintResult, got: {:?}", other),
    }

    child.kill().ok();
    child.wait().ok();
}
