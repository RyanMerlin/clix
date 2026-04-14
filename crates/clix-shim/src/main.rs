/// clix-shim: a tiny PATH-interception binary.
///
/// When `clix init --install-shims` is run, a copy of this binary is placed at
/// `$CLIX_HOME/bin/<command>` (e.g. `~/.clix/bin/gcloud`) for each capability command, and
/// `$CLIX_HOME/bin` is prepended to the agent's PATH.
///
/// When the agent runs `gcloud projects list`, this shim is invoked instead of the real binary.
/// The shim:
///   1. Figures out which command it was invoked as (argv[0]).
///   2. Connects to the gateway's unix socket (`$CLIX_GATEWAY_SOCKET`).
///   3. Sends a JSON-RPC call analogous to `clix run <command> --args ...`.
///   4. Prints stdout/stderr and exits with the same code the gateway returns.
///
/// If the gateway is unreachable, the shim exits 127 with a clear error message.
/// It does NOT fall through to the real binary — that would defeat the purpose.
use std::io::{Read, Write};
use std::os::unix::net::UnixStream;

const DEFAULT_SOCKET: &str = "/tmp/clix-gateway.sock";
const CONNECT_TIMEOUT_SECS: u64 = 5;

fn main() {
    let argv: Vec<String> = std::env::args().collect();
    let command = std::path::Path::new(&argv[0])
        .file_name()
        .and_then(|n| n.to_str())
        .unwrap_or("unknown")
        .to_string();

    let socket_path = std::env::var("CLIX_GATEWAY_SOCKET")
        .unwrap_or_else(|_| DEFAULT_SOCKET.to_string());

    let mut stream = match connect_with_timeout(&socket_path) {
        Ok(s) => s,
        Err(e) => {
            eprintln!("[clix-shim] ERROR: cannot connect to clix gateway at {socket_path}: {e}");
            eprintln!("[clix-shim] The clix gateway must be running (`clix serve --daemon`).");
            eprintln!("[clix-shim] Direct invocation of `{command}` is not permitted through this shim.");
            std::process::exit(127);
        }
    };

    // Build a lightweight JSON-RPC request (compatible with the MCP tools/call shape)
    let args: Vec<&str> = if argv.len() > 1 { argv[1..].iter().map(String::as_str).collect() } else { vec![] };
    let request = serde_json::json!({
        "jsonrpc": "2.0",
        "id": 1,
        "method": "tools/call",
        "params": {
            "name": format!("{command}.shim"),
            "arguments": {
                "_raw_argv": args,
                "_shim_command": command,
            }
        }
    });

    let msg = serde_json::to_string(&request).expect("serialize request") + "\n";
    if let Err(e) = stream.write_all(msg.as_bytes()) {
        eprintln!("[clix-shim] ERROR: write to gateway: {e}");
        std::process::exit(127);
    }

    // Read response (length-prefixed or newline-delimited)
    let mut response = String::new();
    if let Err(e) = stream.read_to_string(&mut response) {
        eprintln!("[clix-shim] ERROR: read from gateway: {e}");
        std::process::exit(127);
    }

    // Parse the response
    let resp: serde_json::Value = match serde_json::from_str(&response) {
        Ok(v) => v,
        Err(_) => {
            // If the gateway spoke something we didn't understand, print raw and exit 1
            print!("{response}");
            std::process::exit(1);
        }
    };

    if let Some(error) = resp.get("error") {
        eprintln!("[clix] policy denied: {}", error["message"].as_str().unwrap_or("unknown"));
        std::process::exit(1);
    }

    // Extract result stdout/stderr/exitCode from the MCP content array
    if let Some(content) = resp["result"]["content"].as_array() {
        for item in content {
            if let Some(text) = item["text"].as_str() {
                print!("{text}");
            }
        }
    }

    let exit_code = resp["result"]["exitCode"].as_i64().unwrap_or(0) as i32;
    std::process::exit(exit_code);
}

fn connect_with_timeout(socket_path: &str) -> std::io::Result<UnixStream> {
    // UnixStream::connect doesn't natively support timeouts, but for a local socket
    // the connect is nearly instant; we set a read timeout instead.
    let stream = UnixStream::connect(socket_path)?;
    stream.set_read_timeout(Some(std::time::Duration::from_secs(CONNECT_TIMEOUT_SECS)))?;
    stream.set_write_timeout(Some(std::time::Duration::from_secs(CONNECT_TIMEOUT_SECS)))?;
    Ok(stream)
}
