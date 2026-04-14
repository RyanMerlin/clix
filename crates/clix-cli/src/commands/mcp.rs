use anyhow::Result;
use clix_serve::{build_serve_state, dispatch};

/// One-shot JSON-RPC call: build the serve state in-process, dispatch the method,
/// print the result, and exit. No long-running server is started.
pub async fn call(method: &str, params_json: Option<&str>) -> Result<()> {
    let params: serde_json::Value = match params_json {
        Some(s) => serde_json::from_str(s)
            .map_err(|e| anyhow::anyhow!("invalid --params JSON: {e}"))?,
        None => serde_json::json!({}),
    };

    let serve = build_serve_state()?;
    let req = serde_json::json!({
        "jsonrpc": "2.0",
        "id": 1,
        "method": method,
        "params": params,
    });
    let resp = dispatch(serve, req).await;
    println!("{}", serde_json::to_string_pretty(&resp)?);
    Ok(())
}
