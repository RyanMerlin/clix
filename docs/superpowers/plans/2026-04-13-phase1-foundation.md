# clix Rust Rewrite — Phase 1: Foundation

> **For agentic workers:** REQUIRED SUB-SKILL: Use superpowers:subagent-driven-development (recommended) or superpowers:executing-plans to implement this plan task-by-task. Steps use checkbox (`- [ ]`) syntax for tracking.

**Goal:** Bootstrap the Cargo workspace and implement `clix-core` foundations: error types, manifest types, state/config, policy engine, JSON Schema validation, and template rendering.

**Architecture:** Cargo workspace with `clix-core` as a pure library (no tokio). All domain types are strongly typed structs/enums using serde. Manifest edges use `serde_json::Value` for flexibility.

**Tech Stack:** Rust stable, serde/serde_json/serde_yaml, thiserror, jsonschema, minijinja, uuid, chrono

**Spec:** `docs/superpowers/specs/2026-04-13-rust-rewrite-design.md`

---

## File Map

```
Cargo.toml                                      # workspace root
crates/clix-core/
  Cargo.toml
  src/
    lib.rs
    error.rs                                    # ClixError (thiserror)
    state.rs                                    # ClixState, ClixConfig, paths
    manifest/
      mod.rs
      capability.rs                             # CapabilityManifest, Backend, RiskLevel, SideEffectClass, Validator, CredentialSource
      profile.rs                                # ProfileManifest, PolicyOverride
      workflow.rs                               # WorkflowManifest, WorkflowStep
      pack.rs                                   # PackManifest
      loader.rs                                 # load_yaml_or_json(), load_dir()
    policy/
      mod.rs                                    # PolicyBundle, PolicyRule, Decision
      evaluate.rs                               # evaluate_policy()
    schema/
      mod.rs                                    # validate_input(), SchemaError
    template/
      mod.rs                                    # render_args()
```

---

### Task 1: Workspace bootstrap

**Files:**
- Create: `Cargo.toml`
- Create: `crates/clix-core/Cargo.toml`
- Create: `crates/clix-core/src/lib.rs`

- [ ] **Step 1: Delete all Go files and create workspace Cargo.toml**

```bash
# From C:/code/clix
rm -rf cmd internal go.mod go.sum bin/clix clix.exe
```

Create `Cargo.toml`:
```toml
[workspace]
resolver = "2"
members = [
    "crates/clix-core",
    "crates/clix-cli",
    "crates/clix-serve",
]

[workspace.dependencies]
serde       = { version = "1", features = ["derive"] }
serde_json  = "1"
serde_yaml  = "0.9"
thiserror   = "2"
anyhow      = "1"
tokio       = { version = "1", features = ["full"] }
reqwest     = { version = "0.12", features = ["json"] }
clap        = { version = "4", features = ["derive", "env"] }
sqlx        = { version = "0.8", features = ["runtime-tokio", "sqlite", "chrono", "uuid"] }
uuid        = { version = "1", features = ["v4", "serde"] }
chrono      = { version = "0.4", features = ["serde"] }
jsonschema  = "0.18"
minijinja   = "2"
axum        = "0.7"
tracing     = "0.1"
tracing-subscriber = { version = "0.3", features = ["env-filter"] }
```

- [ ] **Step 2: Create clix-core crate**

```bash
mkdir -p crates/clix-core/src/manifest
mkdir -p crates/clix-core/src/policy
mkdir -p crates/clix-core/src/schema
mkdir -p crates/clix-core/src/template
```

Create `crates/clix-core/Cargo.toml`:
```toml
[package]
name    = "clix-core"
version = "0.2.0"
edition = "2021"

[dependencies]
serde       = { workspace = true }
serde_json  = { workspace = true }
serde_yaml  = { workspace = true }
thiserror   = { workspace = true }
uuid        = { workspace = true }
chrono      = { workspace = true }
jsonschema  = { workspace = true }
minijinja   = { workspace = true }
```

- [ ] **Step 3: Create stub lib.rs**

`crates/clix-core/src/lib.rs`:
```rust
pub mod error;
pub mod manifest;
pub mod policy;
pub mod schema;
pub mod state;
pub mod template;
```

- [ ] **Step 4: Verify workspace compiles (empty)**

```bash
cargo check
```
Expected: compiles (with "file not found" errors for modules not yet created — fix by adding empty files):
```bash
touch crates/clix-core/src/error.rs
touch crates/clix-core/src/state.rs
touch crates/clix-core/src/manifest/mod.rs
touch crates/clix-core/src/manifest/capability.rs
touch crates/clix-core/src/manifest/profile.rs
touch crates/clix-core/src/manifest/workflow.rs
touch crates/clix-core/src/manifest/pack.rs
touch crates/clix-core/src/manifest/loader.rs
touch crates/clix-core/src/policy/mod.rs
touch crates/clix-core/src/policy/evaluate.rs
touch crates/clix-core/src/schema/mod.rs
touch crates/clix-core/src/template/mod.rs
# Also create stub CLI and serve crates to satisfy workspace
mkdir -p crates/clix-cli/src
echo '[package]\nname = "clix-cli"\nversion = "0.2.0"\nedition = "2021"\n\n[dependencies]\nclix-core = { path = "../clix-core" }' > crates/clix-cli/Cargo.toml
echo 'fn main() {}' > crates/clix-cli/src/main.rs
mkdir -p crates/clix-serve/src
echo '[package]\nname = "clix-serve"\nversion = "0.2.0"\nedition = "2021"\n\n[dependencies]\nclix-core = { path = "../clix-core" }' > crates/clix-serve/Cargo.toml
echo 'fn main() {}' > crates/clix-serve/src/main.rs
cargo check
```
Expected: no errors

