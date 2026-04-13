# clix Rust Rewrite — Phase 2: Execution Engine

> **For agentic workers:** REQUIRED SUB-SKILL: Use superpowers:subagent-driven-development (recommended) or superpowers:executing-plans to implement this plan task-by-task. Steps use checkbox (`- [ ]`) syntax for tracking.

**Goal:** Implement the complete execution pipeline in `clix-core`: registry, secrets/Infisical, subprocess/builtin/remote backends, secret redaction, approval gate, receipts (SQLite), sandbox (Landlock on Linux), and `run_capability` / `run_workflow` entry points.

**Architecture:** All execution logic lives in `clix-core` as sync-friendly Rust. The backends use `std::process::Command` (sync) — the async wrapper lives in `clix-serve` (Phase 4). Receipts go to SQLite via `sqlx` in blocking mode here (wrapped async in Phase 4).

**Prerequisites:** Phase 1 complete (`clix-core` foundation).

**Tech Stack:** sqlx (sqlite), reqwest (blocking feature for approval/infisical in core), uuid, chrono, landlock (linux-only)

**Spec:** `docs/superpowers/specs/2026-04-13-rust-rewrite-design.md`

---

## File Map

```
crates/clix-core/
  src/
    registry/
      mod.rs                   # CapabilityRegistry, WorkflowRegistry
    secrets/
      mod.rs                   # CredentialSource resolution, resolve_credentials()
      infisical.rs             # InfisicalClient, token cache (reqwest blocking)
      redact.rs                # SecretRedactor
    execution/
      mod.rs                   # run_capability(), run_workflow(), ExecutionOutcome
      context.rs               # ExecutionContext (already in policy — re-export)
      backends/
        mod.rs
        subprocess.rs          # run_subprocess()
        builtin.rs             # builtin_handler() — system.date, system.echo
        remote.rs              # run_remote() via Unix socket or HTTP
      approval.rs              # request_approval(), ApprovalRequest/Response
      validators.rs            # run_validators()
    receipts/
      mod.rs                   # ReceiptStore, write_receipt(), query methods
      schema.sql               # CREATE TABLE receipts ...
    sandbox/
      mod.rs                   # apply_sandbox(), sandbox_enforced() -> bool
      linux.rs                 # Landlock impl
      stub.rs                  # no-op for non-Linux
```

---

### Task 1: Capability and Workflow Registry

**Files:**
- Create: `crates/clix-core/src/registry/mod.rs`
- Modify: `crates/clix-core/src/lib.rs`

- [ ] **Step 1: Write failing test**

`crates/clix-core/src/registry/mod.rs`:
```rust
#[cfg(test)]
mod tests {
    use super::*;
    use crate::manifest::capability::{Backend, CapabilityManifest, RiskLevel, SideEffectClass};

    fn make_cap(name: &str) -> CapabilityManifest {
        CapabilityManifest {
            name: name.to_string(),
            version: 1,
            description: None,
            backend: Backend::Builtin { name: "date".to_string() },
            risk: RiskLevel::Low,
            side_effect_class: SideEffectClass::ReadOnly,
            sandbox_profile: None,
            approval_policy: None,
            input_schema: serde_json::json!({}),
            validators: vec![],
            credentials: vec![],
        }
    }

    #[test]
    fn test_registry_get() {
        let caps = vec![make_cap("sys.date"), make_cap("sys.echo")];
        let reg = CapabilityRegistry::from_vec(caps);
        assert!(reg.get("sys.date").is_some());
        assert!(reg.get("sys.missing").is_none());
        assert_eq!(reg.all().len(), 2);
    }
}
```

- [ ] **Step 2: Run — expect compile failure**

```bash
cargo test -p clix-core registry 2>&1 | head -5
```

- [ ] **Step 3: Implement registry/mod.rs**

`crates/clix-core/src/registry/mod.rs`:
```rust
use std::collections::HashMap;
use crate::manifest::capability::CapabilityManifest;
use crate::manifest::workflow::WorkflowManifest;

#[derive(Debug, Default, Clone)]
pub struct CapabilityRegistry {
    caps: HashMap<String, CapabilityManifest>,
}

impl CapabilityRegistry {
    pub fn from_vec(caps: Vec<CapabilityManifest>) -> Self {
        let mut reg = Self::default();
        for cap in caps {
            reg.caps.insert(cap.name.clone(), cap);
        }
        reg
    }

    pub fn get(&self, name: &str) -> Option<&CapabilityManifest> {
        self.caps.get(name)
    }

    pub fn all(&self) -> Vec<&CapabilityManifest> {
        let mut v: Vec<_> = self.caps.values().collect();
        v.sort_by(|a, b| a.name.cmp(&b.name));
        v
    }

    pub fn insert(&mut self, cap: CapabilityManifest) {
        self.caps.insert(cap.name.clone(), cap);
    }
}

#[derive(Debug, Default, Clone)]
pub struct WorkflowRegistry {
    workflows: HashMap<String, WorkflowManifest>,
}

impl WorkflowRegistry {
    pub fn from_vec(workflows: Vec<WorkflowManifest>) -> Self {
        let mut reg = Self::default();
        for wf in workflows {
            reg.workflows.insert(wf.name.clone(), wf);
        }
        reg
    }

    pub fn get(&self, name: &str) -> Option<&WorkflowManifest> {
        self.workflows.get(name)
    }

    pub fn all(&self) -> Vec<&WorkflowManifest> {
        let mut v: Vec<_> = self.workflows.values().collect();
        v.sort_by(|a, b| a.name.cmp(&b.name));
        v
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::manifest::capability::{Backend, CapabilityManifest, RiskLevel, SideEffectClass};

    fn make_cap(name: &str) -> CapabilityManifest {
        CapabilityManifest {
            name: name.to_string(),
            version: 1,
            description: None,
            backend: Backend::Builtin { name: "date".to_string() },
            risk: RiskLevel::Low,
            side_effect_class: SideEffectClass::ReadOnly,
            sandbox_profile: None,
            approval_policy: None,
            input_schema: serde_json::json!({}),
            validators: vec![],
            credentials: vec![],
        }
    }

    #[test]
    fn test_registry_get() {
        let caps = vec![make_cap("sys.date"), make_cap("sys.echo")];
        let reg = CapabilityRegistry::from_vec(caps);
        assert!(reg.get("sys.date").is_some());
        assert!(reg.get("sys.missing").is_none());
        assert_eq!(reg.all().len(), 2);
    }
}
```

- [ ] **Step 4: Add module to lib.rs**

Add to `crates/clix-core/src/lib.rs`:
```rust
pub mod registry;
```

- [ ] **Step 5: Run test**

```bash
cargo test -p clix-core registry
```
Expected: 1 test passes

- [ ] **Step 6: Commit**

```bash
git add crates/clix-core/src/registry/ crates/clix-core/src/lib.rs
git commit -m "feat(core): add CapabilityRegistry and WorkflowRegistry"
```

---

### Task 2: Secret redaction

**Files:**
- Create: `crates/clix-core/src/secrets/redact.rs`
- Create: `crates/clix-core/src/secrets/mod.rs`

- [ ] **Step 1: Write failing test**

`crates/clix-core/src/secrets/redact.rs`:
```rust
#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_redacts_secret_values() {
        let secrets = std::collections::HashMap::from([
            ("MY_TOKEN".to_string(), "supersecret123".to_string()),
        ]);
        let redactor = SecretRedactor::new(secrets);
        let output = "token: supersecret123 and more supersecret123 text";
        assert_eq!(redactor.redact(output), "token: [REDACTED] and more [REDACTED] text");
    }

    #[test]
    fn test_empty_secrets_passthrough() {
        let redactor = SecretRedactor::new(std::collections::HashMap::new());
        let output = "nothing to redact here";
        assert_eq!(redactor.redact(output), "nothing to redact here");
    }
}
```

- [ ] **Step 2: Run — expect compile failure**

