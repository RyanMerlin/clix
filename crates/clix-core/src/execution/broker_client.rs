/// Thin client for the clix-broker credential daemon.
///
/// Called by the gateway immediately before each worker dispatch to obtain ephemeral
/// credentials (e.g. a short-lived GOOGLE_OAUTH_ACCESS_TOKEN). The minted tokens are
/// merged into `WorkerRequest.env` so the jailed worker receives them without ever
/// touching the adopting credential files.
///
/// Errors are non-fatal: if the broker is unavailable, the gateway logs a warning and
/// falls back to whatever static secrets are already in the request env.
use std::collections::HashMap;
use std::io::{BufRead, BufReader, Write};
use std::os::unix::net::UnixStream;
use std::path::Path;
use crate::error::Result;
use super::worker_protocol::{BrokerMintRequest, BrokerMintResponse};

const DEFAULT_BROKER_SOCKET: &str = "/tmp/clix-broker.sock";

/// Return the broker socket path from `CLIX_BROKER_SOCKET` env var, or the default.
pub fn broker_socket_path() -> std::path::PathBuf {
    std::env::var("CLIX_BROKER_SOCKET")
        .map(std::path::PathBuf::from)
        .unwrap_or_else(|_| std::path::PathBuf::from(DEFAULT_BROKER_SOCKET))
}

/// Try to mint credentials from the broker for the given CLI name (e.g. "gcloud", "kubectl").
///
/// Returns an empty map on any error (broker down, no creds for this CLI, etc.) — the caller
/// should log the warning already printed here and continue with static secrets only.
pub fn mint_credentials(socket_path: &Path, cli: &str) -> Result<HashMap<String, String>> {
    let stream = match UnixStream::connect(socket_path) {
        Ok(s) => s,
        Err(e) => {
            // Broker not running — not a hard failure, may be using static secrets
            eprintln!("[clix-gateway] broker not available at {}: {e}", socket_path.display());
            return Ok(HashMap::new());
        }
    };

    let req = BrokerMintRequest { cli: cli.to_string(), duration_secs: 3600 };
    let mut writer = stream.try_clone()
        .map_err(|e| crate::error::ClixError::Broker(format!("clone broker stream: {e}")))?;
    let msg = serde_json::to_string(&req)? + "\n";
    writer.write_all(msg.as_bytes())
        .map_err(|e| crate::error::ClixError::Broker(format!("write broker request: {e}")))?;
    writer.flush().ok();

    let mut reader = BufReader::new(stream);
    let mut line = String::new();
    reader.read_line(&mut line)
        .map_err(|e| crate::error::ClixError::Broker(format!("read broker response: {e}")))?;

    let resp: BrokerMintResponse = serde_json::from_str(line.trim())
        .map_err(|e| crate::error::ClixError::Broker(format!("parse broker response: {e}")))?;

    if !resp.ok {
        let err = resp.error.unwrap_or_else(|| "unknown error".to_string());
        // Not all CLIs have broker-adopted creds — this is expected for generic tools
        eprintln!("[clix-gateway] broker mint for '{cli}': {err}");
        return Ok(HashMap::new());
    }

    Ok(resp.env)
}

/// Extract the CLI name from a command string (basename without path or extension).
/// `"/usr/bin/gcloud"` → `"gcloud"`, `"kubectl"` → `"kubectl"`.
pub fn cli_name_from_command(command: &str) -> &str {
    std::path::Path::new(command)
        .file_stem()
        .and_then(|s| s.to_str())
        .unwrap_or(command)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_cli_name_from_command() {
        assert_eq!(cli_name_from_command("gcloud"), "gcloud");
        assert_eq!(cli_name_from_command("/usr/bin/gcloud"), "gcloud");
        assert_eq!(cli_name_from_command("kubectl"), "kubectl");
        assert_eq!(cli_name_from_command("/usr/local/bin/kubectl"), "kubectl");
    }
}