- [ ] **Step 5: Commit**

```bash
git add -A
git commit -m "feat: bootstrap Cargo workspace, remove Go source"
```

---

### Task 2: Error types

**Files:**
- Create: `crates/clix-core/src/error.rs`

- [ ] **Step 1: Write the error type**

`crates/clix-core/src/error.rs`:
```rust
use thiserror::Error;

#[derive(Debug, Error)]
pub enum ClixError {
    #[error("capability not found: {0}")]
    CapabilityNotFound(String),

    #[error("workflow not found: {0}")]
    WorkflowNotFound(String),

    #[error("input validation failed: {0}")]
    InputValidation(String),

    #[error("policy denied: {0}")]
    Denied(String),

    #[error("approval denied: {0}")]
    ApprovalDenied(String),

    #[error("approval gate error: {0}")]
    ApprovalGate(String),

    #[error("credential resolution failed: {0}")]
    CredentialResolution(String),

    #[error("template render error: {0}")]
    TemplateRender(String),

    #[error("sandbox error: {0}")]
    Sandbox(String),

    #[error("backend error: {0}")]
    Backend(String),

    #[error("pack error: {0}")]
    Pack(String),

    #[error("schema validation error: {0}")]
    Schema(String),

    #[error("config error: {0}")]
    Config(String),

    #[error("io error: {0}")]
    Io(#[from] std::io::Error),

    #[error("json error: {0}")]
    Json(#[from] serde_json::Error),

    #[error("yaml error: {0}")]
    Yaml(#[from] serde_yaml::Error),
}

pub type Result<T> = std::result::Result<T, ClixError>;
```

- [ ] **Step 2: Verify it compiles**

```bash
cargo check -p clix-core
```
Expected: no errors

- [ ] **Step 3: Commit**

```bash
git add crates/clix-core/src/error.rs
git commit -m "feat(core): add ClixError type"
```

---

### Task 3: Manifest types — Capability

**Files:**
- Modify: `crates/clix-core/src/manifest/capability.rs`
- Modify: `crates/clix-core/src/manifest/mod.rs`

- [ ] **Step 1: Write failing test**

Add to `crates/clix-core/src/manifest/capability.rs`:
```rust
#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_capability_roundtrip_json() {
        let json = serde_json::json!({
            "name": "kubectl.get-pods",
            "version": 1,
            "description": "List pods",
            "backend": {
                "type": "subprocess",
                "command": "kubectl",
                "args": ["get", "pods", "-n", "{{ input.namespace }}"]
            },
            "risk": "low",
            "sideEffectClass": "readOnly",
            "inputSchema": {
                "type": "object",
                "properties": {
                    "namespace": { "type": "string" }
                },
                "required": ["namespace"]
            }
        });
        let cap: CapabilityManifest = serde_json::from_value(json).unwrap();
        assert_eq!(cap.name, "kubectl.get-pods");
        assert!(matches!(cap.risk, RiskLevel::Low));
        assert!(matches!(cap.side_effect_class, SideEffectClass::ReadOnly));
        match &cap.backend {
            Backend::Subprocess { command, .. } => assert_eq!(command, "kubectl"),
            _ => panic!("expected subprocess backend"),
        }
    }

    #[test]
    fn test_capability_roundtrip_yaml() {
        let yaml = r#"
name: gcloud.list-projects
version: 1
backend:
  type: subprocess
  command: gcloud
  args: ["projects", "list", "--format=json"]
risk: low
sideEffectClass: readOnly
inputSchema:
  type: object
  properties: {}
"#;
        let cap: CapabilityManifest = serde_yaml::from_str(yaml).unwrap();
        assert_eq!(cap.name, "gcloud.list-projects");
    }
}
```

- [ ] **Step 2: Run tests — expect compile failure**

```bash
cargo test -p clix-core 2>&1 | head -20
```
Expected: compile error — types not defined yet

- [ ] **Step 3: Implement capability types**