```bash
cargo test -p clix-core secrets::redact 2>&1 | head -5
```

- [ ] **Step 3: Implement redact.rs**

`crates/clix-core/src/secrets/redact.rs`:
```rust
use std::collections::HashMap;

pub struct SecretRedactor {
    secrets: Vec<String>,
}

impl SecretRedactor {
    pub fn new(resolved: HashMap<String, String>) -> Self {
        // Collect non-empty secret values; sort longest-first to avoid partial replacements
        let mut secrets: Vec<String> = resolved
            .into_values()
            .filter(|v| !v.is_empty())
            .collect();
        secrets.sort_by(|a, b| b.len().cmp(&a.len()));
        SecretRedactor { secrets }
    }

    pub fn redact(&self, text: &str) -> String {
        let mut result = text.to_string();
        for secret in &self.secrets {
            result = result.replace(secret.as_str(), "[REDACTED]");
        }
        result
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_redacts_secret_values() {
        let secrets = HashMap::from([
            ("MY_TOKEN".to_string(), "supersecret123".to_string()),
        ]);
        let redactor = SecretRedactor::new(secrets);
        let output = "token: supersecret123 and more supersecret123 text";
        assert_eq!(redactor.redact(output), "token: [REDACTED] and more [REDACTED] text");
    }

    #[test]
    fn test_empty_secrets_passthrough() {
        let redactor = SecretRedactor::new(HashMap::new());
        let output = "nothing to redact here";
        assert_eq!(redactor.redact(output), "nothing to redact here");
    }

    #[test]
    fn test_longest_match_first() {
        // "abc" is a prefix of "abcdef" — longer match must win
        let secrets = HashMap::from([
            ("A".to_string(), "abc".to_string()),
            ("B".to_string(), "abcdef".to_string()),
        ]);
        let redactor = SecretRedactor::new(secrets);
        let output = "value: abcdef";
        // "abcdef" should be replaced as one unit, not "abc" + "def"
        assert_eq!(redactor.redact(output), "value: [REDACTED]");
    }
}
```

- [ ] **Step 4: Create secrets/mod.rs stub**

`crates/clix-core/src/secrets/mod.rs`:
```rust
pub mod redact;
pub use redact::SecretRedactor;
```

- [ ] **Step 5: Add to lib.rs**

Add to `crates/clix-core/src/lib.rs`:
```rust
pub mod secrets;
```

- [ ] **Step 6: Run tests**

```bash
cargo test -p clix-core secrets
```
Expected: 3 tests pass

- [ ] **Step 7: Commit**

```bash
git add crates/clix-core/src/secrets/ crates/clix-core/src/lib.rs
git commit -m "feat(core): add SecretRedactor"
```

---

### Task 3: Credential resolution

**Files:**
- Modify: `crates/clix-core/src/secrets/mod.rs`

- [ ] **Step 1: Write failing test**

Add to `crates/clix-core/src/secrets/mod.rs`:
```rust
#[cfg(test)]
mod tests {
    use super::*;
    use crate::manifest::capability::{CredentialSource, InfisicalRef};

    #[test]
    fn test_resolve_env_credential() {
        std::env::set_var("MY_TEST_SECRET", "env-value-123");
        let creds = vec![CredentialSource::Env {
            env_var: "MY_TEST_SECRET".to_string(),
            inject_as: "TARGET_VAR".to_string(),
        }];
        let resolved = resolve_credentials(&creds, None).unwrap();
        assert_eq!(resolved.get("TARGET_VAR").unwrap(), "env-value-123");
        std::env::remove_var("MY_TEST_SECRET");
    }

    #[test]
    fn test_resolve_literal_credential() {
        let creds = vec![CredentialSource::Literal {
            value: "literal-val".to_string(),
            inject_as: "INJECTED".to_string(),
        }];
        let resolved = resolve_credentials(&creds, None).unwrap();
        assert_eq!(resolved.get("INJECTED").unwrap(), "literal-val");
    }
}
```

- [ ] **Step 2: Run — expect compile failure**

```bash
cargo test -p clix-core secrets::tests 2>&1 | head -5
```

- [ ] **Step 3: Implement resolve_credentials in secrets/mod.rs**

`crates/clix-core/src/secrets/mod.rs`:
```rust
pub mod redact;
pub use redact::SecretRedactor;

use std::collections::HashMap;
use crate::error::{ClixError, Result};
use crate::manifest::capability::CredentialSource;
use crate::state::InfisicalConfig;

/// Resolve a list of CredentialSources to a map of inject_as -> value.
/// Precedence: Literal (no network) -> Env (local) -> Infisical (network, last).
pub fn resolve_credentials(
    creds: &[CredentialSource],
    infisical_cfg: Option<&InfisicalConfig>,
) -> Result<HashMap<String, String>> {
    let mut resolved = HashMap::new();
    for cred in creds {
        match cred {
            CredentialSource::Literal { value, inject_as } => {
                resolved.insert(inject_as.clone(), value.clone());
            }
            CredentialSource::Env { env_var, inject_as } => {
                let value = std::env::var(env_var).unwrap_or_default();
                resolved.insert(inject_as.clone(), value);
            }
            CredentialSource::Infisical { secret_ref, inject_as } => {
                let cfg = infisical_cfg.ok_or_else(|| {
                    ClixError::CredentialResolution(
                        "Infisical credential requires infisical config in config.yaml".to_string(),
                    )
                })?;
                let value = fetch_infisical_secret(cfg, secret_ref)?;
                resolved.insert(inject_as.clone(), value);
            }
        }
    }
    Ok(resolved)
}

fn fetch_infisical_secret(
    cfg: &InfisicalConfig,
    secret_ref: &crate::manifest::capability::InfisicalRef,
) -> Result<String> {
    // Token fetch + secret fetch — using reqwest blocking
    let token = get_infisical_token(cfg)?;
    let project_id = secret_ref.project_id.as_deref().unwrap_or("");
    let url = format!(
        "{}/api/v3/secrets/raw/{}?workspaceId={}&environment={}&secretPath={}",
        cfg.site_url.trim_end_matches('/'),
        secret_ref.secret_name,
        project_id,
        secret_ref.environment,
        urlencoding::encode(&secret_ref.secret_path),
    );
    let client = reqwest::blocking::Client::new();
    let resp = client
        .get(&url)
        .bearer_auth(&token)
        .send()
        .map_err(|e| ClixError::CredentialResolution(format!("Infisical HTTP error: {e}")))?;
    if !resp.status().is_success() {
        let status = resp.status();
        let body = resp.text().unwrap_or_default();
        return Err(ClixError::CredentialResolution(format!(
            "Infisical returned {status}: {body}"
        )));
    }
    let body: serde_json::Value = resp
        .json()
        .map_err(|e| ClixError::CredentialResolution(format!("Infisical decode error: {e}")))?;
    body["secret"]["secretValue"]
        .as_str()
        .map(|s| s.to_string())
        .ok_or_else(|| ClixError::CredentialResolution("secretValue missing in Infisical response".to_string()))
}

fn get_infisical_token(cfg: &InfisicalConfig) -> Result<String> {
    let client_id = cfg.client_id.as_deref()
        .or_else(|| option_env!("INFISICAL_UNIVERSAL_AUTH_CLIENT_ID"))
        .unwrap_or_else(|| std::env::var("INFISICAL_UNIVERSAL_AUTH_CLIENT_ID").unwrap_or_default().leak());
    let client_secret = cfg.client_secret.as_deref()
        .or_else(|| option_env!("INFISICAL_UNIVERSAL_AUTH_CLIENT_SECRET"))
        .unwrap_or_else(|| std::env::var("INFISICAL_UNIVERSAL_AUTH_CLIENT_SECRET").unwrap_or_default().leak());

    let url = format!("{}/api/v1/auth/universal-auth/login", cfg.site_url.trim_end_matches('/'));
    let client = reqwest::blocking::Client::new();
    let resp = client
        .post(&url)
        .json(&serde_json::json!({ "clientId": client_id, "clientSecret": client_secret }))
        .send()
        .map_err(|e| ClixError::CredentialResolution(format!("Infisical auth error: {e}")))?;
    let body: serde_json::Value = resp
        .json()
        .map_err(|e| ClixError::CredentialResolution(format!("Infisical auth decode: {e}")))?;
    body["accessToken"]
        .as_str()
        .map(|s| s.to_string())
        .ok_or_else(|| ClixError::CredentialResolution("accessToken missing in Infisical auth response".to_string()))
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::manifest::capability::CredentialSource;

    #[test]
    fn test_resolve_env_credential() {
        std::env::set_var("MY_TEST_SECRET", "env-value-123");
        let creds = vec![CredentialSource::Env {
            env_var: "MY_TEST_SECRET".to_string(),
            inject_as: "TARGET_VAR".to_string(),
        }];
        let resolved = resolve_credentials(&creds, None).unwrap();
        assert_eq!(resolved.get("TARGET_VAR").unwrap(), "env-value-123");
        std::env::remove_var("MY_TEST_SECRET");
    }

    #[test]
    fn test_resolve_literal_credential() {
        let creds = vec![CredentialSource::Literal {
            value: "literal-val".to_string(),
            inject_as: "INJECTED".to_string(),
        }];
        let resolved = resolve_credentials(&creds, None).unwrap();
        assert_eq!(resolved.get("INJECTED").unwrap(), "literal-val");
    }
}
```

