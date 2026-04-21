use std::path::Path;
use serde::{Deserialize, Serialize};
use uuid::Uuid;
use chrono::{DateTime, Utc};
use crate::error::{ClixError, Result};

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct Receipt {
    pub id: Uuid,
    pub kind: ReceiptKind,
    pub capability: String,
    pub created_at: DateTime<Utc>,
    pub status: ReceiptStatus,
    pub decision: String,
    pub reason: Option<String>,
    pub input: serde_json::Value,
    pub context: serde_json::Value,
    pub execution: Option<serde_json::Value>,
    pub approval: Option<serde_json::Value>,
    pub sandbox_enforced: bool,
    /// Which isolation tier was used for this execution.
    #[serde(default)]
    pub isolation_tier: Option<String>,
    /// SHA-256 hex digest of the CLI binary that was executed.
    #[serde(default)]
    pub binary_sha256: Option<String>,
    /// Opaque ID of the broker token mint used for credentials (for audit correlation).
    #[serde(default)]
    pub token_mint_id: Option<String>,
    /// SHA-256 digest of the JailConfig used (for reproducibility auditing).
    #[serde(default)]
    pub jail_config_digest: Option<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub enum ReceiptKind { Capability, Workflow }

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub enum ReceiptStatus { Succeeded, Failed, Denied, PendingApproval, ApprovalDenied }

impl std::fmt::Display for ReceiptStatus {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            ReceiptStatus::Succeeded => write!(f, "succeeded"),
            ReceiptStatus::Failed => write!(f, "failed"),
            ReceiptStatus::Denied => write!(f, "denied"),
            ReceiptStatus::PendingApproval => write!(f, "pending_approval"),
            ReceiptStatus::ApprovalDenied => write!(f, "approval_denied"),
        }
    }
}

pub struct ReceiptStore {
    conn: rusqlite::Connection,
}

impl ReceiptStore {
    pub fn open(db_path: &Path) -> Result<Self> {
        let conn = rusqlite::Connection::open(db_path)
            .map_err(|e| ClixError::Config(format!("receipts db: {e}")))?;
        conn.busy_timeout(std::time::Duration::from_millis(100))
            .map_err(|e| ClixError::Config(format!("receipts busy_timeout: {e}")))?;
        conn.execute_batch(include_str!("schema.sql"))
            .map_err(|e| ClixError::Config(format!("receipts schema: {e}")))?;
        Ok(ReceiptStore { conn })
    }

    pub fn write(&self, receipt: &Receipt) -> Result<()> {
        self.conn.execute(
            "INSERT INTO receipts (id,kind,capability,created_at,status,decision,reason,input,context,execution,approval,sandbox_enforced,isolation_tier,binary_sha256,token_mint_id,jail_config_digest) VALUES (?1,?2,?3,?4,?5,?6,?7,?8,?9,?10,?11,?12,?13,?14,?15,?16)",
            rusqlite::params![
                receipt.id.to_string(),
                serde_json::to_string(&receipt.kind).unwrap(),
                receipt.capability,
                receipt.created_at.to_rfc3339(),
                receipt.status.to_string(),
                receipt.decision,
                receipt.reason,
                serde_json::to_string(&receipt.input).unwrap(),
                serde_json::to_string(&receipt.context).unwrap(),
                receipt.execution.as_ref().map(|e| serde_json::to_string(e).unwrap()),
                receipt.approval.as_ref().map(|a| serde_json::to_string(a).unwrap()),
                receipt.sandbox_enforced as i64,
                receipt.isolation_tier,
                receipt.binary_sha256,
                receipt.token_mint_id,
                receipt.jail_config_digest,
            ],
        ).map_err(|e| ClixError::Config(format!("receipt insert: {e}")))?;
        Ok(())
    }