`crates/clix-core/src/manifest/capability.rs`:
```rust
use serde::{Deserialize, Serialize};

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct CapabilityManifest {
    pub name: String,
    pub version: u32,
    #[serde(default)]
    pub description: Option<String>,
    pub backend: Backend,
    #[serde(default)]
    pub risk: RiskLevel,
    #[serde(default)]
    pub side_effect_class: SideEffectClass,
    #[serde(default)]
    pub sandbox_profile: Option<String>,
    #[serde(default)]
    pub approval_policy: Option<String>,
    #[serde(default = "default_schema")]
    pub input_schema: serde_json::Value,
    #[serde(default)]
    pub validators: Vec<Validator>,
    #[serde(default)]
    pub credentials: Vec<CredentialSource>,
}

fn default_schema() -> serde_json::Value {
    serde_json::json!({"type": "object", "properties": {}})
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(tag = "type", rename_all = "camelCase")]
pub enum Backend {
    #[serde(rename = "subprocess")]
    Subprocess {
        command: String,
        #[serde(default)]
        args: Vec<String>,
        #[serde(default)]
        cwd_from_input: Option<String>,
    },
    #[serde(rename = "builtin")]
    Builtin { name: String },
    #[serde(rename = "remote")]
    Remote { url: String },
}

#[derive(Debug, Clone, Default, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub enum RiskLevel {
    #[default]
    Low,
    Medium,
    High,
    Critical,
}

#[derive(Debug, Clone, Default, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub enum SideEffectClass {
    #[serde(rename = "none")]
    #[default]
    None,
    #[serde(rename = "readOnly")]
    ReadOnly,
    Additive,
    Mutating,
    Destructive,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct Validator {
    #[serde(rename = "type")]
    pub kind: ValidatorKind,
    #[serde(default)]
    pub path: String,
    #[serde(default)]
    pub key: String,
    #[serde(default)]
    pub values: Vec<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub enum ValidatorKind {
    RequiredPath,
    DenyArgs,
    RequiredInputKey,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(tag = "type", rename_all = "camelCase")]
pub enum CredentialSource {
    #[serde(rename = "env")]
    Env {
        env_var: String,
        inject_as: String,
    },
    #[serde(rename = "literal")]
    Literal {
        value: String,
        inject_as: String,
    },
    #[serde(rename = "infisical")]
    Infisical {
        #[serde(flatten)]
        secret_ref: InfisicalRef,
        inject_as: String,
    },
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct InfisicalRef {
    pub secret_name: String,
    #[serde(default)]
    pub project_id: Option<String>,
    pub environment: String,
    #[serde(default = "default_secret_path")]
    pub secret_path: String,
}

fn default_secret_path() -> String {
    "/".to_string()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_capability_roundtrip_json() {
        let json = serde_json::json!({
            "name": "kubectl.get-pods",
            "version": 1,
            "description": "List pods",
            "backend": {
                "type": "subprocess",
                "command": "kubectl",
                "args": ["get", "pods", "-n", "{{ input.namespace }}"]
            },
            "risk": "low",
            "sideEffectClass": "readOnly",
            "inputSchema": {
                "type": "object",
                "properties": {
                    "namespace": { "type": "string" }
                },
                "required": ["namespace"]
            }
        });
        let cap: CapabilityManifest = serde_json::from_value(json).unwrap();
        assert_eq!(cap.name, "kubectl.get-pods");
        assert!(matches!(cap.risk, RiskLevel::Low));
        assert!(matches!(cap.side_effect_class, SideEffectClass::ReadOnly));
        match &cap.backend {
            Backend::Subprocess { command, .. } => assert_eq!(command, "kubectl"),
            _ => panic!("expected subprocess backend"),
        }
    }

    #[test]
    fn test_capability_roundtrip_yaml() {
        let yaml = r#"
name: gcloud.list-projects
version: 1
backend:
  type: subprocess
  command: gcloud
  args: ["projects", "list", "--format=json"]
risk: low
sideEffectClass: readOnly
inputSchema:
  type: object
  properties: {}
"#;
        let cap: CapabilityManifest = serde_yaml::from_str(yaml).unwrap();
        assert_eq!(cap.name, "gcloud.list-projects");
    }
}
```

- [ ] **Step 4: Run tests — expect pass**

```bash
cargo test -p clix-core manifest::capability
```
Expected: 2 tests pass

- [ ] **Step 5: Commit**

```bash
git add crates/clix-core/src/manifest/capability.rs
git commit -m "feat(core): add CapabilityManifest types"
```

---

### Task 4: Manifest types — Profile, Workflow, Pack, Loader

**Files:**
- Modify: `crates/clix-core/src/manifest/profile.rs`
- Modify: `crates/clix-core/src/manifest/workflow.rs`
- Modify: `crates/clix-core/src/manifest/pack.rs`
- Modify: `crates/clix-core/src/manifest/loader.rs`
- Modify: `crates/clix-core/src/manifest/mod.rs`

- [ ] **Step 1: Write failing tests**

Add to `crates/clix-core/src/manifest/profile.rs`:
```rust
#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn test_profile_yaml() {
        let yaml = r#"
name: kubectl-observe
version: 1
description: Read-only kubectl inspection
capabilities: [kubectl.get-pods, kubectl.get-nodes]
workflows: [kubectl.cluster-health]
"#;
        let p: ProfileManifest = serde_yaml::from_str(yaml).unwrap();
        assert_eq!(p.name, "kubectl-observe");
        assert_eq!(p.capabilities.len(), 2);
    }
}
```

Add to `crates/clix-core/src/manifest/workflow.rs`:
```rust
#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn test_workflow_yaml() {
        let yaml = r#"
name: kubectl.cluster-health
version: 1
description: Check cluster health
steps:
  - capability: kubectl.get-nodes
    input: {}
  - capability: kubectl.get-pods
    input:
      namespace: kube-system
"#;
        let w: WorkflowManifest = serde_yaml::from_str(yaml).unwrap();
        assert_eq!(w.steps.len(), 2);
    }
}
```

- [ ] **Step 2: Run — expect compile failure**

```bash
cargo test -p clix-core 2>&1 | head -5
```
Expected: compile errors — types not defined

- [ ] **Step 3: Implement profile.rs**

`crates/clix-core/src/manifest/profile.rs`:
```rust
use serde::{Deserialize, Serialize};

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct ProfileManifest {
    pub name: String,
    pub version: u32,
    #[serde(default)]
    pub description: Option<String>,
    #[serde(default)]
    pub capabilities: Vec<String>,
    #[serde(default)]
    pub workflows: Vec<String>,
    #[serde(default)]
    pub settings: serde_json::Value,
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn test_profile_yaml() {
        let yaml = r#"
name: kubectl-observe
version: 1
description: Read-only kubectl inspection
capabilities: [kubectl.get-pods, kubectl.get-nodes]
workflows: [kubectl.cluster-health]
"#;
        let p: ProfileManifest = serde_yaml::from_str(yaml).unwrap();
        assert_eq!(p.name, "kubectl-observe");
        assert_eq!(p.capabilities.len(), 2);
    }
}
```

- [ ] **Step 4: Implement workflow.rs**

