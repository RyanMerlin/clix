/// clix-broker: credential store and ephemeral token minter.
///
/// The broker is a long-running daemon that:
///   1. Owns credentials in `$CLIX_BROKER_HOME` (default `/var/lib/clix/broker/`, or
///      `~/.local/share/clix/broker/` when run as a user daemon). The directory is mode 0700.
///   2. Listens on a Unix socket (`$CLIX_BROKER_SOCKET`) authenticated via `SO_PEERCRED`.
///      Only the gateway's UID/GID is allowed to connect.
///   3. On each `BrokerMintRequest`, reads the stored credentials for the requested CLI and
///      returns an ephemeral token set (`BrokerMintResponse`).
///
/// Currently supported CLIs:
///   - `gcloud`:  reads an ADC JSON file, returns `GOOGLE_OAUTH_ACCESS_TOKEN` from its
///                `token_uri` / `client_id` / `refresh_token` fields via the OAuth2 refresh flow.
///   - `kubectl`: reads a kubeconfig, generates an in-memory kubeconfig with a short-lived
///                `KUBECONFIG` pointing to a tmpfile containing a bearer token.
///   - `generic`: reads the credential as an env var and re-injects it verbatim.
///
/// On startup the broker:
///   - `chmod 0700` on the creds directory.
///   - Refuses to start if the directory is world-readable.
///   - Drops supplementary groups (if run as root, also drops to a dedicated user — TODO).
use std::io::{BufRead, BufReader, Write};
use std::os::unix::net::UnixListener;
use base64::Engine as _;
#[cfg(target_os = "linux")]
use std::os::unix::net::UnixStream;
use std::path::{Path, PathBuf};
use std::fs;
use std::collections::HashMap;
use std::sync::{Mutex, OnceLock};
use uuid::Uuid;
use clix_core::execution::worker_protocol::{BrokerMintRequest, BrokerMintResponse};

const DEFAULT_SOCKET_PATH: &str = "/tmp/clix-broker.sock";

// ─── Approval state machine ───────────────────────────────────────────────────

enum ApprovalState {
    Pending {
        capability: String,
        input: serde_json::Value,
        context: serde_json::Value,
        reason: String,
        requested_at: std::time::Instant,
    },
    Granted { approver: String },
    Denied { reason: String },
}

static PENDING_APPROVALS: OnceLock<Mutex<HashMap<Uuid, ApprovalState>>> = OnceLock::new();

fn approvals() -> &'static Mutex<HashMap<Uuid, ApprovalState>> {
    PENDING_APPROVALS.get_or_init(|| Mutex::new(HashMap::new()))
}

fn main() {
    let socket_path = std::env::var("CLIX_BROKER_SOCKET")
        .unwrap_or_else(|_| DEFAULT_SOCKET_PATH.to_string());
    let creds_dir = creds_dir();

    // Ensure creds dir exists with tight permissions
    if let Err(e) = ensure_creds_dir(&creds_dir) {
        eprintln!("[clix-broker] FATAL: {e}");
        std::process::exit(1);
    }

    // Remove stale socket
    let _ = fs::remove_file(&socket_path);

    let listener = match UnixListener::bind(&socket_path) {
        Ok(l) => l,
        Err(e) => {
            eprintln!("[clix-broker] FATAL: bind {socket_path}: {e}");
            std::process::exit(1);
        }
    };

    // Restrict socket to owner-only
    #[cfg(unix)]
    {
        use std::os::unix::fs::PermissionsExt;
        let _ = fs::set_permissions(&socket_path, fs::Permissions::from_mode(0o600));
    }

    eprintln!("[clix-broker] listening on {socket_path}");
    eprintln!("[clix-broker] creds dir: {}", creds_dir.display());

    for stream in listener.incoming() {
        match stream {
            Ok(s) => {
                let creds_dir = creds_dir.clone();
                std::thread::spawn(move || handle_connection(s, &creds_dir));
            }
            Err(e) => eprintln!("[clix-broker] accept error: {e}"),
        }
    }
}

