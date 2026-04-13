use std::sync::{Arc, Mutex};
use tokio::io::{AsyncBufReadExt, AsyncWriteExt, BufReader};
use crate::dispatch::{dispatch, ServeState};

pub async fn process_line(serve: Arc<ServeState>, line: &str) -> Option<String> {
    if line.trim().is_empty() { return None; }
    let req: serde_json::Value = match serde_json::from_str(line) {
        Ok(v) => v,
        Err(e) => {
            let resp = serde_json::json!({"jsonrpc":"2.0","id":null,"error":{"code":-32700,"message":format!("parse error: {e}")}});
            return Some(serde_json::to_string(&resp).unwrap_or_default());
        }
    };
    let resp = dispatch(serve, req).await;
    Some(serde_json::to_string(&resp).unwrap_or_default())
}

pub async fn serve_stdio(serve: Arc<ServeState>) -> anyhow::Result<()> {
    let mut reader = BufReader::new(tokio::io::stdin());
    let mut writer = tokio::io::BufWriter::new(tokio::io::stdout());
    let mut line = String::new();
    loop {
        line.clear();
        let n = reader.read_line(&mut line).await?;
        if n == 0 { break; }
        if let Some(resp) = process_line(Arc::clone(&serve), &line).await {
            writer.write_all(resp.as_bytes()).await?;
            writer.write_all(b"\n").await?;
            writer.flush().await?;
        }
    }
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;
    use clix_core::policy::PolicyBundle;
    use clix_core::receipts::ReceiptStore;
    use clix_core::registry::{CapabilityRegistry, WorkflowRegistry};
    use clix_core::state::ClixState;

    fn make_serve() -> Arc<ServeState> {
        let home = std::env::temp_dir().join("clix-stdio-test");
        std::fs::create_dir_all(&home).unwrap();
        Arc::new(ServeState {
            cap_registry: CapabilityRegistry::from_vec(vec![]),
            wf_registry:  WorkflowRegistry::from_vec(vec![]),
            policy:       PolicyBundle::default(),
            store:        Mutex::new(ReceiptStore::open(&home.join("receipts.db")).unwrap()),
            state:        ClixState::from_home(home),
        })
    }

    #[tokio::test]
    async fn test_process_line_initialize() {
        let s = make_serve();
        let resp = process_line(s, r#"{"jsonrpc":"2.0","id":1,"method":"initialize","params":{}}"#).await.unwrap();
        let parsed: serde_json::Value = serde_json::from_str(&resp).unwrap();
        assert_eq!(parsed["result"]["serverInfo"]["name"], "clix");
    }

    #[tokio::test]
    async fn test_process_invalid_json() {
        let s = make_serve();
        let resp = process_line(s, "not json").await.unwrap();
        let parsed: serde_json::Value = serde_json::from_str(&resp).unwrap();
        assert_eq!(parsed["error"]["code"], -32700);
    }
}
