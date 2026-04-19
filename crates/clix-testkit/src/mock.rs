//! Test mocks: wiremock OAuth2 endpoint and Unix-socket broker echo server.

use std::collections::HashMap;
use wiremock::{MockServer, Mock, ResponseTemplate};
use wiremock::matchers::{method, path};

// ─── OAuth2 mock ──────────────────────────────────────────────────────────────

/// Start a wiremock server that responds to POST /token with a canned access-token response.
/// Returns `(server, token_uri)`. The `token_uri` can be embedded in a fake ADC JSON.
pub async fn oauth2_token_server() -> (MockServer, String) {
    let server = MockServer::start().await;
    Mock::given(method("POST"))
        .and(path("/token"))
        .respond_with(ResponseTemplate::new(200).set_body_json(serde_json::json!({
            "access_token": "ya29.mock-token",
            "expires_in": 3600,
            "token_type": "Bearer"
        })))
        .mount(&server)
        .await;
    let uri = format!("{}/token", server.uri());
    (server, uri)
}

/// Build a fake gcloud ADC JSON that points `token_uri` at the given mock URL.
pub fn fake_adc_json(token_uri: &str) -> String {
    serde_json::json!({
        "type": "authorized_user",
        "client_id": "test-client-id.apps.googleusercontent.com",
        "client_secret": "test-client-secret",
        "refresh_token": "1//test-refresh-token",
        "token_uri": token_uri
    })
    .to_string()
}

// ─── Broker socket echo server ────────────────────────────────────────────────

use std::io::{BufRead, BufReader, Write};
use std::os::unix::net::UnixListener;
use std::path::PathBuf;
use std::sync::Arc;

/// Configuration for a test broker socket server.
pub struct BrokerServerConfig {
    /// The UID that will be accepted (use 0 for accept-all in tests).
    /// Responses keyed by CLI name → canned env vars.
    pub responses: HashMap<String, HashMap<String, String>>,
}

impl Default for BrokerServerConfig {
    fn default() -> Self {
        let mut responses = HashMap::new();
        // gcloud: return a mock token
        let mut gcloud_env = HashMap::new();
        gcloud_env.insert("GOOGLE_OAUTH_ACCESS_TOKEN".to_string(), "ya29.mock-token".to_string());
        responses.insert("gcloud".to_string(), gcloud_env);
        Self { responses }
    }
}

/// Spawn a Unix-socket server that responds to `BrokerMintRequest::Mint` with canned env vars.
/// Returns the socket path. The server runs in a background thread and shuts down when the
/// returned `PathBuf` is dropped (socket file is removed by the OS on process exit).
pub fn spawn_broker_socket(tmp: &tempfile::TempDir, config: BrokerServerConfig) -> PathBuf {
    use clix_core::execution::worker_protocol::{BrokerMintRequest, BrokerMintResponse};

    let socket_path = tmp.path().join("broker.sock");
    let socket_path_bg = socket_path.clone();
    let _ = std::fs::remove_file(&socket_path);
    let listener = UnixListener::bind(&socket_path).expect("bind broker socket");
    let responses = Arc::new(config.responses);

    std::thread::spawn(move || {
        for stream in listener.incoming() {
            let Ok(stream) = stream else { break };
            let responses = Arc::clone(&responses);
            std::thread::spawn(move || {
                let mut writer = stream.try_clone().expect("clone stream");
                let reader = BufReader::new(stream);
                for line in reader.lines() {
                    let Ok(line) = line else { break };
                    let resp: BrokerMintResponse = match serde_json::from_str::<BrokerMintRequest>(&line) {
                        Ok(BrokerMintRequest::Ping) => {
                            BrokerMintResponse::Pong { version: "test".to_string() }
                        }
                        Ok(BrokerMintRequest::Mint { cli, .. }) => {
                            match responses.get(&cli) {
                                Some(env) => BrokerMintResponse::MintResult {
                                    ok: true, env: env.clone(), error: None,
                                },
                                None => BrokerMintResponse::MintResult {
                                    ok: false,
                                    env: Default::default(),
                                    error: Some(format!("no canned response for {cli}")),
                                },
                            }
                        }
                        _ => BrokerMintResponse::MintResult {
                            ok: false, env: Default::default(),
                            error: Some("unsupported in test broker".to_string()),
                        },
                    };
                    let msg = serde_json::to_string(&resp).unwrap() + "\n";
                    if writer.write_all(msg.as_bytes()).is_err() { break; }
                }
            });
        }
        let _ = std::fs::remove_file(&socket_path_bg);
    });

    socket_path
}
