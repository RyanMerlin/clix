use crate::error::{ClixError, Result};

pub fn run_remote(addr: &str, capability_name: &str, input: &serde_json::Value) -> Result<serde_json::Value> {
    if addr.starts_with("http://") || addr.starts_with("https://") {
        run_remote_http(addr, capability_name, input)
    } else {
        let path = addr.trim_start_matches("unix://");
        run_remote_unix(path, capability_name, input)
    }
}

fn run_remote_http(addr: &str, capability_name: &str, input: &serde_json::Value) -> Result<serde_json::Value> {
    let url = format!("{}/", addr.trim_end_matches('/'));
    let payload = serde_json::json!({"jsonrpc":"2.0","id":1,"method":"tools/call","params":{"name":capability_name,"arguments":input}});
    let client = reqwest::blocking::Client::new();
    let resp: serde_json::Value = client.post(&url).json(&payload).send()
        .map_err(|e| ClixError::Backend(format!("remote HTTP: {e}")))?
        .json().map_err(|e| ClixError::Backend(format!("remote decode: {e}")))?;
    if let Some(err) = resp.get("error") { return Err(ClixError::Backend(format!("remote error: {err}"))); }
    Ok(resp["result"].clone())
}

#[cfg(unix)]
fn run_remote_unix(path: &str, capability_name: &str, input: &serde_json::Value) -> Result<serde_json::Value> {
    use std::io::{BufRead, BufReader, Write};
    use std::os::unix::net::UnixStream;
    let mut stream = UnixStream::connect(path).map_err(|e| ClixError::Backend(format!("unix socket {path}: {e}")))?;
    let payload = serde_json::json!({"jsonrpc":"2.0","id":1,"method":"tools/call","params":{"name":capability_name,"arguments":input}});
    let mut line = serde_json::to_string(&payload).unwrap();
    line.push('\n');
    stream.write_all(line.as_bytes()).map_err(|e| ClixError::Backend(format!("unix write: {e}")))?;
    let reader = BufReader::new(stream);
    let resp_line = reader.lines().next()
        .ok_or_else(|| ClixError::Backend("unix socket: no response".to_string()))?
        .map_err(|e| ClixError::Backend(format!("unix read: {e}")))?;
    let resp: serde_json::Value = serde_json::from_str(&resp_line)?;
    if let Some(err) = resp.get("error") { return Err(ClixError::Backend(format!("remote error: {err}"))); }
    Ok(resp["result"].clone())
}

#[cfg(not(unix))]
fn run_remote_unix(_path: &str, _capability_name: &str, _input: &serde_json::Value) -> Result<serde_json::Value> {
    Err(ClixError::Backend("Unix socket not supported on this platform".to_string()))
}
