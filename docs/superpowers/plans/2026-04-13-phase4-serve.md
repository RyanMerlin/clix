# clix Rust Rewrite — Phase 4: Serve Layer (MCP + Extensions)

> **For agentic workers:** REQUIRED SUB-SKILL: Use superpowers:subagent-driven-development (recommended) or superpowers:executing-plans to implement this plan task-by-task. Steps use checkbox (`- [ ]`) syntax for tracking.

**Goal:** Implement the async `clix-serve` crate with tokio + axum — three transports (stdio, Unix socket, HTTP), MCP-compliant core methods, and clix extension methods. Wire it into the `clix serve` CLI command.

**Architecture:** Single `dispatch()` function handles all JSON-RPC 2.0. Three transport adapters funnel bytes to it. MCP core methods are fully spec-compliant. Clix extensions are declared in the `initialize` response.

**Prerequisites:** Phases 1, 2, and 3 complete.

**Tech Stack:** tokio (full), axum 0.7, serde_json, clix-core

**Spec:** `docs/superpowers/specs/2026-04-13-rust-rewrite-design.md`

---

## File Map

```
crates/clix-serve/
  Cargo.toml
  src/
    lib.rs
    dispatch.rs              # JsonRpcRequest, JsonRpcResponse, dispatch()
    methods/
      mod.rs
      mcp.rs                 # initialize, tools/list, tools/call, resources/list
      extensions.rs          # workflows/*, onboard/*, packs/*, status/get
    transport/
      mod.rs
      stdio.rs               # serve_stdio()
      socket.rs              # serve_socket() — Unix only
      http.rs                # serve_http() via axum
```

---

### Task 1: clix-serve crate setup and dispatch core

**Files:**
- Modify: `crates/clix-serve/Cargo.toml`
- Modify: `crates/clix-serve/src/lib.rs`
- Create: `crates/clix-serve/src/dispatch.rs`

- [ ] **Step 1: Update Cargo.toml**

`crates/clix-serve/Cargo.toml`:
```toml
[package]
name    = "clix-serve"
version = "0.2.0"
edition = "2021"

[dependencies]
clix-core  = { path = "../clix-core" }
tokio      = { workspace = true }
axum       = { workspace = true }
serde      = { workspace = true }
serde_json = { workspace = true }
anyhow     = { workspace = true }
tracing    = { workspace = true }
```

- [ ] **Step 2: Write failing test for dispatch**

`crates/clix-serve/src/dispatch.rs`:
```rust
#[cfg(test)]
mod tests {
    use super::*;
    use clix_core::registry::{CapabilityRegistry, WorkflowRegistry};
    use clix_core::policy::PolicyBundle;
    use clix_core::receipts::ReceiptStore;
    use clix_core::state::ClixState;
    use std::sync::Arc;
    use std::path::PathBuf;

    fn test_state() -> Arc<ServeState> {
        let home = PathBuf::from(std::env::temp_dir().join("clix-test-serve"));
        std::fs::create_dir_all(&home).unwrap();
        let db = home.join("receipts.db");
        Arc::new(ServeState {
            cap_registry: CapabilityRegistry::from_vec(vec![]),
            wf_registry: WorkflowRegistry::from_vec(vec![]),
            policy: PolicyBundle::default(),
            store: ReceiptStore::open(&db).unwrap(),
            state: ClixState::from_home(home),
        })
    }

    #[tokio::test]
    async fn test_initialize() {
        let serve_state = test_state();
        let req = serde_json::json!({
            "jsonrpc": "2.0",
            "id": 1,
            "method": "initialize",
            "params": {}
        });
        let resp = dispatch(serve_state, req).await;
        assert_eq!(resp["jsonrpc"], "2.0");
        assert!(resp["result"]["serverInfo"]["name"].as_str() == Some("clix"));
    }

    #[tokio::test]
    async fn test_tools_list_empty() {
        let serve_state = test_state();
        let req = serde_json::json!({
            "jsonrpc": "2.0",
            "id": 2,
            "method": "tools/list",
            "params": {}
        });
        let resp = dispatch(serve_state, req).await;
        assert!(resp["result"]["tools"].as_array().is_some());
    }

    #[tokio::test]
    async fn test_unknown_method() {
        let serve_state = test_state();
        let req = serde_json::json!({
            "jsonrpc": "2.0",
            "id": 3,
            "method": "does/not/exist",
            "params": {}
        });
        let resp = dispatch(serve_state, req).await;
        assert_eq!(resp["error"]["code"], -32601);
    }
}
```