    pub fn list(&self, limit: usize, status_filter: Option<&str>) -> Result<Vec<Receipt>> {
        let mut stmt = self.conn.prepare(
            "SELECT id,kind,capability,created_at,status,decision,reason,input,context,execution,approval,sandbox_enforced,isolation_tier,binary_sha256,token_mint_id,jail_config_digest FROM receipts WHERE (?1 IS NULL OR status = ?1) ORDER BY created_at DESC LIMIT ?2"
        ).map_err(|e| ClixError::Config(e.to_string()))?;
        let rows = stmt.query_map(rusqlite::params![status_filter, limit as i64], |row| {
            Ok((row.get::<_,String>(0)?, row.get::<_,String>(1)?, row.get::<_,String>(2)?,
                row.get::<_,String>(3)?, row.get::<_,String>(4)?, row.get::<_,String>(5)?,
                row.get::<_,Option<String>>(6)?, row.get::<_,String>(7)?, row.get::<_,String>(8)?,
                row.get::<_,Option<String>>(9)?, row.get::<_,Option<String>>(10)?, row.get::<_,i64>(11)?,
                row.get::<_,Option<String>>(12)?, row.get::<_,Option<String>>(13)?,
                row.get::<_,Option<String>>(14)?, row.get::<_,Option<String>>(15)?))
        }).map_err(|e| ClixError::Config(e.to_string()))?;
        let mut receipts = vec![];
        for row in rows {
            let (id,kind,cap,created_at,status,decision,reason,input,context,execution,approval,sandbox_enforced,
                 isolation_tier,binary_sha256,token_mint_id,jail_config_digest) =
                row.map_err(|e| ClixError::Config(e.to_string()))?;
            receipts.push(Receipt {
                id: Uuid::parse_str(&id).unwrap_or_else(|_| Uuid::new_v4()),
                kind: serde_json::from_str(&kind).unwrap_or(ReceiptKind::Capability),
                capability: cap,
                created_at: created_at.parse().unwrap_or_else(|_| Utc::now()),
                status: parse_status(&status),
                decision, reason,
                input: serde_json::from_str(&input).unwrap_or(serde_json::Value::Null),
                context: serde_json::from_str(&context).unwrap_or(serde_json::Value::Null),
                execution: execution.as_deref().and_then(|s| serde_json::from_str(s).ok()),
                approval: approval.as_deref().and_then(|s| serde_json::from_str(s).ok()),
                sandbox_enforced: sandbox_enforced != 0,
                isolation_tier, binary_sha256, token_mint_id, jail_config_digest,
            });
        }
        Ok(receipts)
    }

    /// Export all receipts optionally filtered by status and/or a minimum created_at timestamp.
    /// Returns receipts in ascending chronological order.  No LIMIT — export is unbounded.
    pub fn export(&self, status_filter: Option<&str>, since: Option<DateTime<Utc>>) -> Result<Vec<Receipt>> {
        let since_str = since.map(|dt| dt.to_rfc3339());
        let mut stmt = self.conn.prepare(
            "SELECT id,kind,capability,created_at,status,decision,reason,input,context,execution,approval,sandbox_enforced,isolation_tier,binary_sha256,token_mint_id,jail_config_digest \
             FROM receipts \
             WHERE (?1 IS NULL OR status = ?1) AND (?2 IS NULL OR created_at >= ?2) \
             ORDER BY created_at ASC"
        ).map_err(|e| ClixError::Config(e.to_string()))?;
        let rows = stmt.query_map(rusqlite::params![status_filter, since_str], |row| {
            Ok((row.get::<_,String>(0)?, row.get::<_,String>(1)?, row.get::<_,String>(2)?,
                row.get::<_,String>(3)?, row.get::<_,String>(4)?, row.get::<_,String>(5)?,
                row.get::<_,Option<String>>(6)?, row.get::<_,String>(7)?, row.get::<_,String>(8)?,
                row.get::<_,Option<String>>(9)?, row.get::<_,Option<String>>(10)?, row.get::<_,i64>(11)?,
                row.get::<_,Option<String>>(12)?, row.get::<_,Option<String>>(13)?,
                row.get::<_,Option<String>>(14)?, row.get::<_,Option<String>>(15)?))
        }).map_err(|e| ClixError::Config(e.to_string()))?;
        let mut receipts = vec![];
        for row in rows {
            let (id,kind,cap,created_at,status,decision,reason,input,context,execution,approval,sandbox_enforced,
                 isolation_tier,binary_sha256,token_mint_id,jail_config_digest) =
                row.map_err(|e| ClixError::Config(e.to_string()))?;
            receipts.push(Receipt {
                id: Uuid::parse_str(&id).unwrap_or_else(|_| Uuid::new_v4()),
                kind: serde_json::from_str(&kind).unwrap_or(ReceiptKind::Capability),
                capability: cap,
                created_at: created_at.parse().unwrap_or_else(|_| Utc::now()),
                status: parse_status(&status),
                decision, reason,
                input: serde_json::from_str(&input).unwrap_or(serde_json::Value::Null),
                context: serde_json::from_str(&context).unwrap_or(serde_json::Value::Null),
                execution: execution.as_deref().and_then(|s| serde_json::from_str(s).ok()),
                approval: approval.as_deref().and_then(|s| serde_json::from_str(s).ok()),
                sandbox_enforced: sandbox_enforced != 0,
                isolation_tier, binary_sha256, token_mint_id, jail_config_digest,
            });
        }
        Ok(receipts)
    }

    /// Count receipts grouped by status.  Returns (total, succeeded, denied, failed, pending_approval).
    pub fn count_by_status(&self) -> Result<(usize, usize, usize, usize, usize)> {
        let total: i64 = self.conn.query_row("SELECT COUNT(*) FROM receipts", [], |r| r.get(0))
            .map_err(|e| ClixError::Config(e.to_string()))?;
        let count = |s: &str| -> Result<i64> {
            self.conn.query_row(
                "SELECT COUNT(*) FROM receipts WHERE status = ?1",
                rusqlite::params![s], |r| r.get(0))
                .map_err(|e| ClixError::Config(e.to_string()))
        };
        Ok((total as usize, count("succeeded")? as usize, count("denied")? as usize,
            count("failed")? as usize, count("pending_approval")? as usize))
    }