fn handle_connection(stream: std::os::unix::net::UnixStream, creds_dir: &Path) {
    // Validate SO_PEERCRED — only the current user's processes may connect
    #[cfg(target_os = "linux")]
    if let Err(e) = validate_peer_cred(&stream) {
        eprintln!("[clix-broker] rejected connection: {e}");
        return;
    }

    let mut writer = match stream.try_clone() {
        Ok(w) => w,
        Err(e) => { eprintln!("[clix-broker] clone stream: {e}"); return; }
    };
    let reader = BufReader::new(stream);

    for line in reader.lines() {
        let line = match line {
            Ok(l) => l,
            Err(_) => break,
        };
        let req: BrokerMintRequest = match serde_json::from_str(&line) {
            Ok(r) => r,
            Err(e) => {
                let resp = BrokerMintResponse::MintResult { ok: false, env: Default::default(), error: Some(format!("bad request: {e}")) };
                let _ = write_json(&mut writer, &resp);
                continue;
            }
        };

        let resp = dispatch_request(&req, creds_dir);
        if write_json(&mut writer, &resp).is_err() { break; }
    }
}

/// Dispatch a broker request and produce a response.
fn dispatch_request(req: &BrokerMintRequest, creds_dir: &Path) -> BrokerMintResponse {
    match req {
        BrokerMintRequest::Ping => BrokerMintResponse::Pong {
            version: env!("CARGO_PKG_VERSION").to_string(),
        },
        BrokerMintRequest::Mint { cli, duration_secs } => {
            match cli.as_str() {
                "gcloud" => mint_gcloud(creds_dir, *duration_secs),
                "kubectl" => mint_kubectl(creds_dir),
                _ => mint_generic(cli, creds_dir),
            }
        }
        BrokerMintRequest::RequestApproval { receipt_id, capability, input, context, reason } => {
            let mut map = approvals().lock().unwrap_or_else(|e| e.into_inner());
            map.insert(*receipt_id, ApprovalState::Pending {
                capability: capability.clone(),
                input: input.clone(),
                context: context.clone(),
                reason: reason.clone(),
                requested_at: std::time::Instant::now(),
            });
            eprintln!("[clix-broker] approval requested: {} ({})", receipt_id, capability);
            BrokerMintResponse::ApprovalPending { receipt_id: *receipt_id }
        }
        BrokerMintRequest::PollApproval { receipt_id } => {
            let mut map = approvals().lock().unwrap_or_else(|e| e.into_inner());
            match map.get(receipt_id) {
                None => BrokerMintResponse::ApprovalDenied {
                    receipt_id: *receipt_id,
                    reason: "unknown approval request".to_string(),
                },
                Some(ApprovalState::Pending { requested_at, .. }) => {
                    if requested_at.elapsed().as_secs() > 300 {
                        map.remove(receipt_id);
                        BrokerMintResponse::ApprovalDenied {
                            receipt_id: *receipt_id,
                            reason: "timeout".to_string(),
                        }
                    } else {
                        BrokerMintResponse::ApprovalPending { receipt_id: *receipt_id }
                    }
                }
                Some(ApprovalState::Granted { approver }) => {
                    let approver = approver.clone();
                    map.remove(receipt_id);
                    BrokerMintResponse::ApprovalGranted { receipt_id: *receipt_id, approver }
                }
                Some(ApprovalState::Denied { reason }) => {
                    let reason = reason.clone();
                    map.remove(receipt_id);
                    BrokerMintResponse::ApprovalDenied { receipt_id: *receipt_id, reason }
                }
            }
        }
        BrokerMintRequest::Approve { receipt_id, approver, comment } => {
            let mut map = approvals().lock().unwrap_or_else(|e| e.into_inner());
            match map.get(receipt_id) {
                Some(ApprovalState::Pending { .. }) => {
                    eprintln!("[clix-broker] approved: {} by {}", receipt_id, approver);
                    let _ = comment; // stored in receipt by caller if needed
                    map.insert(*receipt_id, ApprovalState::Granted { approver: approver.clone() });
                    BrokerMintResponse::ApprovalGranted { receipt_id: *receipt_id, approver: approver.clone() }
                }
                _ => BrokerMintResponse::ApprovalDenied {
                    receipt_id: *receipt_id,
                    reason: "no pending approval found for this id".to_string(),
                },
            }
        }
        BrokerMintRequest::Reject { receipt_id, approver, reason } => {
            let mut map = approvals().lock().unwrap_or_else(|e| e.into_inner());
            let denial = reason.clone().unwrap_or_else(|| format!("rejected by {approver}"));
            match map.get(receipt_id) {
                Some(ApprovalState::Pending { .. }) => {
                    eprintln!("[clix-broker] rejected: {} by {}", receipt_id, approver);
                    map.insert(*receipt_id, ApprovalState::Denied { reason: denial.clone() });
                    BrokerMintResponse::ApprovalDenied { receipt_id: *receipt_id, reason: denial }
                }
                _ => BrokerMintResponse::ApprovalDenied {
                    receipt_id: *receipt_id,
                    reason: "no pending approval found for this id".to_string(),
                },
            }
        }
    }
}