- [ ] **Step 3: Run — expect compile failure**

```bash
cargo test -p clix-serve 2>&1 | head -5
```

- [ ] **Step 4: Implement dispatch.rs**

`crates/clix-serve/src/dispatch.rs`:
```rust
use std::sync::Arc;
use clix_core::policy::PolicyBundle;
use clix_core::receipts::ReceiptStore;
use clix_core::registry::{CapabilityRegistry, WorkflowRegistry};
use clix_core::sandbox::sandbox_enforced;
use clix_core::state::ClixState;

/// Shared server state — built once at startup, Arc-shared across connections.
pub struct ServeState {
    pub cap_registry: CapabilityRegistry,
    pub wf_registry:  WorkflowRegistry,
    pub policy:       PolicyBundle,
    pub store:        ReceiptStore,
    pub state:        ClixState,
}

/// Dispatch a single JSON-RPC 2.0 request value, return a response value.
pub async fn dispatch(serve: Arc<ServeState>, req: serde_json::Value) -> serde_json::Value {
    let id  = req.get("id").cloned().unwrap_or(serde_json::Value::Null);
    let method = req["method"].as_str().unwrap_or("").to_string();
    let params = req.get("params")
        .and_then(|p| p.as_object())
        .cloned()
        .map(serde_json::Value::Object)
        .unwrap_or(serde_json::json!({}));

    let result = match method.as_str() {
        "initialize"       => crate::methods::mcp::initialize(&serve),
        "tools/list"       => crate::methods::mcp::tools_list(&serve),
        "tools/call"       => crate::methods::mcp::tools_call(&serve, &params).await,
        "resources/list"   => crate::methods::mcp::resources_list(&serve),
        "workflows/list"   => crate::methods::extensions::workflows_list(&serve),
        "workflows/run"    => crate::methods::extensions::workflows_run(&serve, &params).await,
        "onboard/probe"    => crate::methods::extensions::onboard_probe(&params),
        "packs/list"       => crate::methods::extensions::packs_list(&serve),
        "status/get"       => crate::methods::extensions::status_get(&serve),
        _ => {
            return rpc_error(id, -32601, format!("method not found: {method}"));
        }
    };

    match result {
        Ok(value)  => rpc_ok(id, value),
        Err(e)     => rpc_error(id, -32000, e.to_string()),
    }
}

pub fn rpc_ok(id: serde_json::Value, result: serde_json::Value) -> serde_json::Value {
    serde_json::json!({ "jsonrpc": "2.0", "id": id, "result": result })
}

pub fn rpc_error(id: serde_json::Value, code: i32, message: String) -> serde_json::Value {
    serde_json::json!({ "jsonrpc": "2.0", "id": id, "error": { "code": code, "message": message } })
}

#[cfg(test)]
mod tests {
    use super::*;
    use clix_core::registry::{CapabilityRegistry, WorkflowRegistry};
    use clix_core::policy::PolicyBundle;
    use clix_core::receipts::ReceiptStore;
    use clix_core::state::ClixState;
    use std::sync::Arc;
    use std::path::PathBuf;

    fn test_state() -> Arc<ServeState> {
        let home = std::env::temp_dir().join("clix-test-serve");
        std::fs::create_dir_all(&home).unwrap();
        let db = home.join("receipts.db");
        Arc::new(ServeState {
            cap_registry: CapabilityRegistry::from_vec(vec![]),
            wf_registry:  WorkflowRegistry::from_vec(vec![]),
            policy:       PolicyBundle::default(),
            store:        ReceiptStore::open(&db).unwrap(),
            state:        ClixState::from_home(home),
        })
    }

    #[tokio::test]
    async fn test_initialize() {
        let s = test_state();
        let req = serde_json::json!({"jsonrpc":"2.0","id":1,"method":"initialize","params":{}});
        let resp = dispatch(s, req).await;
        assert_eq!(resp["result"]["serverInfo"]["name"], "clix");
    }

    #[tokio::test]
    async fn test_tools_list_empty() {
        let s = test_state();
        let req = serde_json::json!({"jsonrpc":"2.0","id":2,"method":"tools/list","params":{}});
        let resp = dispatch(s, req).await;
        assert!(resp["result"]["tools"].as_array().is_some());
    }

    #[tokio::test]
    async fn test_unknown_method() {
        let s = test_state();
        let req = serde_json::json!({"jsonrpc":"2.0","id":3,"method":"does/not/exist","params":{}});
        let resp = dispatch(s, req).await;
        assert_eq!(resp["error"]["code"], -32601);
    }
}
```