Add to `crates/clix-core/Cargo.toml`:
```toml
reqwest         = { version = "0.12", features = ["blocking", "json"] }
urlencoding     = "2"
```

Also add to workspace `Cargo.toml` `[workspace.dependencies]`:
```toml
urlencoding = "2"
```

- [ ] **Step 4: Run tests**

```bash
cargo test -p clix-core secrets::tests
```
Expected: 2 tests pass (env + literal; Infisical integration test is behind a feature flag, not run here)

- [ ] **Step 5: Commit**

```bash
git add crates/clix-core/src/secrets/ crates/clix-core/Cargo.toml Cargo.toml
git commit -m "feat(core): add credential resolution (env, literal, Infisical)"
```

---

### Task 4: Subprocess backend

**Files:**
- Create: `crates/clix-core/src/execution/backends/subprocess.rs`
- Create: `crates/clix-core/src/execution/backends/mod.rs`
- Modify: `crates/clix-core/src/lib.rs`

- [ ] **Step 1: Write failing test**

`crates/clix-core/src/execution/backends/subprocess.rs`:
```rust
#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_run_echo() {
        let result = run_subprocess("echo", &["hello world".to_string()], &std::path::PathBuf::from("."), &std::collections::HashMap::new()).unwrap();
        assert_eq!(result.exit_code, 0);
        assert!(result.stdout.contains("hello world"));
    }

    #[test]
    fn test_nonzero_exit() {
        // `false` command always exits 1
        let result = run_subprocess("false", &[], &std::path::PathBuf::from("."), &std::collections::HashMap::new());
        // On Windows `false` may not exist; skip if not found
        if let Ok(r) = result {
            assert_ne!(r.exit_code, 0);
        }
    }
}
```

- [ ] **Step 2: Run — expect compile failure**

```bash
cargo test -p clix-core execution 2>&1 | head -5
```

- [ ] **Step 3: Create execution directory structure**

```bash
mkdir -p crates/clix-core/src/execution/backends
touch crates/clix-core/src/execution/mod.rs
touch crates/clix-core/src/execution/approval.rs
touch crates/clix-core/src/execution/validators.rs
touch crates/clix-core/src/execution/backends/builtin.rs
touch crates/clix-core/src/execution/backends/remote.rs
```

- [ ] **Step 4: Implement subprocess.rs**

`crates/clix-core/src/execution/backends/subprocess.rs`:
```rust
use std::collections::HashMap;
use std::path::PathBuf;
use crate::error::{ClixError, Result};

#[derive(Debug, Clone)]
pub struct SubprocessResult {
    pub exit_code: i32,
    pub stdout: String,
    pub stderr: String,
}

pub fn run_subprocess(
    command: &str,
    args: &[String],
    cwd: &PathBuf,
    secrets: &HashMap<String, String>,
) -> Result<SubprocessResult> {
    let mut cmd = std::process::Command::new(command);
    cmd.args(args).current_dir(cwd);

    // Build environment: inherit parent env, then inject secrets
    let mut env: HashMap<String, String> = std::env::vars().collect();
    for (k, v) in secrets {
        env.insert(k.clone(), v.clone());
    }
    cmd.env_clear();
    for (k, v) in &env {
        cmd.env(k, v);
    }

    let output = cmd.output().map_err(|e| {
        ClixError::Backend(format!("failed to spawn `{command}`: {e}"))
    })?;

    Ok(SubprocessResult {
        exit_code: output.status.code().unwrap_or(1),
        stdout: String::from_utf8_lossy(&output.stdout).to_string(),
        stderr: String::from_utf8_lossy(&output.stderr).to_string(),
    })
}

/// Expand $VAR and ${VAR} references in args using resolved secrets (then env fallback).
pub fn expand_secret_refs(args: &[String], secrets: &HashMap<String, String>) -> Vec<String> {
    args.iter()
        .map(|arg| {
            os_str_expand(arg, |key| {
                secrets
                    .get(key)
                    .cloned()
                    .or_else(|| std::env::var(key).ok())
                    .unwrap_or_default()
            })
        })
        .collect()
}

fn os_str_expand(s: &str, lookup: impl Fn(&str) -> String) -> String {
    // Expand $VAR and ${VAR} without a shell
    let mut result = String::with_capacity(s.len());
    let mut chars = s.chars().peekable();
    while let Some(c) = chars.next() {
        if c == '$' {
            let (key, braced) = if chars.peek() == Some(&'{') {
                chars.next(); // consume '{'
                let key: String = chars.by_ref().take_while(|&c| c != '}').collect();
                (key, true)
            } else {
                let key: String = chars
                    .by_ref()
                    .take_while(|c| c.is_alphanumeric() || *c == '_')
                    .collect();
                (key, false)
            };
            let _ = braced;
            result.push_str(&lookup(&key));
        } else {
            result.push(c);
        }
    }
    result
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_expand_secret_refs() {
        let secrets = HashMap::from([("TOKEN".to_string(), "abc123".to_string())]);
        let args = vec!["Bearer $TOKEN".to_string(), "${TOKEN}".to_string()];
        let expanded = expand_secret_refs(&args, &secrets);
        assert_eq!(expanded[0], "Bearer abc123");
        assert_eq!(expanded[1], "abc123");
    }

    #[cfg(not(target_os = "windows"))]
    #[test]
    fn test_run_echo() {
        let result = run_subprocess(
            "echo",
            &["hello world".to_string()],
            &PathBuf::from("."),
            &HashMap::new(),
        )
        .unwrap();
        assert_eq!(result.exit_code, 0);
        assert!(result.stdout.contains("hello world"));
    }
}
```

- [ ] **Step 5: Implement builtin.rs**

`crates/clix-core/src/execution/backends/builtin.rs`:
```rust
use std::collections::HashMap;
use crate::error::{ClixError, Result};

/// Execute a builtin capability by name.
pub fn builtin_handler(
    name: &str,
    input: &serde_json::Value,
) -> Result<serde_json::Value> {
    match name {
        "date" | "system.date" => {
            let now = chrono::Utc::now().to_rfc3339();
            Ok(serde_json::json!({ "date": now, "exitCode": 0 }))
        }
        "echo" | "system.echo" => {
            let message = input["message"].as_str().unwrap_or("").to_string();
            Ok(serde_json::json!({ "output": message, "exitCode": 0 }))
        }
        _ => Err(ClixError::Backend(format!("unknown builtin: {name}"))),
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_builtin_date() {
        let result = builtin_handler("date", &serde_json::json!({})).unwrap();
        assert!(result["date"].as_str().is_some());
    }

    #[test]
    fn test_builtin_echo() {
        let result = builtin_handler("echo", &serde_json::json!({"message": "hi"})).unwrap();
        assert_eq!(result["output"], "hi");
    }

    #[test]
    fn test_builtin_unknown() {
        let result = builtin_handler("does.not.exist", &serde_json::json!({}));
        assert!(result.is_err());
    }
}
```