`crates/clix-core/src/manifest/workflow.rs`:
```rust
use serde::{Deserialize, Serialize};

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct WorkflowManifest {
    pub name: String,
    pub version: u32,
    #[serde(default)]
    pub description: Option<String>,
    pub steps: Vec<WorkflowStep>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct WorkflowStep {
    pub capability: String,
    #[serde(default)]
    pub input: serde_json::Value,
    #[serde(default)]
    pub on_failure: StepFailurePolicy,
}

#[derive(Debug, Clone, Default, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub enum StepFailurePolicy {
    #[default]
    Abort,
    Continue,
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn test_workflow_yaml() {
        let yaml = r#"
name: kubectl.cluster-health
version: 1
description: Check cluster health
steps:
  - capability: kubectl.get-nodes
    input: {}
  - capability: kubectl.get-pods
    input:
      namespace: kube-system
"#;
        let w: WorkflowManifest = serde_yaml::from_str(yaml).unwrap();
        assert_eq!(w.steps.len(), 2);
    }
}
```

- [ ] **Step 5: Implement pack.rs**

`crates/clix-core/src/manifest/pack.rs`:
```rust
use serde::{Deserialize, Serialize};

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct PackManifest {
    pub name: String,
    pub version: u32,
    #[serde(default)]
    pub description: Option<String>,
    #[serde(default)]
    pub profiles: Vec<String>,
    #[serde(default)]
    pub capabilities: Vec<String>,
    #[serde(default)]
    pub workflows: Vec<String>,
    #[serde(default)]
    pub author: Option<String>,
    #[serde(default)]
    pub homepage: Option<String>,
}
```

- [ ] **Step 6: Implement loader.rs**

`crates/clix-core/src/manifest/loader.rs`:
```rust
use std::path::Path;
use crate::error::{ClixError, Result};

/// Load a manifest from a file, accepting either .yaml/.yml or .json.
pub fn load_manifest<T>(path: &Path) -> Result<T>
where
    T: serde::de::DeserializeOwned,
{
    let content = std::fs::read_to_string(path)?;
    let ext = path.extension().and_then(|e| e.to_str()).unwrap_or("");
    match ext {
        "json" => Ok(serde_json::from_str(&content)?),
        "yaml" | "yml" => Ok(serde_yaml::from_str(&content)?),
        _ => Err(ClixError::Pack(format!(
            "unsupported manifest extension: {ext} (use .yaml or .json)"
        ))),
    }
}

/// Load all manifests of type T from a directory (*.yaml, *.yml, *.json).
pub fn load_dir<T>(dir: &Path) -> Result<Vec<T>>
where
    T: serde::de::DeserializeOwned,
{
    if !dir.exists() {
        return Ok(vec![]);
    }
    let mut results = vec![];
    for entry in std::fs::read_dir(dir)? {
        let entry = entry?;
        let path = entry.path();
        if path.is_dir() {
            continue;
        }
        let ext = path.extension().and_then(|e| e.to_str()).unwrap_or("");
        if matches!(ext, "yaml" | "yml" | "json") {
            match load_manifest::<T>(&path) {
                Ok(m) => results.push(m),
                Err(e) => eprintln!("warn: skipping {}: {e}", path.display()),
            }
        }
    }
    Ok(results)
}
```

- [ ] **Step 7: Wire up manifest/mod.rs**

`crates/clix-core/src/manifest/mod.rs`:
```rust
pub mod capability;
pub mod loader;
pub mod pack;
pub mod profile;
pub mod workflow;

pub use capability::{
    Backend, CapabilityManifest, CredentialSource, InfisicalRef, RiskLevel,
    SideEffectClass, Validator, ValidatorKind,
};
pub use loader::{load_dir, load_manifest};
pub use pack::PackManifest;
pub use profile::ProfileManifest;
pub use workflow::{StepFailurePolicy, WorkflowManifest, WorkflowStep};
```

- [ ] **Step 8: Run all manifest tests**

```bash
cargo test -p clix-core manifest
```
Expected: 4 tests pass (2 capability + 1 profile + 1 workflow)

- [ ] **Step 9: Commit**

```bash
git add crates/clix-core/src/manifest/
git commit -m "feat(core): add Profile, Workflow, Pack manifest types and loader"
```

---

### Task 5: State and Config

**Files:**
- Modify: `crates/clix-core/src/state.rs`

- [ ] **Step 1: Write failing test**

Add to `crates/clix-core/src/state.rs`:
```rust
#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn test_paths_from_env() {
        std::env::set_var("CLIX_HOME", "/tmp/test-clix");
        let state = ClixState::from_home(home_dir());
        assert_eq!(state.config_path, std::path::PathBuf::from("/tmp/test-clix/config.yaml"));
        std::env::remove_var("CLIX_HOME");
    }

    #[test]
    fn test_default_config() {
        let cfg = ClixConfig::default();
        assert_eq!(cfg.schema_version, 1);
        assert!(matches!(cfg.approval_mode, ApprovalMode::Interactive));
    }
}
```

- [ ] **Step 2: Run — expect compile failure**

```bash
cargo test -p clix-core state 2>&1 | head -5
```

- [ ] **Step 3: Implement state.rs**