- [ ] **Step 5: Create lib.rs**

`crates/clix-serve/src/lib.rs`:
```rust
pub mod dispatch;
pub mod methods;
pub mod transport;

pub use dispatch::{dispatch, ServeState};
```

- [ ] **Step 6: Create stub method files**

```bash
mkdir -p crates/clix-serve/src/methods
mkdir -p crates/clix-serve/src/transport
touch crates/clix-serve/src/methods/mod.rs
touch crates/clix-serve/src/methods/mcp.rs
touch crates/clix-serve/src/methods/extensions.rs
touch crates/clix-serve/src/transport/mod.rs
touch crates/clix-serve/src/transport/stdio.rs
touch crates/clix-serve/src/transport/socket.rs
touch crates/clix-serve/src/transport/http.rs
```

`crates/clix-serve/src/methods/mod.rs`:
```rust
pub mod extensions;
pub mod mcp;
```

`crates/clix-serve/src/transport/mod.rs`:
```rust
pub mod http;
pub mod stdio;
#[cfg(unix)]
pub mod socket;
```

- [ ] **Step 7: Run tests**

```bash
cargo test -p clix-serve dispatch
```
Expected: 3 tests pass

- [ ] **Step 8: Commit**

```bash
git add crates/clix-serve/
git commit -m "feat(serve): add dispatch core and ServeState"
```

---

### Task 2: MCP methods

**Files:**
- Modify: `crates/clix-serve/src/methods/mcp.rs`

- [ ] **Step 1: Implement mcp.rs**

`crates/clix-serve/src/methods/mcp.rs`:
```rust
use std::sync::Arc;
use clix_core::sandbox::sandbox_enforced;
use clix_core::policy::evaluate::ExecutionContext;
use clix_core::execution::run_capability;
use crate::dispatch::ServeState;

type Result = std::result::Result<serde_json::Value, String>;

pub fn initialize(serve: &Arc<ServeState>) -> Result {
    Ok(serde_json::json!({
        "serverInfo": { "name": "clix", "version": env!("CARGO_PKG_VERSION") },
        "protocolVersion": "2024-11-05",
        "capabilities": {
            "tools": { "listChanged": false },
            "resources": { "listChanged": false },
            "extensions": {
                "clix": { "workflows": true, "onboard": true }
            }
        },
        "sandboxEnforced": sandbox_enforced()
    }))
}

pub fn tools_list(serve: &Arc<ServeState>) -> Result {
    let tools: Vec<serde_json::Value> = serve
        .cap_registry
        .all()
        .into_iter()
        .map(|cap| serde_json::json!({
            "name": cap.name,
            "description": cap.description.as_deref().unwrap_or(""),
            "inputSchema": cap.input_schema
        }))
        .collect();
    Ok(serde_json::json!({ "tools": tools }))
}

pub async fn tools_call(serve: &Arc<ServeState>, params: &serde_json::Value) -> Result {
    let name = params["name"].as_str()
        .ok_or("tools/call: missing 'name'")?;
    let arguments = params.get("arguments")
        .cloned()
        .unwrap_or(serde_json::json!({}));

    let ctx = ExecutionContext {
        env:     serve.state.config.default_env.clone(),
        cwd:     serve.state.config.workspace_root.clone()
                     .unwrap_or_else(|| std::path::PathBuf::from(".")),
        user:    "agent".to_string(),
        profile: serve.state.config.active_profiles.first()
                     .cloned().unwrap_or_else(|| "default".to_string()),
        approver: None,
    };

    // run_capability is sync — spawn_blocking wraps it for async contexts
    let serve_clone = Arc::clone(serve);
    let name = name.to_string();
    let outcome = tokio::task::spawn_blocking(move || {
        run_capability(
            &serve_clone.cap_registry,
            &serve_clone.policy,
            serve_clone.state.config.infisical.as_ref(),
            &serve_clone.store,
            &name,
            arguments,
            ctx,
        )
    })
    .await
    .map_err(|e| format!("task join: {e}"))?
    .map_err(|e| e.to_string())?;

    // MCP tools/call response format: content array
    let content = if let Some(result) = &outcome.result {
        if let Some(stdout) = result["stdout"].as_str() {
            vec![serde_json::json!({ "type": "text", "text": stdout })]
        } else {
            vec![serde_json::json!({ "type": "text", "text": serde_json::to_string(result).unwrap_or_default() })]
        }
    } else {
        vec![serde_json::json!({ "type": "text", "text": outcome.reason.as_deref().unwrap_or("") })]
    };

    Ok(serde_json::json!({
        "content": content,
        "isError": !outcome.ok,
        "_clix": { "receiptId": outcome.receipt_id, "approvalRequired": outcome.approval_required }
    }))
}

pub fn resources_list(serve: &Arc<ServeState>) -> Result {
    // Installed packs as MCP resources
    let mut resources = vec![];
    if serve.state.packs_dir.exists() {
        if let Ok(entries) = std::fs::read_dir(&serve.state.packs_dir) {
            for entry in entries.flatten() {
                if entry.file_type().map(|t| t.is_dir()).unwrap_or(false) {
                    let name = entry.file_name().to_string_lossy().to_string();
                    resources.push(serde_json::json!({
                        "uri":  format!("clix://packs/{name}"),
                        "name": name,
                        "mimeType": "application/json"
                    }));
                }
            }
        }
    }
    Ok(serde_json::json!({ "resources": resources }))
}
```

