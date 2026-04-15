/// Wire protocol for communication between the clix gateway and a clix-worker process.
///
/// All messages are newline-delimited JSON written over a Unix stream socket.
///
/// Gateway → Worker:
///   `WorkerRequest` for each capability invocation.
///
/// Worker → Gateway:
///   One or more `WorkerEvent` messages per request, terminated by `WorkerEvent::Exit`.
use std::collections::HashMap;
use serde::{Deserialize, Serialize};

/// A single capability dispatch sent from the gateway to a warm worker.
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct WorkerRequest {
    /// Unique request ID — echoed back in all response events so the gateway can correlate.
    pub request_id: String,
    /// Full argv; argv[0] is the absolute pinned binary path.
    pub argv: Vec<String>,
    /// Additional environment variables to inject (e.g. ephemeral credentials from the broker).
    /// These are merged on top of the worker's minimal baseline env.
    pub env: HashMap<String, String>,
    /// Working directory for the subprocess.
    pub cwd: String,
    /// If true, the worker should send stdout/stderr lines as streaming `WorkerEvent::Stdout` /
    /// `WorkerEvent::Stderr` messages. If false, buffers and sends a single `WorkerEvent::Exit`.
    #[serde(default)]
    pub streaming: bool,
}

/// Events streamed back from the worker to the gateway.
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(tag = "type", rename_all = "camelCase")]
pub enum WorkerEvent {
    /// A chunk of stdout data (only sent when `streaming: true`).
    #[serde(rename = "stdout")]
    Stdout { request_id: String, data: String },
    /// A chunk of stderr data (only sent when `streaming: true`).
    #[serde(rename = "stderr")]
    Stderr { request_id: String, data: String },
    /// Terminal event — always sent exactly once per request.
    #[serde(rename = "exit")]
    Exit {
        request_id: String,
        exit_code: i32,
        /// Buffered stdout (populated when `streaming: false`).
        #[serde(default)]
        stdout: String,
        /// Buffered stderr (populated when `streaming: false`).
        #[serde(default)]
        stderr: String,
    },
    /// Worker-level error (e.g. binary hash mismatch, exec failure).
    #[serde(rename = "error")]
    Error { request_id: String, message: String },
}

/// Health-check message sent by the gateway to a worker on startup.
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct WorkerHandshake {
    /// Absolute path to the CLI binary this worker was configured to run.
    pub pinned_binary: String,
    /// Expected SHA-256 hex digest of the binary. Worker must verify before accepting requests.
    pub binary_sha256: String,
}

/// Worker's handshake response.
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct WorkerReady {
    pub ok: bool,
    /// Populated if `ok: false`.
    #[serde(default)]
    pub error: Option<String>,
}

/// Request from the gateway to the broker daemon.
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(tag = "type", rename_all = "camelCase")]
pub enum BrokerMintRequest {
    /// Mint ephemeral credentials for the given CLI.
    #[serde(rename = "mint")]
    Mint {
        /// The CLI identifier (e.g. "gcloud", "kubectl").
        cli: String,
        /// How long the minted token should be valid for (seconds). Broker may cap this.
        #[serde(default = "default_duration")]
        duration_secs: u64,
    },
    /// Health-check ping — broker responds with Pong.
    #[serde(rename = "ping")]
    Ping,
    /// Request a human approval for a pending capability execution.
    #[serde(rename = "requestApproval")]
    RequestApproval {
        receipt_id: uuid::Uuid,
        capability: String,
        input: serde_json::Value,
        context: serde_json::Value,
        reason: String,
    },
    /// Poll the approval state for a previously submitted request.
    #[serde(rename = "pollApproval")]
    PollApproval {
        receipt_id: uuid::Uuid,
    },
    /// Grant a pending approval.
    #[serde(rename = "approve")]
    Approve {
        receipt_id: uuid::Uuid,
        approver: String,
        #[serde(default)]
        comment: Option<String>,
    },
    /// Reject a pending approval.
    #[serde(rename = "reject")]
    Reject {
        receipt_id: uuid::Uuid,
        approver: String,
        #[serde(default)]
        reason: Option<String>,
    },
}

fn default_duration() -> u64 { 3600 }

/// Response from the broker.
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(tag = "type", rename_all = "camelCase")]
pub enum BrokerMintResponse {
    /// Ephemeral credential set minted successfully (or error).
    #[serde(rename = "mintResult")]
    MintResult {
        ok: bool,
        /// Env vars (e.g. `{"GOOGLE_OAUTH_ACCESS_TOKEN": "ya29.xxx"}`).
        #[serde(default)]
        env: HashMap<String, String>,
        #[serde(default)]
        error: Option<String>,
    },
    /// Response to a Ping request.
    #[serde(rename = "pong")]
    Pong {
        version: String,
    },
    /// Approval request registered; waiting for human decision.
    #[serde(rename = "approvalPending")]
    ApprovalPending {
        receipt_id: uuid::Uuid,
    },
    /// Approval granted.
    #[serde(rename = "approvalGranted")]
    ApprovalGranted {
        receipt_id: uuid::Uuid,
        approver: String,
    },
    /// Approval denied.
    #[serde(rename = "approvalDenied")]
    ApprovalDenied {
        receipt_id: uuid::Uuid,
        reason: String,
    },
}

impl BrokerMintResponse {
    /// Convenience: extract (ok, env, error) from a MintResult variant.
    pub fn into_mint_result(self) -> Option<(bool, HashMap<String, String>, Option<String>)> {
        match self {
            BrokerMintResponse::MintResult { ok, env, error } => Some((ok, env, error)),
            _ => None,
        }
    }
}