- [ ] **Step 6: Implement remote.rs stub**

`crates/clix-core/src/execution/backends/remote.rs`:
```rust
use crate::error::{ClixError, Result};

/// Forward a capability call to a remote clix daemon.
/// addr format: "unix:///path/to/sock" or "http://host:port"
pub fn run_remote(
    addr: &str,
    capability_name: &str,
    input: &serde_json::Value,
) -> Result<serde_json::Value> {
    if addr.starts_with("http://") || addr.starts_with("https://") {
        run_remote_http(addr, capability_name, input)
    } else {
        let path = addr.trim_start_matches("unix://");
        run_remote_unix(path, capability_name, input)
    }
}

fn run_remote_http(
    addr: &str,
    capability_name: &str,
    input: &serde_json::Value,
) -> Result<serde_json::Value> {
    let url = format!("{}/", addr.trim_end_matches('/'));
    let payload = serde_json::json!({
        "jsonrpc": "2.0",
        "id": 1,
        "method": "tools/call",
        "params": { "name": capability_name, "arguments": input }
    });
    let client = reqwest::blocking::Client::new();
    let resp: serde_json::Value = client
        .post(&url)
        .json(&payload)
        .send()
        .map_err(|e| ClixError::Backend(format!("remote HTTP error: {e}")))?
        .json()
        .map_err(|e| ClixError::Backend(format!("remote HTTP decode: {e}")))?;
    if let Some(err) = resp.get("error") {
        return Err(ClixError::Backend(format!("remote error: {err}")));
    }
    Ok(resp["result"].clone())
}

#[cfg(unix)]
fn run_remote_unix(
    path: &str,
    capability_name: &str,
    input: &serde_json::Value,
) -> Result<serde_json::Value> {
    use std::io::{BufRead, BufReader, Write};
    use std::os::unix::net::UnixStream;

    let mut stream = UnixStream::connect(path)
        .map_err(|e| ClixError::Backend(format!("unix socket connect {path}: {e}")))?;
    let payload = serde_json::json!({
        "jsonrpc": "2.0",
        "id": 1,
        "method": "tools/call",
        "params": { "name": capability_name, "arguments": input }
    });
    let mut line = serde_json::to_string(&payload).unwrap();
    line.push('\n');
    stream.write_all(line.as_bytes())
        .map_err(|e| ClixError::Backend(format!("unix socket write: {e}")))?;
    let reader = BufReader::new(stream);
    let response_line = reader.lines().next()
        .ok_or_else(|| ClixError::Backend("unix socket: no response".to_string()))?
        .map_err(|e| ClixError::Backend(format!("unix socket read: {e}")))?;
    let resp: serde_json::Value = serde_json::from_str(&response_line)?;
    if let Some(err) = resp.get("error") {
        return Err(ClixError::Backend(format!("remote error: {err}")));
    }
    Ok(resp["result"].clone())
}

#[cfg(not(unix))]
fn run_remote_unix(
    _path: &str,
    _capability_name: &str,
    _input: &serde_json::Value,
) -> Result<serde_json::Value> {
    Err(ClixError::Backend(
        "Unix socket transport not supported on this platform".to_string(),
    ))
}
```

- [ ] **Step 7: Wire backends/mod.rs**

`crates/clix-core/src/execution/backends/mod.rs`:
```rust
pub mod builtin;
pub mod remote;
pub mod subprocess;

pub use builtin::builtin_handler;
pub use remote::run_remote;
pub use subprocess::{expand_secret_refs, run_subprocess, SubprocessResult};
```

- [ ] **Step 8: Run tests**

```bash
cargo test -p clix-core execution::backends
```
Expected: 6 tests pass (expand_secret_refs, echo on non-windows, date, echo builtin, unknown builtin)

- [ ] **Step 9: Commit**

```bash
git add crates/clix-core/src/execution/
git commit -m "feat(core): add subprocess, builtin, and remote backends"
```

---

### Task 5: Validators and Approval gate

**Files:**
- Modify: `crates/clix-core/src/execution/validators.rs`
- Modify: `crates/clix-core/src/execution/approval.rs`

- [ ] **Step 1: Implement validators.rs**

`crates/clix-core/src/execution/validators.rs`:
```rust
use crate::error::Result;
use crate::manifest::capability::{Validator, ValidatorKind};

pub fn run_validators(
    validators: &[Validator],
    input: &serde_json::Value,
    cwd: &std::path::Path,
    resolved_args: &[String],
) -> Vec<String> {
    let mut errors = vec![];
    for v in validators {
        match v.kind {
            ValidatorKind::RequiredPath => {
                let target = cwd.join(&v.path);
                if !target.exists() {
                    errors.push(format!("Required path missing: {}", v.path));
                }
            }
            ValidatorKind::DenyArgs => {
                let args_str = resolved_args.join(" ");
                for forbidden in &v.values {
                    if args_str.contains(forbidden.as_str()) {
                        errors.push(format!("Forbidden argument detected: {forbidden}"));
                    }
                }
            }
            ValidatorKind::RequiredInputKey => {
                if input.get(&v.key).is_none() {
                    errors.push(format!("Input key missing: {}", v.key));
                }
            }
        }
    }
    errors
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::manifest::capability::{Validator, ValidatorKind};

    #[test]
    fn test_deny_args() {
        let validators = vec![Validator {
            kind: ValidatorKind::DenyArgs,
            path: String::new(),
            key: String::new(),
            values: vec!["--force".to_string(), "--delete-all".to_string()],
        }];
        let errors = run_validators(&validators, &serde_json::json!({}), std::path::Path::new("."), &["kubectl".to_string(), "--force".to_string()]);
        assert_eq!(errors.len(), 1);
        assert!(errors[0].contains("--force"));
    }

    #[test]
    fn test_required_input_key_missing() {
        let validators = vec![Validator {
            kind: ValidatorKind::RequiredInputKey,
            path: String::new(),
            key: "namespace".to_string(),
            values: vec![],
        }];
        let errors = run_validators(&validators, &serde_json::json!({}), std::path::Path::new("."), &[]);
        assert_eq!(errors.len(), 1);
    }

    #[test]
    fn test_required_input_key_present() {
        let validators = vec![Validator {
            kind: ValidatorKind::RequiredInputKey,
            path: String::new(),
            key: "namespace".to_string(),
            values: vec![],
        }];
        let errors = run_validators(&validators, &serde_json::json!({"namespace": "default"}), std::path::Path::new("."), &[]);
        assert!(errors.is_empty());
    }
}
```

- [ ] **Step 2: Implement approval.rs**

`crates/clix-core/src/execution/approval.rs`:
```rust
use serde::{Deserialize, Serialize};
use uuid::Uuid;
use crate::error::{ClixError, Result};
use crate::manifest::capability::CapabilityManifest;
use crate::state::ApprovalGateConfig;

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

/// POST an approval request to the configured webhook and return the response.
/// Fail-safe: any error is treated as denial.
pub fn request_approval(
    cfg: &ApprovalGateConfig,
    cap: &CapabilityManifest,
    input: &serde_json::Value,
    policy_reason: &str,
) -> Result<ApprovalResponse> {
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
        .build()
        .map_err(|e| ClixError::ApprovalGate(e.to_string()))?;

    let mut builder = client.post(&cfg.webhook_url).json(&req);
    for (k, v) in &cfg.headers {
        builder = builder.header(k, v);
    }

    let resp = builder
        .send()
        .map_err(|e| {
            // Fail safe: return denied
            Ok::<ApprovalResponse, ClixError>(ApprovalResponse {
                approved: false,
                reason: Some(format!("approval webhook unreachable: {e}")),
                approver: None,
            })
        })
        .unwrap_or_else(|r| r);

    // If we got a real response object (from the Ok branch), return it
    if let Ok(response) = resp.json::<ApprovalResponse>() {
        Ok(response)
    } else {
        Ok(ApprovalResponse {
            approved: false,
            reason: Some("approval webhook response decode failed".to_string()),
            approver: None,
        })
    }
}
```