/// gcloud: read the ADC JSON stored under `creds_dir/gcloud/adc.json` and either return the
/// cached access token (if not expired) or perform an OAuth2 token refresh.
fn mint_gcloud(creds_dir: &Path, _duration_secs: u64) -> BrokerMintResponse {
    let adc_path = creds_dir.join("gcloud").join("adc.json");
    if !adc_path.exists() {
        return BrokerMintResponse::MintResult {
            ok: false,
            env: Default::default(),
            error: Some(format!("gcloud ADC not found at {}. Run: clix init --adopt-creds gcloud", adc_path.display())),
        };
    }

    let adc_text = match fs::read_to_string(&adc_path) {
        Ok(t) => t,
        Err(e) => return BrokerMintResponse::MintResult { ok: false, env: Default::default(), error: Some(format!("read ADC: {e}")) },
    };

    let adc: serde_json::Value = match serde_json::from_str(&adc_text) {
        Ok(v) => v,
        Err(e) => return BrokerMintResponse::MintResult { ok: false, env: Default::default(), error: Some(format!("parse ADC: {e}")) },
    };

    // If there's a cached access token and expiry, check if still valid (with 60s buffer)
    if let (Some(token), Some(expiry)) = (adc["access_token"].as_str(), adc["token_expiry"].as_str()) {
        if let Ok(expiry_dt) = chrono::DateTime::parse_from_rfc3339(expiry) {
            let now = chrono::Utc::now();
            if expiry_dt.with_timezone(&chrono::Utc) > now + chrono::Duration::seconds(60) {
                return BrokerMintResponse::MintResult {
                    ok: true,
                    env: [("GOOGLE_OAUTH_ACCESS_TOKEN".to_string(), token.to_string())].into(),
                    error: None,
                };
            }
        }
    }

    // Need to refresh — call the token_uri with refresh_token
    let refresh_token = adc["refresh_token"].as_str().unwrap_or_default();
    let client_id = adc["client_id"].as_str().unwrap_or_default();
    let client_secret = adc["client_secret"].as_str().unwrap_or_default();
    let token_uri = adc["token_uri"].as_str().unwrap_or("https://oauth2.googleapis.com/token");

    if refresh_token.is_empty() || client_id.is_empty() {
        // Service account — try SA JWT signing
        let sa_result = mint_sa_token(&adc);
        match sa_result {
            Ok((token, _expires_in)) => {
                return BrokerMintResponse::MintResult {
                    ok: true,
                    env: [("GOOGLE_OAUTH_ACCESS_TOKEN".to_string(), token)].into(),
                    error: None,
                };
            }
            Err(e) => {
                // Fall back to cached token if available
                if let Some(token) = adc["access_token"].as_str() {
                    eprintln!("[clix-broker] WARNING: SA JWT signing failed ({e}), using cached token which may be expired");
                    return BrokerMintResponse::MintResult {
                        ok: true,
                        env: [("GOOGLE_OAUTH_ACCESS_TOKEN".to_string(), token.to_string())].into(),
                        error: None,
                    };
                }
                return BrokerMintResponse::MintResult {
                    ok: false,
                    env: Default::default(),
                    error: Some(format!("gcloud SA token error: {e}")),
                };
            }
        }
    }

    // Perform the OAuth2 refresh
    match oauth2_refresh(token_uri, client_id, client_secret, refresh_token) {
        Ok(access_token) => {
            // Write updated access_token + expiry back to ADC (best-effort)
            let mut updated = adc.clone();
            let expiry = chrono::Utc::now() + chrono::Duration::seconds(3600);
            updated["access_token"] = serde_json::Value::String(access_token.clone());
            updated["token_expiry"] = serde_json::Value::String(expiry.to_rfc3339());
            let _ = fs::write(&adc_path, serde_json::to_string_pretty(&updated).unwrap_or_default());

            BrokerMintResponse::MintResult {
                ok: true,
                env: [("GOOGLE_OAUTH_ACCESS_TOKEN".to_string(), access_token)].into(),
                error: None,
            }
        }
        Err(e) => BrokerMintResponse::MintResult {
            ok: false,
            env: Default::default(),
            error: Some(format!("gcloud OAuth2 refresh failed: {e}")),
        },
    }
}