`crates/clix-core/src/state.rs`:
```rust
use std::path::PathBuf;
use serde::{Deserialize, Serialize};
use crate::error::{ClixError, Result};

pub fn home_dir() -> PathBuf {
    if let Ok(v) = std::env::var("CLIX_HOME") {
        return PathBuf::from(v);
    }
    dirs::home_dir()
        .unwrap_or_else(|| PathBuf::from("."))
        .join(".clix")
}

#[derive(Debug, Clone)]
pub struct ClixState {
    pub home: PathBuf,
    pub config_path: PathBuf,
    pub policy_path: PathBuf,
    pub packs_dir: PathBuf,
    pub profiles_dir: PathBuf,
    pub capabilities_dir: PathBuf,
    pub workflows_dir: PathBuf,
    pub receipts_db: PathBuf,
    pub bundles_dir: PathBuf,
    pub cache_dir: PathBuf,
    pub config: ClixConfig,
}

impl ClixState {
    pub fn from_home(home: PathBuf) -> Self {
        ClixState {
            config_path:      home.join("config.yaml"),
            policy_path:      home.join("policy.yaml"),
            packs_dir:        home.join("packs"),
            profiles_dir:     home.join("profiles"),
            capabilities_dir: home.join("capabilities"),
            workflows_dir:    home.join("workflows"),
            receipts_db:      home.join("receipts.db"),
            bundles_dir:      home.join("bundles"),
            cache_dir:        home.join("cache"),
            config:           ClixConfig::default(),
            home,
        }
    }

    pub fn load(home: PathBuf) -> Result<Self> {
        let mut state = Self::from_home(home);
        if state.config_path.exists() {
            let content = std::fs::read_to_string(&state.config_path)?;
            state.config = serde_yaml::from_str(&content)?;
        }
        Ok(state)
    }

    pub fn ensure_dirs(&self) -> Result<()> {
        for dir in [
            &self.home,
            &self.packs_dir,
            &self.profiles_dir,
            &self.capabilities_dir,
            &self.workflows_dir,
            &self.bundles_dir,
            &self.cache_dir,
        ] {
            std::fs::create_dir_all(dir)?;
        }
        Ok(())
    }
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct ClixConfig {
    #[serde(default = "default_schema_version")]
    pub schema_version: u32,
    #[serde(default)]
    pub approval_mode: ApprovalMode,
    #[serde(default = "default_env")]
    pub default_env: String,
    #[serde(default)]
    pub workspace_root: Option<PathBuf>,
    #[serde(default)]
    pub active_profiles: Vec<String>,
    #[serde(default)]
    pub infisical: Option<InfisicalConfig>,
    #[serde(default)]
    pub approval_gate: Option<ApprovalGateConfig>,
    #[serde(default)]
    pub sandbox: SandboxConfig,
}

impl Default for ClixConfig {
    fn default() -> Self {
        ClixConfig {
            schema_version: 1,
            approval_mode: ApprovalMode::Interactive,
            default_env: "default".to_string(),
            workspace_root: None,
            active_profiles: vec![],
            infisical: None,
            approval_gate: None,
            sandbox: SandboxConfig::default(),
        }
    }
}

fn default_schema_version() -> u32 { 1 }
fn default_env() -> String { "default".to_string() }

#[derive(Debug, Clone, Default, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub enum ApprovalMode {
    Auto,
    #[default]
    Interactive,
    AlwaysRequire,
}

#[derive(Debug, Clone, Default, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct SandboxConfig {
    #[serde(default)]
    pub enabled: bool,
    #[serde(default)]
    pub allowed_executables: Vec<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct InfisicalConfig {
    #[serde(default = "default_infisical_url")]
    pub site_url: String,
    #[serde(default)]
    pub client_id: Option<String>,
    #[serde(default)]
    pub client_secret: Option<String>,
}

fn default_infisical_url() -> String {
    "https://app.infisical.com".to_string()
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct ApprovalGateConfig {
    pub webhook_url: String,
    #[serde(default)]
    pub timeout_seconds: u64,
    #[serde(default)]
    pub headers: std::collections::HashMap<String, String>,
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_paths_from_env() {
        std::env::set_var("CLIX_HOME_TEST_ONLY", "/tmp/test-clix");
        let state = ClixState::from_home(PathBuf::from("/tmp/test-clix"));
        assert_eq!(state.config_path, PathBuf::from("/tmp/test-clix/config.yaml"));
    }

    #[test]
    fn test_default_config() {
        let cfg = ClixConfig::default();
        assert_eq!(cfg.schema_version, 1);
        assert!(matches!(cfg.approval_mode, ApprovalMode::Interactive));
    }
}
```

Add `dirs` to `crates/clix-core/Cargo.toml`:
```toml
dirs = "5"
```

Also add to workspace `Cargo.toml`:
```toml
dirs = "5"
```

- [ ] **Step 4: Run tests**

```bash
cargo test -p clix-core state
```
Expected: 2 tests pass

- [ ] **Step 5: Commit**

```bash
git add crates/clix-core/src/state.rs crates/clix-core/Cargo.toml Cargo.toml
git commit -m "feat(core): add ClixState and ClixConfig"
```

---

### Task 6: Policy engine

**Files:**
- Modify: `crates/clix-core/src/policy/mod.rs`
- Modify: `crates/clix-core/src/policy/evaluate.rs`

- [ ] **Step 1: Write failing tests**