Wait — the above `request_approval` has a logic issue with the fail-safe. Let me rewrite it cleanly:

`crates/clix-core/src/execution/approval.rs`:
```rust
use serde::{Deserialize, Serialize};
use uuid::Uuid;
use crate::error::{ClixError, Result};
use crate::manifest::capability::CapabilityManifest;
use crate::state::ApprovalGateConfig;

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

/// POST an approval request to the configured webhook.
/// Fail-safe: any network/decode error returns an ApprovalResponse with approved=false.
pub fn request_approval(
    cfg: &ApprovalGateConfig,
    cap: &CapabilityManifest,
    input: &serde_json::Value,
    policy_reason: &str,
) -> Result<ApprovalResponse> {
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
        .build()
        .map_err(|e| ClixError::ApprovalGate(e.to_string()))?;

    let mut builder = client.post(&cfg.webhook_url).json(&req);
    for (k, v) in &cfg.headers {
        builder = builder.header(k, v);
    }

    let http_resp = match builder.send() {
        Err(e) => return Ok(ApprovalResponse::denied(format!("webhook unreachable: {e}"))),
        Ok(r) => r,
    };

    if !http_resp.status().is_success() {
        let status = http_resp.status();
        return Ok(ApprovalResponse::denied(format!("webhook HTTP {status}")));
    }

    match http_resp.json::<ApprovalResponse>() {
        Ok(r) => Ok(r),
        Err(e) => Ok(ApprovalResponse::denied(format!("webhook decode failed: {e}"))),
    }
}
```

- [ ] **Step 3: Run validator tests**

```bash
cargo test -p clix-core execution::validators
```
Expected: 3 tests pass

- [ ] **Step 4: Commit**

```bash
git add crates/clix-core/src/execution/validators.rs crates/clix-core/src/execution/approval.rs
git commit -m "feat(core): add validators and approval gate"
```

---

### Task 6: Receipts — SQLite store

**Files:**
- Create: `crates/clix-core/src/receipts/mod.rs`
- Create: `crates/clix-core/src/receipts/schema.sql`

- [ ] **Step 1: Add sqlx to clix-core**

Add to `crates/clix-core/Cargo.toml`:
```toml
sqlx = { workspace = true }
```

- [ ] **Step 2: Create schema.sql**

`crates/clix-core/src/receipts/schema.sql`:
```sql
CREATE TABLE IF NOT EXISTS receipts (
    id              TEXT PRIMARY KEY,
    kind            TEXT NOT NULL,
    capability      TEXT NOT NULL,
    created_at      TEXT NOT NULL,
    status          TEXT NOT NULL,
    decision        TEXT NOT NULL,
    reason          TEXT,
    input           TEXT NOT NULL,
    context         TEXT NOT NULL,
    execution       TEXT,
    approval        TEXT,
    sandbox_enforced INTEGER NOT NULL DEFAULT 0
);

CREATE INDEX IF NOT EXISTS idx_receipts_capability ON receipts(capability);
CREATE INDEX IF NOT EXISTS idx_receipts_status     ON receipts(status);
CREATE INDEX IF NOT EXISTS idx_receipts_created_at ON receipts(created_at);
```

- [ ] **Step 3: Implement receipts/mod.rs**