    pub fn get(&self, id: &str) -> Result<Option<Receipt>> {
        let mut stmt = self.conn.prepare(
            "SELECT id,kind,capability,created_at,status,decision,reason,input,context,execution,approval,sandbox_enforced,isolation_tier,binary_sha256,token_mint_id,jail_config_digest FROM receipts WHERE id = ?1"
        ).map_err(|e| ClixError::Config(e.to_string()))?;
        let row = stmt.query_row(rusqlite::params![id], |row| {
            Ok((row.get::<_,String>(0)?, row.get::<_,String>(1)?, row.get::<_,String>(2)?,
                row.get::<_,String>(3)?, row.get::<_,String>(4)?, row.get::<_,String>(5)?,
                row.get::<_,Option<String>>(6)?, row.get::<_,String>(7)?, row.get::<_,String>(8)?,
                row.get::<_,Option<String>>(9)?, row.get::<_,Option<String>>(10)?, row.get::<_,i64>(11)?,
                row.get::<_,Option<String>>(12)?, row.get::<_,Option<String>>(13)?,
                row.get::<_,Option<String>>(14)?, row.get::<_,Option<String>>(15)?))
        });
        match row {
            Err(rusqlite::Error::QueryReturnedNoRows) => Ok(None),
            Err(e) => Err(ClixError::Config(e.to_string())),
            Ok((id,kind,cap,created_at,status,decision,reason,input,context,execution,approval,sandbox_enforced,
                isolation_tier,binary_sha256,token_mint_id,jail_config_digest)) => {
                Ok(Some(Receipt {
                    id: Uuid::parse_str(&id).unwrap_or_else(|_| Uuid::new_v4()),
                    kind: serde_json::from_str(&kind).unwrap_or(ReceiptKind::Capability),
                    capability: cap,
                    created_at: created_at.parse().unwrap_or_else(|_| Utc::now()),
                    status: parse_status(&status),
                    decision, reason,
                    input: serde_json::from_str(&input).unwrap_or(serde_json::Value::Null),
                    context: serde_json::from_str(&context).unwrap_or(serde_json::Value::Null),
                    execution: execution.as_deref().and_then(|s| serde_json::from_str(s).ok()),
                    approval: approval.as_deref().and_then(|s| serde_json::from_str(s).ok()),
                    sandbox_enforced: sandbox_enforced != 0,
                    isolation_tier, binary_sha256, token_mint_id, jail_config_digest,
                }))
            }
        }
    }
}

fn parse_status(s: &str) -> ReceiptStatus {
    match s {
        "succeeded" => ReceiptStatus::Succeeded,
        "failed" => ReceiptStatus::Failed,
        "denied" => ReceiptStatus::Denied,
        "pending_approval" => ReceiptStatus::PendingApproval,
        "approval_denied" => ReceiptStatus::ApprovalDenied,
        _ => ReceiptStatus::Failed,
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn store() -> ReceiptStore { ReceiptStore::open(Path::new(":memory:")).unwrap() }

    fn stub(cap: &str, status: ReceiptStatus) -> Receipt {
        Receipt {
            id: Uuid::new_v4(), kind: ReceiptKind::Capability, capability: cap.to_string(),
            created_at: Utc::now(), status, decision: "allow".to_string(), reason: None,
            input: serde_json::json!({}), context: serde_json::json!({}),
            execution: None, approval: None, sandbox_enforced: false,
            isolation_tier: None, binary_sha256: None, token_mint_id: None, jail_config_digest: None,
        }
    }

    #[test]
    fn test_write_and_list() {
        let s = store();
        s.write(&stub("sys.date", ReceiptStatus::Succeeded)).unwrap();
        let list = s.list(10, None).unwrap();
        assert_eq!(list.len(), 1);
        assert_eq!(list[0].capability, "sys.date");
    }

    #[test]
    fn test_status_filter() {
        let s = store();
        s.write(&stub("a", ReceiptStatus::Succeeded)).unwrap();
        s.write(&stub("b", ReceiptStatus::Failed)).unwrap();
        let succeeded = s.list(10, Some("succeeded")).unwrap();
        assert_eq!(succeeded.len(), 1);
        assert_eq!(succeeded[0].capability, "a");
    }

    #[test]
    fn test_get_by_id() {
        let s = store();
        let r = stub("sys.echo", ReceiptStatus::Succeeded);
        let id = r.id.to_string();
        s.write(&r).unwrap();
        assert!(s.get(&id).unwrap().is_some());
    }
}