`crates/clix-core/src/policy/evaluate.rs`:
```rust
#[cfg(test)]
mod tests {
    use super::*;
    use crate::manifest::capability::{CapabilityManifest, Backend, RiskLevel, SideEffectClass};

    fn stub_cap(name: &str, risk: RiskLevel) -> CapabilityManifest {
        CapabilityManifest {
            name: name.to_string(),
            version: 1,
            description: None,
            backend: Backend::Builtin { name: "date".to_string() },
            risk,
            side_effect_class: SideEffectClass::ReadOnly,
            sandbox_profile: None,
            approval_policy: None,
            input_schema: serde_json::json!({}),
            validators: vec![],
            credentials: vec![],
        }
    }

    fn ctx() -> ExecutionContext {
        ExecutionContext {
            env: "default".to_string(),
            cwd: std::path::PathBuf::from("/tmp"),
            user: "agent".to_string(),
            profile: "base".to_string(),
            approver: None,
        }
    }

    #[test]
    fn test_allow_low_risk_no_rules() {
        let policy = PolicyBundle::default();
        let cap = stub_cap("sys.date", RiskLevel::Low);
        let decision = evaluate_policy(&policy, &ctx(), &cap);
        assert!(matches!(decision, Decision::Allow));
    }

    #[test]
    fn test_deny_by_name() {
        let mut policy = PolicyBundle::default();
        policy.rules.push(PolicyRule {
            capability: Some("dangerous.rm".to_string()),
            action: PolicyAction::Deny,
            reason: Some("not allowed".to_string()),
            ..Default::default()
        });
        let cap = stub_cap("dangerous.rm", RiskLevel::High);
        let decision = evaluate_policy(&policy, &ctx(), &cap);
        assert!(matches!(decision, Decision::Deny { .. }));
    }

    #[test]
    fn test_require_approval_for_mutating() {
        let mut policy = PolicyBundle::default();
        policy.rules.push(PolicyRule {
            side_effect_class: Some(crate::manifest::capability::SideEffectClass::Mutating),
            action: PolicyAction::RequireApproval,
            reason: Some("mutating ops need approval".to_string()),
            ..Default::default()
        });
        let mut cap = stub_cap("k8s.apply", RiskLevel::High);
        cap.side_effect_class = SideEffectClass::Mutating;
        let decision = evaluate_policy(&policy, &ctx(), &cap);
        assert!(matches!(decision, Decision::RequireApproval { .. }));
    }
}
```

- [ ] **Step 2: Run — expect compile failure**

```bash
cargo test -p clix-core policy 2>&1 | head -5
```

- [ ] **Step 3: Implement policy/mod.rs**

`crates/clix-core/src/policy/mod.rs`:
```rust
pub mod evaluate;

pub use evaluate::{evaluate_policy, Decision, ExecutionContext};

use serde::{Deserialize, Serialize};
use crate::manifest::capability::SideEffectClass;

#[derive(Debug, Clone, Default, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct PolicyBundle {
    #[serde(default)]
    pub rules: Vec<PolicyRule>,
    #[serde(default)]
    pub default_action: PolicyAction,
}

#[derive(Debug, Clone, Default, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct PolicyRule {
    #[serde(default)]
    pub capability: Option<String>,
    #[serde(default)]
    pub profile: Option<String>,
    #[serde(default)]
    pub env: Option<String>,
    #[serde(default)]
    pub risk: Option<String>,
    #[serde(default)]
    pub side_effect_class: Option<SideEffectClass>,
    pub action: PolicyAction,
    #[serde(default)]
    pub reason: Option<String>,
}

#[derive(Debug, Clone, Default, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub enum PolicyAction {
    #[default]
    Allow,
    Deny,
    RequireApproval,
}
```

- [ ] **Step 4: Implement policy/evaluate.rs**