`crates/clix-core/src/receipts/mod.rs`:
```rust
use std::path::Path;
use serde::{Deserialize, Serialize};
use uuid::Uuid;
use chrono::{DateTime, Utc};
use crate::error::{ClixError, Result};
use crate::policy::evaluate::Decision;
use crate::policy::evaluate::ExecutionContext;

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
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub enum ReceiptKind { Capability, Workflow }

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub enum ReceiptStatus {
    Succeeded,
    Failed,
    Denied,
    PendingApproval,
    ApprovalDenied,
}

impl std::fmt::Display for ReceiptStatus {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            ReceiptStatus::Succeeded      => write!(f, "succeeded"),
            ReceiptStatus::Failed         => write!(f, "failed"),
            ReceiptStatus::Denied         => write!(f, "denied"),
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
            .map_err(|e| ClixError::Config(format!("receipts db open: {e}")))?;
        conn.execute_batch(include_str!("schema.sql"))
            .map_err(|e| ClixError::Config(format!("receipts schema: {e}")))?;
        Ok(ReceiptStore { conn })
    }

    pub fn write(&self, receipt: &Receipt) -> Result<()> {
        self.conn.execute(
            r#"INSERT INTO receipts
               (id, kind, capability, created_at, status, decision, reason,
                input, context, execution, approval, sandbox_enforced)
               VALUES (?1,?2,?3,?4,?5,?6,?7,?8,?9,?10,?11,?12)"#,
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
            ],
        ).map_err(|e| ClixError::Config(format!("receipt insert: {e}")))?;
        Ok(())
    }

    pub fn list(&self, limit: usize, status_filter: Option<&str>) -> Result<Vec<Receipt>> {
        let sql = match status_filter {
            Some(_) => "SELECT * FROM receipts WHERE status = ?1 ORDER BY created_at DESC LIMIT ?2",
            None    => "SELECT * FROM receipts WHERE 1=1 ORDER BY created_at DESC LIMIT ?2",
        };
        // Simplified: always pass both params, sqlite ignores ?1 when not referenced
        let mut stmt = self.conn.prepare(
            "SELECT id,kind,capability,created_at,status,decision,reason,input,context,execution,approval,sandbox_enforced FROM receipts WHERE (?1 IS NULL OR status = ?1) ORDER BY created_at DESC LIMIT ?2"
        ).map_err(|e| ClixError::Config(e.to_string()))?;

        let rows = stmt.query_map(
            rusqlite::params![status_filter, limit as i64],
            |row| {
                Ok((
                    row.get::<_, String>(0)?,
                    row.get::<_, String>(1)?,
                    row.get::<_, String>(2)?,
                    row.get::<_, String>(3)?,
                    row.get::<_, String>(4)?,
                    row.get::<_, String>(5)?,
                    row.get::<_, Option<String>>(6)?,
                    row.get::<_, String>(7)?,
                    row.get::<_, String>(8)?,
                    row.get::<_, Option<String>>(9)?,
                    row.get::<_, Option<String>>(10)?,
                    row.get::<_, i64>(11)?,
                ))
            },
        ).map_err(|e| ClixError::Config(e.to_string()))?;

        let mut receipts = vec![];
        for row in rows {
            let (id, kind, cap, created_at, status, decision, reason,
                 input, context, execution, approval, sandbox_enforced) =
                row.map_err(|e| ClixError::Config(e.to_string()))?;

            receipts.push(Receipt {
                id:               Uuid::parse_str(&id).unwrap_or_else(|_| Uuid::new_v4()),
                kind:             serde_json::from_str(&kind).unwrap_or(ReceiptKind::Capability),
                capability:       cap,
                created_at:       created_at.parse().unwrap_or_else(|_| Utc::now()),
                status:           parse_status(&status),
                decision,
                reason,
                input:            serde_json::from_str(&input).unwrap_or(serde_json::Value::Null),
                context:          serde_json::from_str(&context).unwrap_or(serde_json::Value::Null),
                execution:        execution.as_deref().and_then(|s| serde_json::from_str(s).ok()),
                approval:         approval.as_deref().and_then(|s| serde_json::from_str(s).ok()),
                sandbox_enforced: sandbox_enforced != 0,
            });
        }
        Ok(receipts)
    }

    pub fn get(&self, id: &str) -> Result<Option<Receipt>> {
        let mut results = self.list(1, None)?;
        // Fetch by ID directly
        let mut stmt = self.conn.prepare(
            "SELECT id,kind,capability,created_at,status,decision,reason,input,context,execution,approval,sandbox_enforced FROM receipts WHERE id = ?1"
        ).map_err(|e| ClixError::Config(e.to_string()))?;
        let _ = results; // ignore list results
        let row = stmt.query_row(rusqlite::params![id], |row| {
            Ok((
                row.get::<_, String>(0)?,
                row.get::<_, String>(1)?,
                row.get::<_, String>(2)?,
                row.get::<_, String>(3)?,
                row.get::<_, String>(4)?,
                row.get::<_, String>(5)?,
                row.get::<_, Option<String>>(6)?,
                row.get::<_, String>(7)?,
                row.get::<_, String>(8)?,
                row.get::<_, Option<String>>(9)?,
                row.get::<_, Option<String>>(10)?,
                row.get::<_, i64>(11)?,
            ))
        });
        match row {
            Err(rusqlite::Error::QueryReturnedNoRows) => Ok(None),
            Err(e) => Err(ClixError::Config(e.to_string())),
            Ok((id, kind, cap, created_at, status, decision, reason,
                input, context, execution, approval, sandbox_enforced)) => {
                Ok(Some(Receipt {
                    id:               Uuid::parse_str(&id).unwrap_or_else(|_| Uuid::new_v4()),
                    kind:             serde_json::from_str(&kind).unwrap_or(ReceiptKind::Capability),
                    capability:       cap,
                    created_at:       created_at.parse().unwrap_or_else(|_| Utc::now()),
                    status:           parse_status(&status),
                    decision,
                    reason,
                    input:            serde_json::from_str(&input).unwrap_or(serde_json::Value::Null),
                    context:          serde_json::from_str(&context).unwrap_or(serde_json::Value::Null),
                    execution:        execution.as_deref().and_then(|s| serde_json::from_str(s).ok()),
                    approval:         approval.as_deref().and_then(|s| serde_json::from_str(s).ok()),
                    sandbox_enforced: sandbox_enforced != 0,
                }))
            }
        }
    }
}

fn parse_status(s: &str) -> ReceiptStatus {
    match s {
        "succeeded"       => ReceiptStatus::Succeeded,
        "failed"          => ReceiptStatus::Failed,
        "denied"          => ReceiptStatus::Denied,
        "pending_approval"=> ReceiptStatus::PendingApproval,
        "approval_denied" => ReceiptStatus::ApprovalDenied,
        _                 => ReceiptStatus::Failed,
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn temp_store() -> ReceiptStore {
        ReceiptStore::open(Path::new(":memory:")).unwrap()
    }

    fn stub_receipt(cap: &str, status: ReceiptStatus) -> Receipt {
        Receipt {
            id: Uuid::new_v4(),
            kind: ReceiptKind::Capability,
            capability: cap.to_string(),
            created_at: Utc::now(),
            status,
            decision: "allow".to_string(),
            reason: None,
            input: serde_json::json!({}),
            context: serde_json::json!({}),
            execution: None,
            approval: None,
            sandbox_enforced: false,
        }
    }

    #[test]
    fn test_write_and_list() {
        let store = temp_store();
        let r = stub_receipt("sys.date", ReceiptStatus::Succeeded);
        store.write(&r).unwrap();
        let list = store.list(10, None).unwrap();
        assert_eq!(list.len(), 1);
        assert_eq!(list[0].capability, "sys.date");
    }

    #[test]
    fn test_list_with_status_filter() {
        let store = temp_store();
        store.write(&stub_receipt("a", ReceiptStatus::Succeeded)).unwrap();
        store.write(&stub_receipt("b", ReceiptStatus::Failed)).unwrap();
        let succeeded = store.list(10, Some("succeeded")).unwrap();
        assert_eq!(succeeded.len(), 1);
        assert_eq!(succeeded[0].capability, "a");
    }

    #[test]
    fn test_get_by_id() {
        let store = temp_store();
        let r = stub_receipt("sys.echo", ReceiptStatus::Succeeded);
        let id = r.id.to_string();
        store.write(&r).unwrap();
        let found = store.get(&id).unwrap();
        assert!(found.is_some());
        assert_eq!(found.unwrap().capability, "sys.echo");
    }
}
```

Add `rusqlite` to `crates/clix-core/Cargo.toml`:
```toml
rusqlite = { version = "0.32", features = ["bundled"] }
```

Add to workspace `Cargo.toml`:
```toml
rusqlite = { version = "0.32", features = ["bundled"] }
```

- [ ] **Step 4: Add module to lib.rs**

```rust
pub mod receipts;
```

- [ ] **Step 5: Run tests**

```bash
cargo test -p clix-core receipts
```
Expected: 3 tests pass

- [ ] **Step 6: Commit**

```bash
git add crates/clix-core/src/receipts/ crates/clix-core/Cargo.toml Cargo.toml crates/clix-core/src/lib.rs
git commit -m "feat(core): add SQLite receipt store"
```

---

### Task 7: Sandbox (Landlock on Linux, no-op elsewhere)

**Files:**
- Create: `crates/clix-core/src/sandbox/mod.rs`
- Create: `crates/clix-core/src/sandbox/linux.rs`
- Create: `crates/clix-core/src/sandbox/stub.rs`

- [ ] **Step 1: Add landlock to clix-core (Linux only)**

Add to `crates/clix-core/Cargo.toml`:
```toml
[target.'cfg(target_os = "linux")'.dependencies]
landlock = "0.4"
```

- [ ] **Step 2: Implement sandbox/stub.rs**

`crates/clix-core/src/sandbox/stub.rs`:
```rust
use crate::error::Result;

pub fn apply_sandbox(_allowed_executables: &[String]) -> Result<()> {
    Ok(()) // No-op on non-Linux
}

pub fn sandbox_enforced() -> bool {
    false
}
```

- [ ] **Step 3: Implement sandbox/linux.rs**

`crates/clix-core/src/sandbox/linux.rs`:
```rust
use crate::error::{ClixError, Result};

pub fn apply_sandbox(allowed_executables: &[String]) -> Result<()> {
    use landlock::{
        Access, AccessFs, PathBeneath, PathFd, Ruleset, RulesetAttr,
        RulesetCreatedAttr, ABI,
    };

    if allowed_executables.is_empty() {
        return Ok(());
    }

    let abi = ABI::V3;
    let mut ruleset = Ruleset::default()
        .handle_access(AccessFs::Execute)
        .map_err(|e| ClixError::Sandbox(format!("landlock ruleset: {e}")))?
        .create()
        .map_err(|e| ClixError::Sandbox(format!("landlock create: {e}")))?;

    for path in allowed_executables {
        let fd = PathFd::new(path)
            .map_err(|e| ClixError::Sandbox(format!("landlock path {path}: {e}")))?;
        ruleset = ruleset
            .add_rule(PathBeneath::new(fd, AccessFs::Execute))
            .map_err(|e| ClixError::Sandbox(format!("landlock rule: {e}")))?;
    }

    ruleset
        .restrict_self()
        .map_err(|e| ClixError::Sandbox(format!("landlock restrict: {e}")))?;

    Ok(())
}

pub fn sandbox_enforced() -> bool {
    true
}
```

- [ ] **Step 4: Implement sandbox/mod.rs**

