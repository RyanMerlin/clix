use anyhow::Result;
use uuid::Uuid;
use clix_core::execution::broker_client::BrokerClient;

pub fn approve(receipt_id: &str, approver: &str, comment: Option<&str>) -> Result<()> {
    let id = receipt_id.parse::<Uuid>()
        .map_err(|_| anyhow::anyhow!("invalid receipt id: {receipt_id}"))?;
    let mut client = BrokerClient::connect()
        .map_err(|e| anyhow::anyhow!("cannot connect to broker: {e}"))?;
    client.send_approve(id, approver.to_string(), comment.map(|s| s.to_string()))
        .map_err(|e| anyhow::anyhow!("broker error: {e}"))?;
    println!("approved: {receipt_id} (by {approver})");
    Ok(())
}

pub fn reject(receipt_id: &str, approver: &str, reason: Option<&str>) -> Result<()> {
    let id = receipt_id.parse::<Uuid>()
        .map_err(|_| anyhow::anyhow!("invalid receipt id: {receipt_id}"))?;
    let mut client = BrokerClient::connect()
        .map_err(|e| anyhow::anyhow!("cannot connect to broker: {e}"))?;
    client.send_reject(id, approver.to_string(), reason.map(|s| s.to_string()))
        .map_err(|e| anyhow::anyhow!("broker error: {e}"))?;
    println!("rejected: {receipt_id} (by {approver})");
    Ok(())
}
