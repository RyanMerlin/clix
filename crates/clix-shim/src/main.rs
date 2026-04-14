/// clix-shim: a tiny PATH-interception binary.
///
/// When `clix init --install-shims` is run, a copy of this binary is placed at
/// `$CLIX_HOME/bin/<command>` (e.g. `~/.clix/bin/git`) for each capability command, and
/// `$CLIX_HOME/bin` is prepended to the agent's PATH.
///
/// When the agent runs `git status`, this shim is invoked instead of the real binary.
/// The shim:
///   1. Figures out which command it was invoked as (argv[0]).
///   2. Connects to the gateway's unix socket (`$CLIX_GATEWAY_SOCKET`).
///   3. Sends a `shim/call` JSON-RPC request with the command name and argv.
///      The gateway resolves argv against capability argv_patterns and dispatches.
///   4. Prints stdout/stderr and exits with the same code the gateway returns.
///
/// If the gateway is unreachable, the shim exits 127 with a clear error message.
/// It does NOT fall through to the real binary — that would defeat the purpose.
///
/// Profile denial: exit 77 with a hint.
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
            eprintln!("[clix-shim] Start the gateway with `clix serve --socket {socket_path}`.");
            eprintln!("[clix-shim] Direct invocation of `{command}` is not permitted through this shim.");
            std::process::exit(127);
        }
    };

    // argv[1..] are the arguments to the command
    let args: Vec<&str> = if argv.len() > 1 { argv[1..].iter().map(String::as_str).collect() } else { vec![] };

    // Use `shim/call` — the gateway resolves argv to a capability via argv_pattern.
    let request = serde_json::json!({
        "jsonrpc": "2.0",
        "id": 1,
        "method": "shim/call",
        "params": {
            "command": command,
            "argv": args,
        }
    });

    let msg = serde_json::to_string(&request).expect("serialize request") + "\n";
    if let Err(e) = stream.write_all(msg.as_bytes()) {
        eprintln!("[clix-shim] ERROR: write to gateway: {e}");
        std::process::exit(127);
    }

    let mut response = String::new();
    if let Err(e) = stream.read_to_string(&mut response) {
        eprintln!("[clix-shim] ERROR: read from gateway: {e}");
        std::process::exit(127);
    }

    let resp: serde_json::Value = match serde_json::from_str(&response) {
        Ok(v) => v,
        Err(_) => {
            print!("{response}");
            std::process::exit(1);
        }
    };

    // Gateway-level JSON-RPC error (e.g. no matching capability)
    if let Some(error) = resp.get("error") {
        let msg = error["message"].as_str().unwrap_or("unknown error");
        if error["code"].as_i64() == Some(-32000) && msg.contains("no capability matched") {
            eprintln!("[clix] {msg}");
        } else {
            eprintln!("[clix] error: {msg}");
        }
        std::process::exit(126);
    }

    let result = &resp["result"];

    // Profile-blocked → exit 77 with a hint
    if result["_blocked"].as_bool().unwrap_or(false) {
        let profile = result["profile"].as_str().unwrap_or("current");
        eprintln!("[clix] blocked by profile '{profile}' — try `clix profile activate write`");
        std::process::exit(77);
    }

    // Print stdout
    let stdout = result["stdout"].as_str().unwrap_or("");
    if !stdout.is_empty() { print!("{stdout}"); }

    // Print stderr
    let stderr = result["stderr"].as_str().unwrap_or("");
    if !stderr.is_empty() { eprint!("{stderr}"); }

    let exit_code = result["exit_code"].as_i64().unwrap_or(if result["ok"].as_bool().unwrap_or(true) { 0 } else { 1 }) as i32;
    std::process::exit(exit_code);
}

fn connect_with_timeout(socket_path: &str) -> std::io::Result<UnixStream> {
    let stream = UnixStream::connect(socket_path)?;
    stream.set_read_timeout(Some(std::time::Duration::from_secs(CONNECT_TIMEOUT_SECS)))?;
    stream.set_write_timeout(Some(std::time::Duration::from_secs(CONNECT_TIMEOUT_SECS)))?;
    Ok(stream)
}