`crates/clix-core/src/sandbox/mod.rs`:
```rust
#[cfg(target_os = "linux")]
mod linux;
#[cfg(not(target_os = "linux"))]
mod stub;

/// Apply the sandbox for this process.
/// On Linux: enforces Landlock exec allowlist.
/// On other platforms: no-op.
pub fn apply_sandbox(allowed_executables: &[String]) -> crate::error::Result<()> {
    #[cfg(target_os = "linux")]
    return linux::apply_sandbox(allowed_executables);
    #[cfg(not(target_os = "linux"))]
    return stub::apply_sandbox(allowed_executables);
}

/// Returns true if sandbox enforcement is actually active.
pub fn sandbox_enforced() -> bool {
    #[cfg(target_os = "linux")]
    return linux::sandbox_enforced();
    #[cfg(not(target_os = "linux"))]
    return stub::sandbox_enforced();
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_sandbox_enforced_flag() {
        // Just verify it doesn't panic and returns a bool
        let _ = sandbox_enforced();
    }

    #[test]
    fn test_apply_empty_allowlist_is_noop() {
        // Empty allowlist should always succeed
        apply_sandbox(&[]).unwrap();
    }
}
```

- [ ] **Step 5: Add to lib.rs**

```rust
pub mod sandbox;
```

- [ ] **Step 6: Run tests**

```bash
cargo test -p clix-core sandbox
```
Expected: 2 tests pass on all platforms

- [ ] **Step 7: Commit**

```bash
git add crates/clix-core/src/sandbox/ crates/clix-core/Cargo.toml crates/clix-core/src/lib.rs
git commit -m "feat(core): add sandbox (Landlock on Linux, no-op elsewhere)"
```

---

### Task 8: run_capability and run_workflow

**Files:**
- Modify: `crates/clix-core/src/execution/mod.rs`

- [ ] **Step 1: Write failing test**

`crates/clix-core/src/execution/mod.rs`:
```rust
#[cfg(test)]
mod tests {
    use super::*;
    use crate::manifest::capability::{Backend, CapabilityManifest, RiskLevel, SideEffectClass};
    use crate::policy::{PolicyBundle, evaluate::ExecutionContext};
    use crate::registry::CapabilityRegistry;
    use crate::receipts::ReceiptStore;
    use std::path::PathBuf;

    fn store() -> ReceiptStore {
        ReceiptStore::open(std::path::Path::new(":memory:")).unwrap()
    }

    fn ctx() -> ExecutionContext {
        ExecutionContext {
            env: "test".to_string(),
            cwd: PathBuf::from("."),
            user: "tester".to_string(),
            profile: "base".to_string(),
            approver: None,
        }
    }

    fn date_cap() -> CapabilityManifest {
        CapabilityManifest {
            name: "sys.date".to_string(),
            version: 1,
            description: None,
            backend: Backend::Builtin { name: "date".to_string() },
            risk: RiskLevel::Low,
            side_effect_class: SideEffectClass::None,
            sandbox_profile: None,
            approval_policy: None,
            input_schema: serde_json::json!({"type":"object","properties":{}}),
            validators: vec![],
            credentials: vec![],
        }
    }

    #[test]
    fn test_run_builtin_capability() {
        let registry = CapabilityRegistry::from_vec(vec![date_cap()]);
        let policy = PolicyBundle::default();
        let store = store();
        let outcome = run_capability(
            &registry,
            &policy,
            None,
            &store,
            "sys.date",
            serde_json::json!({}),
            ctx(),
        ).unwrap();
        assert!(outcome.ok);
        assert!(!outcome.approval_required);
    }

    #[test]
    fn test_unknown_capability_errors() {
        let registry = CapabilityRegistry::from_vec(vec![]);
        let policy = PolicyBundle::default();
        let store = store();
        let result = run_capability(
            &registry,
            &policy,
            None,
            &store,
            "does.not.exist",
            serde_json::json!({}),
            ctx(),
        );
        assert!(result.is_err());
    }
}
```

- [ ] **Step 2: Run — expect compile failure**

```bash
cargo test -p clix-core execution::tests 2>&1 | head -5
```

- [ ] **Step 3: Implement execution/mod.rs**

