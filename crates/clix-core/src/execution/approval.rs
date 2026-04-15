use serde::{Deserialize, Serialize};
use uuid::Uuid;
use crate::error::{ClixError, Result};
use crate::manifest::capability::CapabilityManifest;
use crate::state::ApprovalGateConfig;
use super::broker_client::{ApprovalPollResult, BrokerClient};
use super::ExecutionOutcome;

#[derive(Debug, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct ApprovalRequest {
    pub request_id: String,
    pub capability: String,
    pub input: serde_json::Value,
    pub risk: String,
    pub reason: String,
    pub created_at: String,
}

#[derive(Debug, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct ApprovalResponse {
    pub approved: bool,
    #[serde(default)]
    pub reason: Option<String>,
    #[serde(default)]
    pub approver: Option<String>,
}

impl ApprovalResponse {
    pub fn denied(reason: impl Into<String>) -> Self {
        ApprovalResponse { approved: false, reason: Some(reason.into()), approver: None }
    }
}

pub fn request_approval(cfg: &ApprovalGateConfig, cap: &CapabilityManifest, input: &serde_json::Value, policy_reason: &str) -> Result<ApprovalResponse> {
    let timeout = if cfg.timeout_seconds > 0 { cfg.timeout_seconds } else { 300 };
    let req = ApprovalRequest {
        request_id: Uuid::new_v4().to_string(),
        capability: cap.name.clone(),
        input: input.clone(),
        risk: format!("{:?}", cap.risk).to_lowercase(),
        reason: policy_reason.to_string(),
        created_at: chrono::Utc::now().to_rfc3339(),
    };
    let client = reqwest::blocking::Client::builder()
        .timeout(std::time::Duration::from_secs(timeout))
        .build().map_err(|e| ClixError::ApprovalGate(e.to_string()))?;
    let mut builder = client.post(&cfg.webhook_url).json(&req);
    for (k, v) in &cfg.headers { builder = builder.header(k, v); }
    let http_resp = match builder.send() {
        Err(e) => return Ok(ApprovalResponse::denied(format!("webhook unreachable: {e}"))),
        Ok(r) => r,
    };
    if !http_resp.status().is_success() {
        return Ok(ApprovalResponse::denied(format!("webhook HTTP {}", http_resp.status())));
    }
    match http_resp.json::<ApprovalResponse>() {
        Ok(r) => Ok(r),
        Err(e) => Ok(ApprovalResponse::denied(format!("webhook decode failed: {e}"))),
    }
}

/// Wait for broker-based approval, polling every 2 seconds up to 300 seconds.
/// Returns an ExecutionOutcome reflecting the approval decision.
pub fn wait_for_broker_approval(
    receipt_id: Uuid,
    capability: &str,
    input: &serde_json::Value,
    ctx_value: &serde_json::Value,
    reason: &str,
) -> Result<ExecutionOutcome> {
    let mut client = BrokerClient::connect().map_err(|e| {
        ClixError::Broker(format!("cannot connect to broker for approval: {e}"))
    })?;

    client.send_request_approval(receipt_id, capability, input, ctx_value, reason)?;

    let deadline = std::time::Instant::now() + std::time::Duration::from_secs(300);
    loop {
        std::thread::sleep(std::time::Duration::from_secs(2));
        if std::time::Instant::now() > deadline {
            return Ok(ExecutionOutcome {
                ok: false,
                approval_required: true,
                receipt_id,
                result: None,
                reason: Some("approval timeout".to_string()),
            });
        }
        match client.poll_approval(receipt_id)? {
            ApprovalPollResult::Pending => continue,
            ApprovalPollResult::Granted { .. } => {
                return Ok(ExecutionOutcome {
                    ok: true,
                    approval_required: false,
                    receipt_id,
                    result: None,
                    reason: None,
                });
            }
            ApprovalPollResult::Denied { reason: denial_reason } => {
                return Ok(ExecutionOutcome {
                    ok: false,
                    approval_required: false,
                    receipt_id,
                    result: None,
                    reason: Some(denial_reason),
                });
            }
        }
    }
}