- [ ] **Step 2: Run tests**

```bash
cargo test -p clix-serve
```
Expected: 3 dispatch tests still pass

- [ ] **Step 3: Commit**

```bash
git add crates/clix-serve/src/methods/mcp.rs
git commit -m "feat(serve): implement MCP core methods (initialize, tools/list, tools/call, resources/list)"
```

---

### Task 3: clix extension methods

**Files:**
- Modify: `crates/clix-serve/src/methods/extensions.rs`

- [ ] **Step 1: Implement extensions.rs**

`crates/clix-serve/src/methods/extensions.rs`:
```rust
use std::sync::Arc;
use clix_core::execution::run_workflow;
use clix_core::packs::onboard_cli;
use clix_core::policy::evaluate::ExecutionContext;
use clix_core::sandbox::sandbox_enforced;
use crate::dispatch::ServeState;

type Result = std::result::Result<serde_json::Value, String>;

pub fn workflows_list(serve: &Arc<ServeState>) -> Result {
    let workflows: Vec<serde_json::Value> = serve
        .wf_registry
        .all()
        .into_iter()
        .map(|wf| serde_json::json!({
            "name": wf.name,
            "description": wf.description.as_deref().unwrap_or(""),
            "stepCount": wf.steps.len()
        }))
        .collect();
    Ok(serde_json::json!({ "workflows": workflows }))
}

pub async fn workflows_run(serve: &Arc<ServeState>, params: &serde_json::Value) -> Result {
    let name = params["name"].as_str()
        .ok_or("workflows/run: missing 'name'")?;
    let arguments = params.get("arguments")
        .cloned()
        .unwrap_or(serde_json::json!({}));

    let ctx = ExecutionContext {
        env:     serve.state.config.default_env.clone(),
        cwd:     serve.state.config.workspace_root.clone()
                     .unwrap_or_else(|| std::path::PathBuf::from(".")),
        user:    "agent".to_string(),
        profile: serve.state.config.active_profiles.first()
                     .cloned().unwrap_or_else(|| "default".to_string()),
        approver: None,
    };

    let serve_clone = Arc::clone(serve);
    let name = name.to_string();
    let outcomes = tokio::task::spawn_blocking(move || {
        run_workflow(
            &serve_clone.cap_registry,
            &serve_clone.wf_registry,
            &serve_clone.policy,
            serve_clone.state.config.infisical.as_ref(),
            &serve_clone.store,
            &name,
            arguments,
            ctx,
        )
    })
    .await
    .map_err(|e| format!("task join: {e}"))?
    .map_err(|e| e.to_string())?;

    Ok(serde_json::json!({ "outcomes": outcomes }))
}

pub fn onboard_probe(params: &serde_json::Value) -> Result {
    let name    = params["name"].as_str().ok_or("onboard/probe: missing 'name'")?;
    let command = params["command"].as_str().ok_or("onboard/probe: missing 'command'")?;
    let tmp = std::env::temp_dir().join(format!("clix-onboard-{name}"));
    std::fs::create_dir_all(&tmp).map_err(|e| e.to_string())?;
    let report = onboard_cli(name, command, &tmp).map_err(|e| e.to_string())?;
    Ok(serde_json::to_value(report).map_err(|e| e.to_string())?)
}

pub fn packs_list(serve: &Arc<ServeState>) -> Result {
    let mut packs = vec![];
    if serve.state.packs_dir.exists() {
        if let Ok(entries) = std::fs::read_dir(&serve.state.packs_dir) {
            for entry in entries.flatten() {
                if !entry.file_type().map(|t| t.is_dir()).unwrap_or(false) { continue; }
                let pack_file = entry.path().join("pack.yaml");
                if pack_file.exists() {
                    if let Ok(content) = std::fs::read_to_string(&pack_file) {
                        if let Ok(p) = serde_yaml::from_str::<clix_core::manifest::pack::PackManifest>(&content) {
                            packs.push(serde_json::json!({
                                "name": p.name,
                                "version": p.version,
                                "description": p.description
                            }));
                        }
                    }
                }
            }
        }
    }
    packs.sort_by(|a, b| a["name"].as_str().cmp(&b["name"].as_str()));
    Ok(serde_json::json!({ "packs": packs }))
}

pub fn status_get(serve: &Arc<ServeState>) -> Result {
    Ok(serde_json::json!({
        "home":            serve.state.home,
        "activeProfiles":  serve.state.config.active_profiles,
        "defaultEnv":      serve.state.config.default_env,
        "approvalMode":    format!("{:?}", serve.state.config.approval_mode),
        "sandboxEnforced": sandbox_enforced(),
        "capabilityCount": serve.cap_registry.all().len(),
        "workflowCount":   serve.wf_registry.all().len(),
    }))
}
```

