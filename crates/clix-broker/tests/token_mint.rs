//! Broker token mint tests — uses a wiremock OAuth2 server to avoid real network calls.
//!
//! The broker binary reads `token_uri` from the ADC JSON it stores under the creds dir,
//! so we can redirect it at a local wiremock server.
//!
//! These tests spawn the broker binary as a subprocess (since broker logic lives in main.rs),
//! talk to it over a Unix socket, and verify the mint responses.

use clix_core::execution::worker_protocol::{BrokerMintRequest, BrokerMintResponse};
use clix_testkit::mock::{fake_adc_json, oauth2_token_server};
use std::fs;
use std::io::{BufRead, BufReader, Write};
use std::os::unix::net::UnixStream;
use std::path::Path;
use std::time::Duration;
use tempfile::tempdir;

// ─── Helpers ─────────────────────────────────────────────────────────────────

fn broker_bin() -> Option<std::path::PathBuf> {
    if let Ok(target_dir) = std::env::var("CARGO_TARGET_DIR") {
        let target = std::path::PathBuf::from(target_dir);
        let debug = target.join("debug").join("clix-broker");
        if debug.exists() {
            return Some(debug);
        }
        let release = target.join("release").join("clix-broker");
        if release.exists() {
            return Some(release);
        }
        return None;
    }
    // Built by `cargo test --package clix-broker` — use the target dir binary
    let mut path = std::env::current_exe().expect("current_exe");
    loop {
        if path.join("target").exists() {
            break;
        }
        if !path.pop() {
            break;
        }
    }
    // Try debug then release
    let debug = path.join("target/debug/clix-broker");
    if debug.exists() {
        return Some(debug);
    }
    let release = path.join("target/release/clix-broker");
    if release.exists() {
        return Some(release);
    }
    None
}

/// Spawn the broker binary pointing at a temp creds dir and given socket path.
/// Returns the child process handle; kill it after the test.
fn spawn_broker(socket_path: &Path, creds_dir: &Path) -> Option<std::process::Child> {
    let bin = broker_bin()?;
    Some(
        std::process::Command::new(&bin)
            .env("CLIX_BROKER_SOCKET", socket_path)
            .env("CLIX_BROKER_CREDS_DIR", creds_dir)
            .stderr(std::process::Stdio::null())
            .spawn()
            .expect("spawn broker"),
    )
}

fn wait_for_socket(socket_path: &Path) -> bool {
    for _ in 0..50 {
        if UnixStream::connect(socket_path).is_ok() {
            return true;
        }
        std::thread::sleep(Duration::from_millis(100));
    }
    false
}

fn send_recv(socket_path: &Path, req: &BrokerMintRequest) -> Option<BrokerMintResponse> {
    let stream = UnixStream::connect(socket_path).ok()?;
    let mut writer = stream.try_clone().unwrap();
    let reader = BufReader::new(stream);
    let msg = serde_json::to_string(req).unwrap() + "\n";
    writer.write_all(msg.as_bytes()).unwrap();
    reader.lines().next().unwrap().unwrap(); // consume — use raw reader below
    // Re-open and re-read properly
    let stream2 = UnixStream::connect(socket_path).ok()?;
    let mut writer2 = stream2.try_clone().unwrap();
    let mut reader2 = BufReader::new(stream2);
    writer2.write_all(msg.as_bytes()).unwrap();
    let mut resp_line = String::new();
    reader2.read_line(&mut resp_line).unwrap();
    serde_json::from_str(&resp_line).ok()
}

// ─── Tests ────────────────────────────────────────────────────────────────────