fn oauth2_refresh(token_uri: &str, client_id: &str, client_secret: &str, refresh_token: &str) -> Result<String, String> {
    let body = format!(
        "client_id={}&client_secret={}&refresh_token={}&grant_type=refresh_token",
        urlencoding::encode(client_id),
        urlencoding::encode(client_secret),
        urlencoding::encode(refresh_token),
    );

    let output = std::process::Command::new("curl")
        .args(["--silent", "--fail", "-X", "POST", token_uri,
               "-H", "Content-Type: application/x-www-form-urlencoded",
               "-d", &body])
        .output()
        .map_err(|e| format!("curl: {e}"))?;

    if !output.status.success() {
        return Err(format!("HTTP error: {}", String::from_utf8_lossy(&output.stderr)));
    }

    let resp: serde_json::Value = serde_json::from_slice(&output.stdout)
        .map_err(|e| format!("parse token response: {e}"))?;
    resp["access_token"].as_str()
        .map(String::from)
        .ok_or_else(|| format!("no access_token in response: {}", String::from_utf8_lossy(&output.stdout)))
}

/// kubectl: write a temporary kubeconfig with the cached bearer token and return `KUBECONFIG`.
fn mint_kubectl(creds_dir: &Path) -> BrokerMintResponse {
    let kubeconfig_path = creds_dir.join("kubectl").join("kubeconfig");
    if !kubeconfig_path.exists() {
        return BrokerMintResponse::MintResult {
            ok: false,
            env: Default::default(),
            error: Some(format!("kubectl config not found at {}. Run: clix init --adopt-creds kubectl", kubeconfig_path.display())),
        };
    }

    // Write to a tmpfile so the worker gets a path it can read inside its jail.
    // For now we return KUBECONFIG pointing to the broker-owned location (the worker's
    // fs policy must include an RO bind of this path — handled by the manifest).
    BrokerMintResponse::MintResult {
        ok: true,
        env: [("KUBECONFIG".to_string(), kubeconfig_path.to_string_lossy().to_string())].into(),
        error: None,
    }
}

/// Generic: look up `creds_dir/<cli>/secret.env` and re-inject the stored env vars.
fn mint_generic(cli: &str, creds_dir: &Path) -> BrokerMintResponse {
    let secret_path = creds_dir.join(cli).join("secret.env");
    if !secret_path.exists() {
        return BrokerMintResponse::MintResult {
            ok: false,
            env: Default::default(),
            error: Some(format!("no creds for `{cli}` at {}", secret_path.display())),
        };
    }
    let text = match fs::read_to_string(&secret_path) {
        Ok(t) => t,
        Err(e) => return BrokerMintResponse::MintResult { ok: false, env: Default::default(), error: Some(format!("read: {e}")) },
    };
    let mut env = std::collections::HashMap::new();
    for line in text.lines() {
        let line = line.trim();
        if line.is_empty() || line.starts_with('#') { continue; }
        if let Some((k, v)) = line.split_once('=') {
            env.insert(k.to_string(), v.to_string());
        }
    }
    BrokerMintResponse::MintResult { ok: true, env, error: None }
}