- [ ] **Step 2: Run tests**

```bash
cargo test -p clix-serve
```
Expected: all pass

- [ ] **Step 3: Commit**

```bash
git add crates/clix-serve/src/methods/extensions.rs
git commit -m "feat(serve): implement clix extension methods (workflows, onboard, packs, status)"
```

---

### Task 4: Transport — stdio

**Files:**
- Modify: `crates/clix-serve/src/transport/stdio.rs`

- [ ] **Step 1: Write failing test**

`crates/clix-serve/src/transport/stdio.rs`:
```rust
#[cfg(test)]
mod tests {
    use super::*;
    use clix_core::registry::{CapabilityRegistry, WorkflowRegistry};
    use clix_core::policy::PolicyBundle;
    use clix_core::receipts::ReceiptStore;
    use clix_core::state::ClixState;
    use std::sync::Arc;

    #[tokio::test]
    async fn test_process_line_initialize() {
        let home = std::env::temp_dir().join("clix-stdio-test");
        std::fs::create_dir_all(&home).unwrap();
        let serve = Arc::new(crate::dispatch::ServeState {
            cap_registry: CapabilityRegistry::from_vec(vec![]),
            wf_registry:  WorkflowRegistry::from_vec(vec![]),
            policy:       PolicyBundle::default(),
            store:        ReceiptStore::open(&home.join("receipts.db")).unwrap(),
            state:        ClixState::from_home(home),
        });
        let line = r#"{"jsonrpc":"2.0","id":1,"method":"initialize","params":{}}"#;
        let resp = process_line(Arc::clone(&serve), line).await.unwrap();
        let parsed: serde_json::Value = serde_json::from_str(&resp).unwrap();
        assert_eq!(parsed["result"]["serverInfo"]["name"], "clix");
    }
}
```

- [ ] **Step 2: Run — expect compile failure**

```bash
cargo test -p clix-serve transport 2>&1 | head -5
```

- [ ] **Step 3: Implement stdio.rs**