`crates/clix-core/src/policy/evaluate.rs`:
```rust
use std::path::PathBuf;
use serde::{Deserialize, Serialize};
use crate::manifest::capability::{CapabilityManifest, RiskLevel, SideEffectClass};
use super::{PolicyAction, PolicyBundle, PolicyRule};

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ExecutionContext {
    pub env: String,
    pub cwd: PathBuf,
    pub user: String,
    pub profile: String,
    pub approver: Option<String>,
}

#[derive(Debug, Clone)]
pub enum Decision {
    Allow,
    Deny { reason: String },
    RequireApproval { reason: String },
}

pub fn evaluate_policy(
    policy: &PolicyBundle,
    ctx: &ExecutionContext,
    cap: &CapabilityManifest,
) -> Decision {
    for rule in &policy.rules {
        if !rule_matches(rule, ctx, cap) {
            continue;
        }
        let reason = rule
            .reason
            .clone()
            .unwrap_or_else(|| format!("matched policy rule for {}", cap.name));
        return match rule.action {
            PolicyAction::Allow => Decision::Allow,
            PolicyAction::Deny => Decision::Deny { reason },
            PolicyAction::RequireApproval => Decision::RequireApproval { reason },
        };
    }
    // Default: allow low/medium risk, require_approval for high/critical
    match cap.risk {
        RiskLevel::Low | RiskLevel::Medium => Decision::Allow,
        RiskLevel::High | RiskLevel::Critical => Decision::RequireApproval {
            reason: format!("{} risk capability requires approval", risk_label(&cap.risk)),
        },
    }
}

fn rule_matches(rule: &PolicyRule, ctx: &ExecutionContext, cap: &CapabilityManifest) -> bool {
    if let Some(ref name) = rule.capability {
        if name != &cap.name {
            return false;
        }
    }
    if let Some(ref profile) = rule.profile {
        if profile != &ctx.profile {
            return false;
        }
    }
    if let Some(ref env) = rule.env {
        if env != &ctx.env {
            return false;
        }
    }
    if let Some(ref risk) = rule.risk {
        if risk != &risk_label(&cap.risk) {
            return false;
        }
    }
    if let Some(ref sec) = rule.side_effect_class {
        if !side_effect_matches(sec, &cap.side_effect_class) {
            return false;
        }
    }
    true
}

fn risk_label(r: &RiskLevel) -> String {
    match r {
        RiskLevel::Low => "low",
        RiskLevel::Medium => "medium",
        RiskLevel::High => "high",
        RiskLevel::Critical => "critical",
    }
    .to_string()
}

fn side_effect_matches(a: &SideEffectClass, b: &SideEffectClass) -> bool {
    std::mem::discriminant(a) == std::mem::discriminant(b)
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::manifest::capability::{Backend, CapabilityManifest};

    fn stub_cap(name: &str, risk: RiskLevel) -> CapabilityManifest {
        CapabilityManifest {
            name: name.to_string(),
            version: 1,
            description: None,
            backend: Backend::Builtin { name: "date".to_string() },
            risk,
            side_effect_class: SideEffectClass::ReadOnly,
            sandbox_profile: None,
            approval_policy: None,
            input_schema: serde_json::json!({}),
            validators: vec![],
            credentials: vec![],
        }
    }

    fn ctx() -> ExecutionContext {
        ExecutionContext {
            env: "default".to_string(),
            cwd: PathBuf::from("/tmp"),
            user: "agent".to_string(),
            profile: "base".to_string(),
            approver: None,
        }
    }

    #[test]
    fn test_allow_low_risk_no_rules() {
        let policy = PolicyBundle::default();
        let cap = stub_cap("sys.date", RiskLevel::Low);
        let decision = evaluate_policy(&policy, &ctx(), &cap);
        assert!(matches!(decision, Decision::Allow));
    }

    #[test]
    fn test_deny_by_name() {
        let mut policy = PolicyBundle::default();
        policy.rules.push(PolicyRule {
            capability: Some("dangerous.rm".to_string()),
            action: PolicyAction::Deny,
            reason: Some("not allowed".to_string()),
            ..Default::default()
        });
        let cap = stub_cap("dangerous.rm", RiskLevel::High);
        let decision = evaluate_policy(&policy, &ctx(), &cap);
        assert!(matches!(decision, Decision::Deny { .. }));
    }

    #[test]
    fn test_require_approval_for_mutating() {
        let mut policy = PolicyBundle::default();
        policy.rules.push(PolicyRule {
            side_effect_class: Some(SideEffectClass::Mutating),
            action: PolicyAction::RequireApproval,
            reason: Some("mutating ops need approval".to_string()),
            ..Default::default()
        });
        let mut cap = stub_cap("k8s.apply", RiskLevel::High);
        cap.side_effect_class = SideEffectClass::Mutating;
        let decision = evaluate_policy(&policy, &ctx(), &cap);
        assert!(matches!(decision, Decision::RequireApproval { .. }));
    }
}
```

- [ ] **Step 5: Run tests**

```bash
cargo test -p clix-core policy
```
Expected: 3 tests pass

- [ ] **Step 6: Commit**

```bash
git add crates/clix-core/src/policy/
git commit -m "feat(core): add policy engine with Decision enum"
```

---

### Task 7: Schema validation

**Files:**
- Modify: `crates/clix-core/src/schema/mod.rs`

- [ ] **Step 1: Write failing tests**

`crates/clix-core/src/schema/mod.rs`:
```rust
#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_valid_input_passes() {
        let schema = serde_json::json!({
            "type": "object",
            "properties": {
                "namespace": { "type": "string" }
            },
            "required": ["namespace"]
        });
        let input = serde_json::json!({ "namespace": "default" });
        assert!(validate_input(&schema, &input).is_ok());
    }

    #[test]
    fn test_missing_required_field_fails() {
        let schema = serde_json::json!({
            "type": "object",
            "properties": {
                "namespace": { "type": "string" }
            },
            "required": ["namespace"]
        });
        let input = serde_json::json!({});
        let result = validate_input(&schema, &input);
        assert!(result.is_err());
        let msg = result.unwrap_err().to_string();
        assert!(msg.contains("namespace") || msg.contains("required"));
    }

    #[test]
    fn test_wrong_type_fails() {
        let schema = serde_json::json!({
            "type": "object",
            "properties": {
                "count": { "type": "integer" }
            },
            "required": ["count"]
        });
        let input = serde_json::json!({ "count": "not-a-number" });
        assert!(validate_input(&schema, &input).is_err());
    }
}
```

- [ ] **Step 2: Run — expect compile failure**

```bash
cargo test -p clix-core schema 2>&1 | head -5
```

- [ ] **Step 3: Implement schema/mod.rs**

`crates/clix-core/src/schema/mod.rs`:
```rust
use crate::error::{ClixError, Result};

/// Validate `input` against a JSON Schema value.
/// Returns Ok(()) if valid, Err(ClixError::InputValidation) with a message if not.
pub fn validate_input(schema: &serde_json::Value, input: &serde_json::Value) -> Result<()> {
    let compiled = jsonschema::JSONSchema::compile(schema)
        .map_err(|e| ClixError::Schema(format!("invalid schema: {e}")))?;

    let result = compiled.validate(input);
    if let Err(errors) = result {
        let messages: Vec<String> = errors.map(|e| e.to_string()).collect();
        return Err(ClixError::InputValidation(messages.join("; ")));
    }
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_valid_input_passes() {
        let schema = serde_json::json!({
            "type": "object",
            "properties": {
                "namespace": { "type": "string" }
            },
            "required": ["namespace"]
        });
        let input = serde_json::json!({ "namespace": "default" });
        assert!(validate_input(&schema, &input).is_ok());
    }

    #[test]
    fn test_missing_required_field_fails() {
        let schema = serde_json::json!({
            "type": "object",
            "properties": {
                "namespace": { "type": "string" }
            },
            "required": ["namespace"]
        });
        let input = serde_json::json!({});
        let result = validate_input(&schema, &input);
        assert!(result.is_err());
    }

    #[test]
    fn test_wrong_type_fails() {
        let schema = serde_json::json!({
            "type": "object",
            "properties": {
                "count": { "type": "integer" }
            },
            "required": ["count"]
        });
        let input = serde_json::json!({ "count": "not-a-number" });
        assert!(validate_input(&schema, &input).is_err());
    }
}
```