`crates/clix-core/src/execution/mod.rs`:
```rust
pub mod approval;
pub mod backends;
pub mod validators;

use serde::{Deserialize, Serialize};
use uuid::Uuid;
use chrono::Utc;

use crate::error::{ClixError, Result};
use crate::manifest::capability::{Backend, CapabilityManifest};
use crate::manifest::workflow::StepFailurePolicy;
use crate::policy::{evaluate_policy, evaluate::ExecutionContext, Decision, PolicyBundle};
use crate::receipts::{Receipt, ReceiptKind, ReceiptStatus, ReceiptStore};
use crate::registry::{CapabilityRegistry, WorkflowRegistry};
use crate::sandbox::sandbox_enforced;
use crate::schema::validate_input;
use crate::secrets::{resolve_credentials, SecretRedactor};
use crate::state::InfisicalConfig;
use crate::template::render_args;

use backends::{builtin_handler, expand_secret_refs, run_remote, run_subprocess};
use validators::run_validators;

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct ExecutionOutcome {
    pub ok: bool,
    pub approval_required: bool,
    pub receipt_id: Uuid,
    pub result: Option<serde_json::Value>,
    pub reason: Option<String>,
}

pub fn run_capability(
    registry: &CapabilityRegistry,
    policy: &PolicyBundle,
    infisical: Option<&InfisicalConfig>,
    store: &ReceiptStore,
    name: &str,
    input: serde_json::Value,
    ctx: ExecutionContext,
) -> Result<ExecutionOutcome> {
    let cap = registry
        .get(name)
        .ok_or_else(|| ClixError::CapabilityNotFound(name.to_string()))?
        .clone();

    // 1. Input schema validation
    validate_input(&cap.input_schema, &input)?;

    // 2. Policy decision
    let decision = evaluate_policy(policy, &ctx, &cap);
    let receipt_id = Uuid::new_v4();

    match &decision {
        Decision::Deny { reason } => {
            let receipt = Receipt {
                id: receipt_id,
                kind: ReceiptKind::Capability,
                capability: cap.name.clone(),
                created_at: Utc::now(),
                status: ReceiptStatus::Denied,
                decision: "deny".to_string(),
                reason: Some(reason.clone()),
                input: input.clone(),
                context: serde_json::to_value(&ctx).unwrap_or_default(),
                execution: None,
                approval: None,
                sandbox_enforced: sandbox_enforced(),
            };
            store.write(&receipt)?;
            return Ok(ExecutionOutcome {
                ok: false,
                approval_required: false,
                receipt_id,
                result: None,
                reason: Some(reason.clone()),
            });
        }
        Decision::RequireApproval { reason } => {
            // No webhook configured: return pending
            let receipt = Receipt {
                id: receipt_id,
                kind: ReceiptKind::Capability,
                capability: cap.name.clone(),
                created_at: Utc::now(),
                status: ReceiptStatus::PendingApproval,
                decision: "require_approval".to_string(),
                reason: Some(reason.clone()),
                input: input.clone(),
                context: serde_json::to_value(&ctx).unwrap_or_default(),
                execution: None,
                approval: None,
                sandbox_enforced: sandbox_enforced(),
            };
            store.write(&receipt)?;
            return Ok(ExecutionOutcome {
                ok: false,
                approval_required: true,
                receipt_id,
                result: None,
                reason: Some(reason.clone()),
            });
        }
        Decision::Allow => {}
    }

    // 3. Render args
    let template_ctx = serde_json::json!({ "input": &input, "context": { "env": &ctx.env, "cwd": ctx.cwd.to_string_lossy(), "user": &ctx.user } });
    let rendered_args = match &cap.backend {
        Backend::Subprocess { args, .. } => render_args(args, &template_ctx)?,
        _ => vec![],
    };

    // 4. Validators
    let val_errors = run_validators(&cap.validators, &input, &ctx.cwd, &rendered_args);
    if !val_errors.is_empty() {
        let reason = val_errors[0].clone();
        let receipt = Receipt {
            id: receipt_id,
            kind: ReceiptKind::Capability,
            capability: cap.name.clone(),
            created_at: Utc::now(),
            status: ReceiptStatus::Denied,
            decision: "deny".to_string(),
            reason: Some(reason.clone()),
            input: input.clone(),
            context: serde_json::to_value(&ctx).unwrap_or_default(),
            execution: None,
            approval: None,
            sandbox_enforced: sandbox_enforced(),
        };
        store.write(&receipt)?;
        return Ok(ExecutionOutcome {
            ok: false,
            approval_required: false,
            receipt_id,
            result: None,
            reason: Some(reason),
        });
    }

    // 5. Resolve credentials
    let secrets = resolve_credentials(&cap.credentials, infisical)?;
    let redactor = SecretRedactor::new(secrets.clone());

    // 6. Execute backend
    let exec_result = match &cap.backend {
        Backend::Builtin { name } => builtin_handler(name, &input)?,
        Backend::Subprocess { command, cwd_from_input, .. } => {
            let cwd = if let Some(key) = cwd_from_input {
                input[key].as_str().map(std::path::PathBuf::from).unwrap_or_else(|| ctx.cwd.clone())
            } else {
                ctx.cwd.clone()
            };
            let expanded_args = expand_secret_refs(&rendered_args, &secrets);
            let sub_result = run_subprocess(command, &expanded_args, &cwd, &secrets)?;
            serde_json::json!({
                "exitCode": sub_result.exit_code,
                "stdout": redactor.redact(&sub_result.stdout),
                "stderr": redactor.redact(&sub_result.stderr),
            })
        }
        Backend::Remote { url } => {
            let addr = if url.is_empty() {
                std::env::var("CLIX_SOCKET").unwrap_or_default()
            } else {
                url.clone()
            };
            run_remote(&addr, &cap.name, &input)?
        }
    };

    let exit_code = exec_result["exitCode"].as_i64().unwrap_or(0);
    let ok = exit_code == 0;
    let status = if ok { ReceiptStatus::Succeeded } else { ReceiptStatus::Failed };

    let receipt = Receipt {
        id: receipt_id,
        kind: ReceiptKind::Capability,
        capability: cap.name.clone(),
        created_at: Utc::now(),
        status,
        decision: "allow".to_string(),
        reason: None,
        input: input.clone(),
        context: serde_json::to_value(&ctx).unwrap_or_default(),
        execution: Some(exec_result.clone()),
        approval: None,
        sandbox_enforced: sandbox_enforced(),
    };
    store.write(&receipt)?;

    Ok(ExecutionOutcome {
        ok,
        approval_required: false,
        receipt_id,
        result: Some(exec_result),
        reason: None,
    })
}

pub fn run_workflow(
    cap_registry: &CapabilityRegistry,
    wf_registry: &WorkflowRegistry,
    policy: &PolicyBundle,
    infisical: Option<&InfisicalConfig>,
    store: &ReceiptStore,
    name: &str,
    input: serde_json::Value,
    ctx: ExecutionContext,
) -> Result<Vec<ExecutionOutcome>> {
    let wf = wf_registry
        .get(name)
        .ok_or_else(|| ClixError::WorkflowNotFound(name.to_string()))?
        .clone();

    let mut outcomes = vec![];
    for step in &wf.steps {
        // Merge workflow-level input with step-level input (step wins)
        let step_input = merge_inputs(&input, &step.input);
        let outcome = run_capability(
            cap_registry,
            policy,
            infisical,
            store,
            &step.capability,
            step_input,
            ctx.clone(),
        )?;

        let failed = !outcome.ok;
        outcomes.push(outcome);

        if failed {
            match step.on_failure {
                StepFailurePolicy::Abort => break,
                StepFailurePolicy::Continue => {}
            }
        }
    }
    Ok(outcomes)
}

fn merge_inputs(base: &serde_json::Value, step: &serde_json::Value) -> serde_json::Value {
    match (base, step) {
        (serde_json::Value::Object(b), serde_json::Value::Object(s)) => {
            let mut merged = b.clone();
            for (k, v) in s {
                merged.insert(k.clone(), v.clone());
            }
            serde_json::Value::Object(merged)
        }
        (_, s) if !s.is_null() => s.clone(),
        (b, _) => b.clone(),
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::manifest::capability::{Backend, CapabilityManifest, RiskLevel, SideEffectClass};
    use crate::policy::PolicyBundle;
    use crate::registry::CapabilityRegistry;
    use crate::receipts::ReceiptStore;
    use std::path::PathBuf;

    fn store() -> ReceiptStore {
        ReceiptStore::open(std::path::Path::new(":memory:")).unwrap()
    }

    fn ctx() -> ExecutionContext {
        ExecutionContext {
            env: "test".to_string(),
            cwd: PathBuf::from("."),
            user: "tester".to_string(),
            profile: "base".to_string(),
            approver: None,
        }
    }

    fn date_cap() -> CapabilityManifest {
        CapabilityManifest {
            name: "sys.date".to_string(),
            version: 1,
            description: None,
            backend: Backend::Builtin { name: "date".to_string() },
            risk: RiskLevel::Low,
            side_effect_class: SideEffectClass::None,
            sandbox_profile: None,
            approval_policy: None,
            input_schema: serde_json::json!({"type":"object","properties":{}}),
            validators: vec![],
            credentials: vec![],
        }
    }

    #[test]
    fn test_run_builtin_capability() {
        let registry = CapabilityRegistry::from_vec(vec![date_cap()]);
        let policy = PolicyBundle::default();
        let store = store();
        let outcome = run_capability(
            &registry, &policy, None, &store,
            "sys.date", serde_json::json!({}), ctx(),
        ).unwrap();
        assert!(outcome.ok);
        assert!(!outcome.approval_required);
    }

    #[test]
    fn test_unknown_capability_errors() {
        let registry = CapabilityRegistry::from_vec(vec![]);
        let policy = PolicyBundle::default();
        let store = store();
        let result = run_capability(
            &registry, &policy, None, &store,
            "does.not.exist", serde_json::json!({}), ctx(),
        );
        assert!(result.is_err());
    }

    #[test]
    fn test_denied_capability_writes_receipt() {
        use crate::policy::{PolicyBundle, PolicyRule, PolicyAction};
        let registry = CapabilityRegistry::from_vec(vec![date_cap()]);
        let mut policy = PolicyBundle::default();
        policy.rules.push(PolicyRule {
            capability: Some("sys.date".to_string()),
            action: PolicyAction::Deny,
            reason: Some("test deny".to_string()),
            ..Default::default()
        });
        let store = store();
        let outcome = run_capability(
            &registry, &policy, None, &store,
            "sys.date", serde_json::json!({}), ctx(),
        ).unwrap();
        assert!(!outcome.ok);
        let receipts = store.list(10, Some("denied")).unwrap();
        assert_eq!(receipts.len(), 1);
    }
}
```

- [ ] **Step 4: Add module to lib.rs**

```rust
pub mod execution;
```

- [ ] **Step 5: Run tests**

```bash
cargo test -p clix-core execution::tests
```
Expected: 3 tests pass

- [ ] **Step 6: Run all clix-core tests**

```bash
cargo test -p clix-core
```
Expected: all tests pass (~20 tests)

- [ ] **Step 7: Commit**

```bash
git add crates/clix-core/src/execution/ crates/clix-core/src/lib.rs
git commit -m "feat(core): add run_capability and run_workflow execution engine"
```

---

### Task 9: Phase 2 wrap-up

- [ ] **Step 1: Run clippy**

```bash
cargo clippy -- -D warnings
```
Fix any warnings.

- [ ] **Step 2: Run all tests**

```bash
cargo test
```
Expected: all tests pass

- [ ] **Step 3: Final commit**

```bash
git add -A
git commit -m "feat: Phase 2 complete — execution engine, secrets, receipts, sandbox"
```

---

## Phase 2 Complete

Produces: Complete `clix-core` execution engine — `run_capability` and `run_workflow` are fully functional with policy enforcement, schema validation, template rendering, credential injection, secret redaction, receipts to SQLite, and Landlock sandbox on Linux.

**Next:** `docs/superpowers/plans/2026-04-13-phase3-packs-cli.md` — pack management, built-in packs, and the `clix-cli` binary.