`crates/clix-serve/src/transport/stdio.rs`:
```rust
use std::sync::Arc;
use tokio::io::{AsyncBufReadExt, AsyncWriteExt, BufReader};
use crate::dispatch::{dispatch, ServeState};

/// Handle a single JSON-RPC line, return the serialized response.
pub async fn process_line(serve: Arc<ServeState>, line: &str) -> Option<String> {
    if line.trim().is_empty() {
        return None;
    }
    let req: serde_json::Value = match serde_json::from_str(line) {
        Ok(v) => v,
        Err(e) => serde_json::json!({
            "jsonrpc": "2.0", "id": null,
            "error": { "code": -32700, "message": format!("parse error: {e}") }
        }),
    };
    let resp = dispatch(serve, req).await;
    Some(serde_json::to_string(&resp).unwrap_or_default())
}

/// Serve JSON-RPC over stdin/stdout, one request per line.
pub async fn serve_stdio(serve: Arc<ServeState>) -> anyhow::Result<()> {
    let stdin  = tokio::io::stdin();
    let stdout = tokio::io::stdout();
    let mut reader = BufReader::new(stdin);
    let mut writer = tokio::io::BufWriter::new(stdout);
    let mut line   = String::new();

    loop {
        line.clear();
        let n = reader.read_line(&mut line).await?;
        if n == 0 { break; } // EOF
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
    use clix_core::registry::{CapabilityRegistry, WorkflowRegistry};
    use clix_core::policy::PolicyBundle;
    use clix_core::receipts::ReceiptStore;
    use clix_core::state::ClixState;

    #[tokio::test]
    async fn test_process_line_initialize() {
        let home = std::env::temp_dir().join("clix-stdio-test");
        std::fs::create_dir_all(&home).unwrap();
        let serve = Arc::new(ServeState {
            cap_registry: CapabilityRegistry::from_vec(vec![]),
            wf_registry:  WorkflowRegistry::from_vec(vec![]),
            policy:       PolicyBundle::default(),
            store:        ReceiptStore::open(&home.join("receipts.db")).unwrap(),
            state:        ClixState::from_home(home),
        });
        let line = r#"{"jsonrpc":"2.0","id":1,"method":"initialize","params":{}}"#;
        let resp = process_line(Arc::clone(&serve), line).await.unwrap();
        let parsed: serde_json::Value = serde_json::from_str(&resp).unwrap();
        assert_eq!(parsed["result"]["serverInfo"]["name"], "clix");
    }

    #[tokio::test]
    async fn test_process_invalid_json() {
        let home = std::env::temp_dir().join("clix-stdio-test2");
        std::fs::create_dir_all(&home).unwrap();
        let serve = Arc::new(ServeState {
            cap_registry: CapabilityRegistry::from_vec(vec![]),
            wf_registry:  WorkflowRegistry::from_vec(vec![]),
            policy:       PolicyBundle::default(),
            store:        ReceiptStore::open(&home.join("receipts.db")).unwrap(),
            state:        ClixState::from_home(home),
        });
        let resp = process_line(Arc::clone(&serve), "not json at all").await.unwrap();
        let parsed: serde_json::Value = serde_json::from_str(&resp).unwrap();
        assert_eq!(parsed["error"]["code"], -32700);
    }
}
```

- [ ] **Step 4: Run tests**

```bash
cargo test -p clix-serve transport
```
Expected: 2 tests pass

- [ ] **Step 5: Commit**

```bash
git add crates/clix-serve/src/transport/stdio.rs
git commit -m "feat(serve): add stdio transport with process_line and serve_stdio"
```

---

### Task 5: Transport — Unix socket and HTTP

**Files:**
- Modify: `crates/clix-serve/src/transport/socket.rs`
- Modify: `crates/clix-serve/src/transport/http.rs`

- [ ] **Step 1: Implement socket.rs (Unix only)**