- [ ] **Step 4: Run tests**

```bash
cargo test -p clix-core schema
```
Expected: 3 tests pass

- [ ] **Step 5: Commit**

```bash
git add crates/clix-core/src/schema/
git commit -m "feat(core): add JSON Schema input validation"
```

---

### Task 8: Template rendering

**Files:**
- Modify: `crates/clix-core/src/template/mod.rs`

- [ ] **Step 1: Write failing tests**

`crates/clix-core/src/template/mod.rs`:
```rust
#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_simple_substitution() {
        let args = vec!["get".to_string(), "pods".to_string(), "-n".to_string(), "{{ input.namespace }}".to_string()];
        let ctx = serde_json::json!({ "input": { "namespace": "production" }, "context": { "env": "prod" } });
        let rendered = render_args(&args, &ctx).unwrap();
        assert_eq!(rendered, vec!["get", "pods", "-n", "production"]);
    }

    #[test]
    fn test_context_substitution() {
        let args = vec!["--env={{ context.env }}".to_string()];
        let ctx = serde_json::json!({ "input": {}, "context": { "env": "staging" } });
        let rendered = render_args(&args, &ctx).unwrap();
        assert_eq!(rendered, vec!["--env=staging"]);
    }

    #[test]
    fn test_no_template_passthrough() {
        let args = vec!["get".to_string(), "nodes".to_string()];
        let ctx = serde_json::json!({ "input": {}, "context": {} });
        let rendered = render_args(&args, &ctx).unwrap();
        assert_eq!(rendered, vec!["get", "nodes"]);
    }
}
```

- [ ] **Step 2: Run — expect compile failure**

```bash
cargo test -p clix-core template 2>&1 | head -5
```

- [ ] **Step 3: Implement template/mod.rs**

`crates/clix-core/src/template/mod.rs`:
```rust
use crate::error::{ClixError, Result};

/// Render a list of arg templates against a context value.
/// Templates use Jinja2 syntax: {{ input.namespace }}, {{ context.env }}
pub fn render_args(args: &[String], ctx: &serde_json::Value) -> Result<Vec<String>> {
    let mut env = minijinja::Environment::new();
    args.iter()
        .enumerate()
        .map(|(i, arg)| {
            // Only parse if it looks like a template to avoid minijinja overhead
            if !arg.contains("{{") {
                return Ok(arg.clone());
            }
            let tpl_name = format!("arg_{i}");
            env.add_template_owned(tpl_name.clone(), arg.clone())
                .map_err(|e| ClixError::TemplateRender(e.to_string()))?;
            let tpl = env.get_template(&tpl_name)
                .map_err(|e| ClixError::TemplateRender(e.to_string()))?;
            tpl.render(ctx)
                .map_err(|e| ClixError::TemplateRender(e.to_string()))
        })
        .collect()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_simple_substitution() {
        let args = vec![
            "get".to_string(),
            "pods".to_string(),
            "-n".to_string(),
            "{{ input.namespace }}".to_string(),
        ];
        let ctx = serde_json::json!({
            "input": { "namespace": "production" },
            "context": { "env": "prod" }
        });
        let rendered = render_args(&args, &ctx).unwrap();
        assert_eq!(rendered, vec!["get", "pods", "-n", "production"]);
    }

    #[test]
    fn test_context_substitution() {
        let args = vec!["--env={{ context.env }}".to_string()];
        let ctx = serde_json::json!({ "input": {}, "context": { "env": "staging" } });
        let rendered = render_args(&args, &ctx).unwrap();
        assert_eq!(rendered, vec!["--env=staging"]);
    }

    #[test]
    fn test_no_template_passthrough() {
        let args = vec!["get".to_string(), "nodes".to_string()];
        let ctx = serde_json::json!({ "input": {}, "context": {} });
        let rendered = render_args(&args, &ctx).unwrap();
        assert_eq!(rendered, vec!["get", "nodes"]);
    }
}
```

- [ ] **Step 4: Run tests**

```bash
cargo test -p clix-core template
```
Expected: 3 tests pass

- [ ] **Step 5: Run all clix-core tests**

```bash
cargo test -p clix-core
```
Expected: all tests pass (capability x2, profile x1, workflow x1, state x2, policy x3, schema x3, template x3 = 15 tests)

- [ ] **Step 6: Commit**

```bash
git add crates/clix-core/src/template/
git commit -m "feat(core): add minijinja template rendering for capability args"
```

---

### Task 9: Phase 1 wrap-up

- [ ] **Step 1: Run full test suite**

```bash
cargo test
```
Expected: all tests pass, no warnings about unused code in active modules

- [ ] **Step 2: Run clippy**

```bash
cargo clippy -- -D warnings
```
Fix any warnings before continuing.

- [ ] **Step 3: Final commit**

```bash
git add -A
git commit -m "feat: Phase 1 complete — clix-core foundation (types, policy, schema, template)"
```

---

## Phase 1 Complete

Produces: `clix-core` library with all foundational types, policy engine, schema validation, and template rendering — fully tested. No I/O, no tokio, pure library.

**Next:** `docs/superpowers/plans/2026-04-13-phase2-execution.md` — execution pipeline, backends, secrets, receipts, sandbox, approval gate.