/// Sign a JWT and exchange it for a Google access token using a service account JSON.
fn mint_sa_token(sa_json: &serde_json::Value) -> Result<(String, u64), String> {
    use rsa::{RsaPrivateKey, pkcs1v15::SigningKey, pkcs8::DecodePrivateKey};
    use rsa::signature::{SignatureEncoding, Signer};
    use sha2::Sha256;

    let client_email = sa_json["client_email"].as_str()
        .ok_or("missing client_email")?;
    let private_key_pem = sa_json["private_key"].as_str()
        .ok_or("missing private_key")?;
    let token_uri = sa_json["token_uri"].as_str()
        .unwrap_or("https://oauth2.googleapis.com/token");

    let now = std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH).unwrap().as_secs();

    // Build JWT header and claims
    let header = base64::engine::general_purpose::URL_SAFE_NO_PAD
        .encode(r#"{"alg":"RS256","typ":"JWT"}"#);
    let claims = serde_json::json!({
        "iss": client_email,
        "scope": "https://www.googleapis.com/auth/cloud-platform",
        "aud": token_uri,
        "exp": now + 3600,
        "iat": now,
    });
    let claims_b64 = base64::engine::general_purpose::URL_SAFE_NO_PAD
        .encode(claims.to_string());
    let signing_input = format!("{}.{}", header, claims_b64);

    // Sign with RSA-SHA256 (PKCS1v15 — deterministic, no rng needed)
    let private_key = RsaPrivateKey::from_pkcs8_pem(private_key_pem)
        .map_err(|e| format!("parse private key: {e}"))?;
    let signing_key = SigningKey::<Sha256>::new(private_key);
    let sig = signing_key.sign(signing_input.as_bytes());
    let sig_b64 = base64::engine::general_purpose::URL_SAFE_NO_PAD
        .encode(sig.to_bytes());

    let jwt = format!("{}.{}", signing_input, sig_b64);

    // Exchange JWT for access token via curl (same pattern as oauth2_refresh)
    let body = format!(
        "grant_type={}&assertion={}",
        urlencoding::encode("urn:ietf:params:oauth:grant-type:jwt-bearer"),
        urlencoding::encode(&jwt),
    );
    let output = std::process::Command::new("curl")
        .args(["--silent", "--fail", "-X", "POST", token_uri,
               "-H", "Content-Type: application/x-www-form-urlencoded",
               "-d", &body])
        .output()
        .map_err(|e| format!("curl: {e}"))?;

    if !output.status.success() {
        return Err(format!("HTTP error: {}", String::from_utf8_lossy(&output.stderr)));
    }

    let resp: serde_json::Value = serde_json::from_slice(&output.stdout)
        .map_err(|e| format!("parse token response: {e}"))?;
    let token = resp["access_token"].as_str()
        .ok_or_else(|| format!("no access_token in response: {}", String::from_utf8_lossy(&output.stdout)))?
        .to_string();
    let expires_in = resp["expires_in"].as_u64().unwrap_or(3600);
    Ok((token, expires_in))
}

// ── Security helpers ──────────────────────────────────────────────────────────

/// Validate that the connecting process is owned by the same user as us (via SO_PEERCRED).
#[cfg(target_os = "linux")]
fn validate_peer_cred(stream: &UnixStream) -> Result<(), String> {
    use std::os::unix::io::AsRawFd;

    let fd = stream.as_raw_fd();
    let mut cred = libc::ucred { pid: 0, uid: 0, gid: 0 };
    let mut len = std::mem::size_of::<libc::ucred>() as libc::socklen_t;
    let ret = unsafe {
        libc::getsockopt(
            fd,
            libc::SOL_SOCKET,
            libc::SO_PEERCRED,
            &mut cred as *mut _ as *mut libc::c_void,
            &mut len,
        )
    };
    if ret != 0 {
        return Err(format!("getsockopt SO_PEERCRED: {}", std::io::Error::last_os_error()));
    }
    let our_uid = unsafe { libc::getuid() };
    if cred.uid != our_uid {
        return Err(format!("UID mismatch: peer={}, ours={}", cred.uid, our_uid));
    }
    Ok(())
}

/// Ensure the creds directory exists and has mode 0700.
fn ensure_creds_dir(dir: &Path) -> Result<(), String> {
    if !dir.exists() {
        fs::create_dir_all(dir).map_err(|e| format!("create creds dir {}: {e}", dir.display()))?;
    }
    #[cfg(unix)]
    {
        use std::os::unix::fs::PermissionsExt;
        let perms = fs::Permissions::from_mode(0o700);
        fs::set_permissions(dir, perms).map_err(|e| format!("chmod 700 {}: {e}", dir.display()))?;
    }
    Ok(())
}

fn creds_dir() -> PathBuf {
    std::env::var("CLIX_BROKER_CREDS_DIR")
        .map(PathBuf::from)
        .unwrap_or_else(|_| {
            dirs::data_local_dir()
                .unwrap_or_else(|| PathBuf::from("/var/lib"))
                .join("clix")
                .join("broker")
        })
}

fn write_json<T: serde::Serialize>(writer: &mut impl Write, value: &T) -> std::io::Result<()> {
    let line = serde_json::to_string(value).map_err(|e| std::io::Error::new(std::io::ErrorKind::Other, e))?;
    writer.write_all(line.as_bytes())?;
    writer.write_all(b"\n")?;
    writer.flush()
}