`crates/clix-serve/src/transport/socket.rs`:
```rust
use std::sync::Arc;
use tokio::io::{AsyncBufReadExt, AsyncWriteExt, BufReader};
use tokio::net::UnixListener;
use crate::dispatch::ServeState;
use crate::transport::stdio::process_line;

pub async fn serve_socket(serve: Arc<ServeState>, path: &str) -> anyhow::Result<()> {
    // Remove stale socket
    let _ = std::fs::remove_file(path);
    let listener = UnixListener::bind(path)?;
    eprintln!("clix daemon listening on unix:{path}");

    loop {
        let (stream, _) = listener.accept().await?;
        let serve = Arc::clone(&serve);
        tokio::spawn(async move {
            let (reader, mut writer) = stream.into_split();
            let mut reader = BufReader::new(reader);
            let mut line = String::new();
            loop {
                line.clear();
                match reader.read_line(&mut line).await {
                    Ok(0) | Err(_) => break,
                    Ok(_) => {
                        if let Some(resp) = process_line(Arc::clone(&serve), &line).await {
                            let _ = writer.write_all(resp.as_bytes()).await;
                            let _ = writer.write_all(b"\n").await;
                            let _ = writer.flush().await;
                        }
                    }
                }
            }
        });
    }
}
```

- [ ] **Step 2: Implement http.rs**

`crates/clix-serve/src/transport/http.rs`:
```rust
use std::sync::Arc;
use axum::{extract::State, routing::post, Json, Router};
use crate::dispatch::{dispatch, ServeState};

pub async fn serve_http(serve: Arc<ServeState>, addr: &str) -> anyhow::Result<()> {
    let app = Router::new()
        .route("/", post(handle_rpc))
        .with_state(serve);

    eprintln!("clix daemon listening on http://{addr}");
    let listener = tokio::net::TcpListener::bind(addr).await?;
    axum::serve(listener, app).await?;
    Ok(())
}

async fn handle_rpc(
    State(serve): State<Arc<ServeState>>,
    Json(req): Json<serde_json::Value>,
) -> Json<serde_json::Value> {
    Json(dispatch(serve, req).await)
}
```

- [ ] **Step 3: Compile check**

```bash
cargo check -p clix-serve
```
Expected: no errors

- [ ] **Step 4: Commit**

```bash
git add crates/clix-serve/src/transport/
git commit -m "feat(serve): add Unix socket and HTTP axum transports"
```

---

### Task 6: Wire serve into CLI + build_serve_state helper

**Files:**
- Modify: `crates/clix-serve/src/lib.rs`
- Modify: `crates/clix-cli/src/commands/serve.rs`
- Modify: `crates/clix-cli/Cargo.toml`

- [ ] **Step 1: Add build_serve_state to clix-serve/lib.rs**

`crates/clix-serve/src/lib.rs`:
```rust
pub mod dispatch;
pub mod methods;
pub mod transport;

pub use dispatch::{dispatch, ServeState};

use std::sync::Arc;
use anyhow::Result;
use clix_core::loader::{build_registry, build_workflow_registry, load_policy};
use clix_core::receipts::ReceiptStore;
use clix_core::state::{ClixState, home_dir};

/// Load state from disk and build a ServeState ready for serving.
pub fn build_serve_state() -> Result<Arc<ServeState>> {
    let state = ClixState::load(home_dir())?;
    state.ensure_dirs()?;
    let cap_registry = build_registry(&state)?;
    let wf_registry  = build_workflow_registry(&state)?;
    let policy       = load_policy(&state)?;
    let store        = ReceiptStore::open(&state.receipts_db)?;
    Ok(Arc::new(ServeState { cap_registry, wf_registry, policy, store, state }))
}
```

- [ ] **Step 2: Update clix-cli serve command**

`crates/clix-cli/src/commands/serve.rs`:
```rust
use anyhow::Result;
use clix_serve::build_serve_state;

pub async fn run(socket: Option<String>, http: Option<String>) -> Result<()> {
    let serve = build_serve_state()?;
    match (socket, http) {
        (Some(path), _) => {
            #[cfg(unix)]
            {
                clix_serve::transport::socket::serve_socket(serve, &path).await?;
            }
            #[cfg(not(unix))]
            anyhow::bail!("Unix socket transport not supported on Windows");
        }
        (_, Some(addr)) => {
            clix_serve::transport::http::serve_http(serve, &addr).await?;
        }
        _ => {
            clix_serve::transport::stdio::serve_stdio(serve).await?;
        }
    }
    Ok(())
}
```

- [ ] **Step 3: Build and verify**

```bash
cargo build -p clix-cli
```
Expected: compiles

```bash
# Test stdio mode: pipe a single request
echo '{"jsonrpc":"2.0","id":1,"method":"initialize","params":{}}' | cargo run -p clix-cli -- serve
```
Expected: JSON response with `serverInfo.name = "clix"`