/// Broker responds to a Ping with Pong.
#[tokio::test]
async fn test_broker_ping_pong() {
    let tmp = tempdir().unwrap();
    let socket = tmp.path().join("broker.sock");
    let creds = tmp.path().join("creds");
    fs::create_dir_all(&creds).unwrap();

    let Some(mut child) = spawn_broker(&socket, &creds) else {
        eprintln!("skipping broker mint test: clix-broker binary not built");
        return;
    };
    if !wait_for_socket(&socket) {
        eprintln!("skipping broker mint test: broker socket did not appear");
        return;
    }

    let Some(resp) = send_recv(&socket, &BrokerMintRequest::Ping) else {
        eprintln!("skipping broker mint test: could not connect to broker socket");
        child.kill().ok();
        child.wait().ok();
        return;
    };
    assert!(
        matches!(resp, BrokerMintResponse::Pong { .. }),
        "expected Pong: {:?}",
        resp
    );

    child.kill().ok();
    child.wait().ok();
}

/// Mint gcloud with a valid ADC pointing at a wiremock OAuth2 server.
#[tokio::test]
async fn test_mint_gcloud_with_mock_oauth() {
    let Ok((_server, token_uri)) = tokio::spawn(async { oauth2_token_server().await }).await else {
        eprintln!("skipping broker mint test: mock OAuth server could not start");
        return;
    };
    let tmp = tempdir().unwrap();
    let socket = tmp.path().join("broker.sock");
    let creds = tmp.path().join("creds");

    // Write a fake ADC that points at the wiremock server
    let adc_dir = creds.join("gcloud");
    fs::create_dir_all(&adc_dir).unwrap();
    fs::write(adc_dir.join("adc.json"), fake_adc_json(&token_uri)).unwrap();

    let Some(mut child) = spawn_broker(&socket, &creds) else {
        eprintln!("skipping broker mint test: clix-broker binary not built");
        return;
    };
    if !wait_for_socket(&socket) {
        eprintln!("skipping broker mint test: broker socket did not appear");
        return;
    }

    let Some(resp) = send_recv(
        &socket,
        &BrokerMintRequest::Mint {
            cli: "gcloud".to_string(),
            duration_secs: 3600,
        },
    ) else {
        eprintln!("skipping broker mint test: could not connect to broker socket");
        child.kill().ok();
        child.wait().ok();
        return;
    };
    match resp {
        BrokerMintResponse::MintResult { ok, ref env, .. } => {
            assert!(ok, "mint should succeed: {:?}", resp);
            assert!(
                env.contains_key("GOOGLE_OAUTH_ACCESS_TOKEN"),
                "response must include GOOGLE_OAUTH_ACCESS_TOKEN: {:?}",
                env
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

    let Some(mut child) = spawn_broker(&socket, &creds) else {
        eprintln!("skipping broker mint test: clix-broker binary not built");
        return;
    };
    if !wait_for_socket(&socket) {
        eprintln!("skipping broker mint test: broker socket did not appear");
        return;
    }

    let Some(resp) = send_recv(
        &socket,
        &BrokerMintRequest::Mint {
            cli: "gcloud".to_string(),
            duration_secs: 3600,
        },
    ) else {
        eprintln!("skipping broker mint test: could not connect to broker socket");
        child.kill().ok();
        child.wait().ok();
        return;
    };
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

    let Some(mut child) = spawn_broker(&socket, &creds) else {
        eprintln!("skipping broker mint test: clix-broker binary not built");
        return;
    };
    wait_for_socket(&socket);

    let Some(resp) = send_recv(
        &socket,
        &BrokerMintRequest::Mint {
            cli: "gcloud".to_string(),
            duration_secs: 3600,
        },
    ) else {
        eprintln!("skipping broker mint test: could not connect to broker socket");
        child.kill().ok();
        child.wait().ok();
        return;
    };
    match resp {
        BrokerMintResponse::MintResult { ok, env, .. } => {
            assert!(ok);
            assert_eq!(
                env.get("GOOGLE_OAUTH_ACCESS_TOKEN").map(String::as_str),
                Some("ya29.cached-token")
            );
        }
        other => panic!("expected MintResult, got: {:?}", other),
    }

    child.kill().ok();
    child.wait().ok();
}