```bash
# Test HTTP mode (background, then curl)
cargo run -p clix-cli -- serve --http 127.0.0.1:18080 &
sleep 1
curl -s -X POST http://127.0.0.1:18080/ \
  -H 'Content-Type: application/json' \
  -d '{"jsonrpc":"2.0","id":1,"method":"tools/list","params":{}}' | jq .
kill %1
```
Expected: JSON with `result.tools` array

- [ ] **Step 4: Commit**

```bash
git add crates/clix-serve/src/lib.rs crates/clix-cli/src/commands/serve.rs
git commit -m "feat(serve): wire serve layer into CLI, add build_serve_state"
```

---

### Task 7: Phase 4 wrap-up

- [ ] **Step 1: Run all tests**

```bash
cargo test
```
Expected: all pass

- [ ] **Step 2: Run clippy**

```bash
cargo clippy -- -D warnings
```
Fix any warnings.

- [ ] **Step 3: Full smoke test**

```bash
cargo build --release

# Init and run a capability
./target/release/clix init
./target/release/clix run sys.date
./target/release/clix run sys.date --json

# Pack management
./target/release/clix pack list
./target/release/clix pack validate packs/base
./target/release/clix pack discover packs/kubectl-observe --json

# MCP serve via stdio
echo '{"jsonrpc":"2.0","id":1,"method":"initialize","params":{}}' | ./target/release/clix serve
echo '{"jsonrpc":"2.0","id":2,"method":"tools/list","params":{}}' | ./target/release/clix serve
echo '{"jsonrpc":"2.0","id":3,"method":"tools/call","params":{"name":"system.date","arguments":{}}}' | ./target/release/clix serve

# Receipts
./target/release/clix receipts list
./target/release/clix receipts list --json
```
Expected: all commands run without panicking, JSON output is valid.

- [ ] **Step 4: Update GitHub Actions workflows**

Modify `.github/workflows/ci.yml` to use Rust instead of Go:

```yaml
name: CI
on: [push, pull_request]
jobs:
  test:
    runs-on: ubuntu-latest
    steps:
      - uses: actions/checkout@v4
      - uses: dtolnay/rust-toolchain@stable
      - uses: Swatinem/rust-cache@v2
      - run: cargo test --all
      - run: cargo clippy -- -D warnings
      - run: cargo build --release
```

Modify `.github/workflows/release.yml` to cross-compile with `cross`:

```yaml
name: Release
on:
  push:
    tags: ['v*']
jobs:
  build:
    strategy:
      matrix:
        include:
          - target: x86_64-unknown-linux-musl
            os: ubuntu-latest
          - target: aarch64-unknown-linux-musl
            os: ubuntu-latest
          - target: x86_64-apple-darwin
            os: macos-latest
          - target: aarch64-apple-darwin
            os: macos-latest
          - target: x86_64-pc-windows-msvc
            os: windows-latest
    runs-on: ${{ matrix.os }}
    steps:
      - uses: actions/checkout@v4
      - uses: dtolnay/rust-toolchain@stable
        with:
          targets: ${{ matrix.target }}
      - uses: Swatinem/rust-cache@v2
      - run: cargo build --release --target ${{ matrix.target }}
      - name: Upload binary
        uses: actions/upload-artifact@v4
        with:
          name: clix-${{ matrix.target }}
          path: target/${{ matrix.target }}/release/clix*
```

- [ ] **Step 5: Final commit**

```bash
git add -A
git commit -m "feat: Phase 4 complete — MCP+extensions serve layer, CI/release workflows updated"
```

---

## Phase 4 Complete — Full Rewrite Done

All four phases complete. clix is now a fully native Rust project:

| Component | Status |
|---|---|
| Workspace (4 crates) | ✅ |
| clix-core: types, policy, schema, template | ✅ |
| clix-core: execution pipeline, backends, secrets | ✅ |
| clix-core: SQLite receipts, Landlock sandbox | ✅ |
| clix-core: pack management (all 8 commands) | ✅ |
| Built-in YAML packs (base, kubectl, gcloud, gh) | ✅ |
| clix-cli: all subcommands, --json everywhere | ✅ |
| clix-serve: MCP core + clix extensions | ✅ |
| Three transports: stdio, Unix socket, HTTP | ✅ |
| CI + cross-platform release workflows | ✅ |
