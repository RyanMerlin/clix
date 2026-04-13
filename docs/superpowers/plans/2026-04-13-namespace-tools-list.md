# Namespace-Aware tools/list + gcloud-aiplatform Pack

> **For agentic workers:** REQUIRED SUB-SKILL: Use superpowers:subagent-driven-development (recommended) or superpowers:executing-plans to implement this plan task-by-task. Steps use checkbox (`- [ ]`) syntax for tracking.

**Goal:** Implement hierarchical `tools/list` (namespace stubs by default, drill-in by param) and ship the `gcloud-aiplatform` pack as the first real test of it.

**Architecture:** `CapabilityRegistry` gains `namespaces()` + `by_namespace()`. `tools/list` defaults to stub view grouped by first-two-dot-segments; `namespace` param drills in; `all: true` restores flat list. The `gcloud-aiplatform` pack adds 6 Vertex AI capabilities under `gcloud.aiplatform.*`. CLI `capabilities list` gets matching `--namespace` / `--all` flags.

**Tech Stack:** Rust, serde_json, clix-core registry, clix-serve MCP methods, clix-cli commands, YAML pack manifests.

---

## File Map

| File | Change |
|---|---|
| `crates/clix-core/src/registry/mod.rs` | Add `namespaces()`, `by_namespace()`, `NamespaceStub` |
| `crates/clix-serve/src/methods/mcp.rs` | `tools_list` accepts `params`, returns stubs/drill/flat |
| `crates/clix-serve/src/dispatch.rs` | Pass `params` to `tools_list` |
| `crates/clix-cli/src/commands/capabilities.rs` | Add `--namespace`, `--all` flags |
| `packs/gcloud-aiplatform/pack.yaml` | New pack manifest |
| `packs/gcloud-aiplatform/capabilities/*.yaml` | 6 capability manifests |
| `packs/gcloud-aiplatform/profiles/gcloud-aiplatform.yaml` | Profile manifest |
| `docs/agent-tool-registry.md` | Architecture doc (already written) |

---

### Task 1: `CapabilityRegistry` — namespace grouping

**Files:**
- Modify: `crates/clix-core/src/registry/mod.rs`

- [ ] **Step 1: Write the failing tests**

Add to the bottom of `crates/clix-core/src/registry/mod.rs`, inside the existing `#[cfg(test)] mod tests` block:

```rust
    #[test]
    fn test_group_key_one_dot() {
        // "system.date" → "system", "gcloud.list-projects" → "gcloud"
        assert_eq!(CapabilityRegistry::group_key("system.date"), "system");
        assert_eq!(CapabilityRegistry::group_key("gcloud.list-projects"), "gcloud");
        assert_eq!(CapabilityRegistry::group_key("nodot"), "nodot");
    }

    #[test]
    fn test_group_key_two_plus_dots() {
        // "gcloud.aiplatform.models.list" → "gcloud.aiplatform"
        assert_eq!(CapabilityRegistry::group_key("gcloud.aiplatform.models.list"), "gcloud.aiplatform");
        assert_eq!(CapabilityRegistry::group_key("gcloud.aiplatform.endpoints.describe"), "gcloud.aiplatform");
        assert_eq!(CapabilityRegistry::group_key("a.b.c"), "a.b");
    }

    #[test]
    fn test_namespaces() {
        let reg = CapabilityRegistry::from_vec(vec![
            make_cap("system.date"),
            make_cap("system.echo"),
            make_cap("gcloud.aiplatform.models.list"),
            make_cap("gcloud.aiplatform.endpoints.list"),
        ]);
        let stubs = reg.namespaces();
        assert_eq!(stubs.len(), 2);
        let sys = stubs.iter().find(|s| s.key == "system").unwrap();
        assert_eq!(sys.count, 2);
        let gca = stubs.iter().find(|s| s.key == "gcloud.aiplatform").unwrap();
        assert_eq!(gca.count, 2);
    }

    #[test]
    fn test_by_namespace() {
        let reg = CapabilityRegistry::from_vec(vec![
            make_cap("gcloud.aiplatform.models.list"),
            make_cap("gcloud.aiplatform.endpoints.list"),
            make_cap("system.date"),
        ]);
        let matched = reg.by_namespace("gcloud.aiplatform");
        assert_eq!(matched.len(), 2);
        assert!(matched.iter().all(|c| c.name.starts_with("gcloud.aiplatform.")));

        // exact prefix only — "gcloud" should NOT match "gcloud.aiplatform.*"
        let gcloud_only = reg.by_namespace("gcloud");
        assert_eq!(gcloud_only.len(), 0);
    }
```

- [ ] **Step 2: Run to confirm they fail**

```sh
cargo test -p clix-core 2>&1 | grep -E "FAILED|error\[" | head -20
```

Expected: compile error — `group_key`, `namespaces`, `by_namespace`, `NamespaceStub` not defined.

- [ ] **Step 3: Implement in `crates/clix-core/src/registry/mod.rs`**

Replace the entire file with:

```rust
use std::collections::HashMap;
use crate::manifest::capability::CapabilityManifest;
use crate::manifest::workflow::WorkflowManifest;

#[derive(Debug, Clone)]
pub struct NamespaceStub {
    pub key: String,
    pub count: usize,
}

#[derive(Debug, Default, Clone)]
pub struct CapabilityRegistry {
    caps: HashMap<String, CapabilityManifest>,
}

impl CapabilityRegistry {
    pub fn from_vec(caps: Vec<CapabilityManifest>) -> Self {
        let mut reg = Self::default();
        for cap in caps { reg.caps.insert(cap.name.clone(), cap); }
        reg
    }

    pub fn get(&self, name: &str) -> Option<&CapabilityManifest> { self.caps.get(name) }

    pub fn all(&self) -> Vec<&CapabilityManifest> {
        let mut v: Vec<_> = self.caps.values().collect();
        v.sort_by(|a, b| a.name.cmp(&b.name));
        v
    }

    pub fn insert(&mut self, cap: CapabilityManifest) { self.caps.insert(cap.name.clone(), cap); }

    /// Returns the top-level namespace group key for a capability name.
    /// - 0 dots → the name itself
    /// - 1 dot  → everything before the dot  ("system.date" → "system")
    /// - 2+ dots → first two segments         ("gcloud.aiplatform.models.list" → "gcloud.aiplatform")
    pub fn group_key(name: &str) -> String {
        let dots: Vec<usize> = name.match_indices('.').map(|(i, _)| i).collect();
        match dots.len() {
            0 => name.to_string(),
            1 => name[..dots[0]].to_string(),
            _ => name[..dots[1]].to_string(),
        }
    }

    /// Returns namespace stubs grouped by `group_key`, sorted by key.
    pub fn namespaces(&self) -> Vec<NamespaceStub> {
        let mut counts: HashMap<String, usize> = HashMap::new();
        for name in self.caps.keys() {
            *counts.entry(Self::group_key(name)).or_insert(0) += 1;
        }
        let mut stubs: Vec<NamespaceStub> = counts
            .into_iter()
            .map(|(key, count)| NamespaceStub { key, count })
            .collect();
        stubs.sort_by(|a, b| a.key.cmp(&b.key));
        stubs
    }

    /// Returns all capabilities whose name starts with `{namespace}.` (exact prefix match).
    pub fn by_namespace(&self, namespace: &str) -> Vec<&CapabilityManifest> {
        let prefix = format!("{namespace}.");
        let mut v: Vec<_> = self.caps.values()
            .filter(|c| c.name.starts_with(&prefix))
            .collect();
        v.sort_by(|a, b| a.name.cmp(&b.name));
        v
    }
}

#[derive(Debug, Default, Clone)]
pub struct WorkflowRegistry {
    workflows: HashMap<String, WorkflowManifest>,
}

impl WorkflowRegistry {
    pub fn from_vec(workflows: Vec<WorkflowManifest>) -> Self {
        let mut reg = Self::default();
        for wf in workflows { reg.workflows.insert(wf.name.clone(), wf); }
        reg
    }
    pub fn get(&self, name: &str) -> Option<&WorkflowManifest> { self.workflows.get(name) }
    pub fn all(&self) -> Vec<&WorkflowManifest> {
        let mut v: Vec<_> = self.workflows.values().collect();
        v.sort_by(|a, b| a.name.cmp(&b.name));
        v
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::manifest::capability::{Backend, RiskLevel, SideEffectClass};

    fn make_cap(name: &str) -> CapabilityManifest {
        CapabilityManifest {
            name: name.to_string(), version: 1, description: None,
            backend: Backend::Builtin { name: "date".to_string() },
            risk: RiskLevel::Low, side_effect_class: SideEffectClass::None,
            sandbox_profile: None, approval_policy: None,
            input_schema: serde_json::json!({}), validators: vec![], credentials: vec![],
        }
    }

    #[test]
    fn test_registry_get() {
        let reg = CapabilityRegistry::from_vec(vec![make_cap("sys.date"), make_cap("sys.echo")]);
        assert!(reg.get("sys.date").is_some());
        assert!(reg.get("missing").is_none());
        assert_eq!(reg.all().len(), 2);
    }

    #[test]
    fn test_group_key_one_dot() {
        assert_eq!(CapabilityRegistry::group_key("system.date"), "system");
        assert_eq!(CapabilityRegistry::group_key("gcloud.list-projects"), "gcloud");
        assert_eq!(CapabilityRegistry::group_key("nodot"), "nodot");
    }

    #[test]
    fn test_group_key_two_plus_dots() {
        assert_eq!(CapabilityRegistry::group_key("gcloud.aiplatform.models.list"), "gcloud.aiplatform");
        assert_eq!(CapabilityRegistry::group_key("gcloud.aiplatform.endpoints.describe"), "gcloud.aiplatform");
        assert_eq!(CapabilityRegistry::group_key("a.b.c"), "a.b");
    }

    #[test]
    fn test_namespaces() {
        let reg = CapabilityRegistry::from_vec(vec![
            make_cap("system.date"),
            make_cap("system.echo"),
            make_cap("gcloud.aiplatform.models.list"),
            make_cap("gcloud.aiplatform.endpoints.list"),
        ]);
        let stubs = reg.namespaces();
        assert_eq!(stubs.len(), 2);
        let sys = stubs.iter().find(|s| s.key == "system").unwrap();
        assert_eq!(sys.count, 2);
        let gca = stubs.iter().find(|s| s.key == "gcloud.aiplatform").unwrap();
        assert_eq!(gca.count, 2);
    }

    #[test]
    fn test_by_namespace() {
        let reg = CapabilityRegistry::from_vec(vec![
            make_cap("gcloud.aiplatform.models.list"),
            make_cap("gcloud.aiplatform.endpoints.list"),
            make_cap("system.date"),
        ]);
        let matched = reg.by_namespace("gcloud.aiplatform");
        assert_eq!(matched.len(), 2);
        assert!(matched.iter().all(|c| c.name.starts_with("gcloud.aiplatform.")));

        let gcloud_only = reg.by_namespace("gcloud");
        assert_eq!(gcloud_only.len(), 0);
    }
}
```

- [ ] **Step 4: Run tests — expect all pass**

```sh
cargo test -p clix-core 2>&1 | tail -10
```

Expected:
```
test result: ok. N passed; 0 failed
```

- [ ] **Step 5: Commit**

```sh
git add crates/clix-core/src/registry/mod.rs
git commit -m "feat(registry): add namespace grouping — group_key, namespaces(), by_namespace()"
```

---

### Task 2: `tools/list` — namespace-aware MCP method

**Files:**
- Modify: `crates/clix-serve/src/methods/mcp.rs`
- Modify: `crates/clix-serve/src/dispatch.rs`

- [ ] **Step 1: Write the failing tests**

Add to `crates/clix-serve/src/dispatch.rs`, inside the existing `#[cfg(test)] mod tests` block after `test_tools_list_empty`:

```rust
    #[tokio::test]
    async fn test_tools_list_stub_view() {
        // Two caps with the same group key → one stub returned
        let mut reg = CapabilityRegistry::from_vec(vec![]);
        use clix_core::manifest::capability::{Backend, RiskLevel, SideEffectClass};
        let make = |name: &str| clix_core::manifest::capability::CapabilityManifest {
            name: name.to_string(), version: 1, description: Some(format!("{name} desc")),
            backend: Backend::Builtin { name: "date".to_string() },
            risk: RiskLevel::Low, side_effect_class: SideEffectClass::ReadOnly,
            sandbox_profile: None, approval_policy: None,
            input_schema: serde_json::json!({}), validators: vec![], credentials: vec![],
        };
        reg.insert(make("gcloud.aiplatform.models.list"));
        reg.insert(make("gcloud.aiplatform.endpoints.list"));
        reg.insert(make("system.date"));

        let home = std::env::temp_dir().join("clix-test-serve-ns");
        std::fs::create_dir_all(&home).unwrap();
        let s = Arc::new(ServeState {
            cap_registry: reg,
            wf_registry:  WorkflowRegistry::from_vec(vec![]),
            policy:       PolicyBundle::default(),
            store:        Mutex::new(ReceiptStore::open(&home.join("receipts.db")).unwrap()),
            state:        ClixState::from_home(home),
        });

        // Default (no params) → stub view
        let req = serde_json::json!({"jsonrpc":"2.0","id":10,"method":"tools/list","params":{}});
        let resp = dispatch(Arc::clone(&s), req).await;
        let tools = resp["result"]["tools"].as_array().unwrap();
        assert_eq!(tools.len(), 2, "expected 2 namespace stubs");
        let types: Vec<_> = tools.iter().map(|t| t["type"].as_str().unwrap_or("")).collect();
        assert!(types.iter().all(|t| *t == "namespace"));

        // namespace drill-in
        let req2 = serde_json::json!({"jsonrpc":"2.0","id":11,"method":"tools/list","params":{"namespace":"gcloud.aiplatform"}});
        let resp2 = dispatch(Arc::clone(&s), req2).await;
        let tools2 = resp2["result"]["tools"].as_array().unwrap();
        assert_eq!(tools2.len(), 2);
        assert!(tools2.iter().all(|t| t["type"].as_str() != Some("namespace")));

        // all:true → flat list
        let req3 = serde_json::json!({"jsonrpc":"2.0","id":12,"method":"tools/list","params":{"all":true}});
        let resp3 = dispatch(Arc::clone(&s), req3).await;
        let tools3 = resp3["result"]["tools"].as_array().unwrap();
        assert_eq!(tools3.len(), 3);
    }
```

- [ ] **Step 2: Run to confirm failure**

```sh
cargo test -p clix-serve test_tools_list_stub_view 2>&1 | tail -5
```

Expected: FAILED — `tools_list` doesn't accept params yet.

- [ ] **Step 3: Update `tools_list` in `crates/clix-serve/src/methods/mcp.rs`**

Replace `tools_list` (lines 22–29) with:

```rust
pub fn tools_list(serve: &Arc<ServeState>, params: &serde_json::Value) -> MethodResult {
    // "all: true" → flat list (backward compat)
    if params.get("all").and_then(|v| v.as_bool()).unwrap_or(false) {
        let tools: Vec<serde_json::Value> = serve.cap_registry.all().iter().map(|cap| serde_json::json!({
            "name": cap.name,
            "description": cap.description.as_deref().unwrap_or(""),
            "inputSchema": cap.input_schema
        })).collect();
        return Ok(serde_json::json!({"tools": tools}));
    }

    // "namespace: X" → drill-in: return full tool descriptors for that namespace
    if let Some(ns) = params.get("namespace").and_then(|v| v.as_str()) {
        let tools: Vec<serde_json::Value> = serve.cap_registry.by_namespace(ns).iter().map(|cap| serde_json::json!({
            "name": cap.name,
            "description": cap.description.as_deref().unwrap_or(""),
            "inputSchema": cap.input_schema
        })).collect();
        return Ok(serde_json::json!({"tools": tools}));
    }

    // Default → namespace stub view
    let tools: Vec<serde_json::Value> = serve.cap_registry.namespaces().iter().map(|stub| serde_json::json!({
        "name": stub.key,
        "type": "namespace",
        "count": stub.count
    })).collect();
    Ok(serde_json::json!({"tools": tools}))
}
```

Also update the `initialize` response (line 15) to advertise namespace support — change:

```rust
            "extensions": {"clix": {"workflows": true,"onboard": true}}
```

to:

```rust
            "extensions": {"clix": {"namespaces": true, "workflows": true,"onboard": true}}
```

- [ ] **Step 4: Update `dispatch.rs` to pass params to `tools_list`**

In `crates/clix-serve/src/dispatch.rs`, change line 23:

```rust
        "tools/list"     => crate::methods::mcp::tools_list(&serve),
```

to:

```rust
        "tools/list"     => crate::methods::mcp::tools_list(&serve, &params),
```

Also update the existing `test_tools_list_empty` test to still pass — it uses `{}` params which will return stubs (empty slice), which is fine. No change needed.

- [ ] **Step 5: Run tests**

```sh
cargo test -p clix-serve 2>&1 | tail -10
```

Expected: all pass including `test_tools_list_stub_view`.

- [ ] **Step 6: Commit**

```sh
git add crates/clix-serve/src/methods/mcp.rs crates/clix-serve/src/dispatch.rs
git commit -m "feat(serve): namespace-aware tools/list — stub view, drill-in, all flag"
```

---

### Task 3: CLI `capabilities list` — namespace flags

**Files:**
- Modify: `crates/clix-cli/src/commands/capabilities.rs`

- [ ] **Step 1: Read current file**

```sh
cat crates/clix-cli/src/commands/capabilities.rs
```

Current content (for reference):
```rust
use anyhow::Result;
use clix_core::loader::build_registry;
use clix_core::state::ClixState;
use crate::output::print_table;

pub fn run(json: bool) -> Result<()> {
    let state = ClixState::default();
    let reg = build_registry(&state)?;
    if json {
        println!("{}", serde_json::to_string_pretty(&serde_json::json!(
            reg.all().iter().map(|c| serde_json::json!({"name":c.name,"description":c.description})).collect::<Vec<_>>()
        ))?);
    } else {
        let rows: Vec<[String; 2]> = reg.all().iter()
            .map(|c| [c.name.clone(), c.description.as_deref().unwrap_or("").to_string()])
            .collect();
        print_table(&rows);
    }
    Ok(())
}
```

- [ ] **Step 2: Replace with namespace-aware version**

Write `crates/clix-cli/src/commands/capabilities.rs`:

```rust
use anyhow::Result;
use clix_core::loader::build_registry;
use clix_core::state::ClixState;
use crate::output::print_table;

pub fn run(json: bool, namespace: Option<&str>, all: bool) -> Result<()> {
    let state = ClixState::default();
    let reg = build_registry(&state)?;

    if let Some(ns) = namespace {
        // Drill into a specific namespace
        let caps = reg.by_namespace(ns);
        if json {
            println!("{}", serde_json::to_string_pretty(&serde_json::json!(
                caps.iter().map(|c| serde_json::json!({"name":c.name,"description":c.description})).collect::<Vec<_>>()
            ))?);
        } else {
            let rows: Vec<[String; 2]> = caps.iter()
                .map(|c| [c.name.clone(), c.description.as_deref().unwrap_or("").to_string()])
                .collect();
            print_table(&rows);
        }
        return Ok(());
    }

    if all {
        // Flat list — original behavior
        let caps = reg.all();
        if json {
            println!("{}", serde_json::to_string_pretty(&serde_json::json!(
                caps.iter().map(|c| serde_json::json!({"name":c.name,"description":c.description})).collect::<Vec<_>>()
            ))?);
        } else {
            let rows: Vec<[String; 2]> = caps.iter()
                .map(|c| [c.name.clone(), c.description.as_deref().unwrap_or("").to_string()])
                .collect();
            print_table(&rows);
        }
        return Ok(());
    }

    // Default: namespace stub view
    let stubs = reg.namespaces();
    if json {
        println!("{}", serde_json::to_string_pretty(&serde_json::json!(
            stubs.iter().map(|s| serde_json::json!({"namespace":s.key,"count":s.count})).collect::<Vec<_>>()
        ))?);
    } else {
        let rows: Vec<[String; 2]> = stubs.iter()
            .map(|s| [s.key.clone(), format!("{} capabilities", s.count)])
            .collect();
        print_table(&rows);
    }
    Ok(())
}
```

- [ ] **Step 3: Update `cli.rs` to wire the new flags**

In `crates/clix-cli/src/cli.rs`, find the `Capabilities` subcommand variant. It currently looks like:

```rust
Capabilities {
    #[command(subcommand)]
    command: CapabilitiesCommand,
},
```

The `CapabilitiesCommand::List` variant needs `--namespace` and `--all` flags. Find `CapabilitiesCommand` and update `List`:

```rust
#[derive(Subcommand)]
pub enum CapabilitiesCommand {
    List {
        #[arg(long)]
        json: bool,
        /// Drill into a namespace (e.g. "gcloud.aiplatform")
        #[arg(long)]
        namespace: Option<String>,
        /// Show all capabilities as a flat list
        #[arg(long)]
        all: bool,
    },
}
```

- [ ] **Step 4: Update `main.rs` dispatch**

In `crates/clix-cli/src/main.rs`, find the match arm for `CapabilitiesCommand::List` and update it:

```rust
CapabilitiesCommand::List { json, namespace, all } => {
    commands::capabilities::run(*json, namespace.as_deref(), *all)?;
}
```

- [ ] **Step 5: Build and smoke-test**

```sh
cargo build -p clix-cli 2>&1 | grep -E "^error" | head -10
```

Expected: no errors (warnings ok).

```sh
./target/debug/clix capabilities list
```

Expected: namespace stub table (e.g. `gcloud   1 capabilities`, `system   2 capabilities`).

```sh
./target/debug/clix capabilities list --all
```

Expected: flat list of all 7 capabilities.

```sh
./target/debug/clix capabilities list --namespace gcloud
```

Expected: just `gcloud.list-projects`.

- [ ] **Step 6: Commit**

```sh
git add crates/clix-cli/src/commands/capabilities.rs crates/clix-cli/src/cli.rs crates/clix-cli/src/main.rs
git commit -m "feat(cli): capabilities list --namespace / --all flags"
```

---

### Task 4: `gcloud-aiplatform` pack

**Files:**
- Create: `packs/gcloud-aiplatform/pack.yaml`
- Create: `packs/gcloud-aiplatform/profiles/gcloud-aiplatform.yaml`
- Create: `packs/gcloud-aiplatform/capabilities/gcloud.aiplatform.models.list.yaml`
- Create: `packs/gcloud-aiplatform/capabilities/gcloud.aiplatform.models.describe.yaml`
- Create: `packs/gcloud-aiplatform/capabilities/gcloud.aiplatform.endpoints.list.yaml`
- Create: `packs/gcloud-aiplatform/capabilities/gcloud.aiplatform.endpoints.describe.yaml`
- Create: `packs/gcloud-aiplatform/capabilities/gcloud.aiplatform.jobs.list.yaml`
- Create: `packs/gcloud-aiplatform/capabilities/gcloud.aiplatform.datasets.list.yaml`

- [ ] **Step 1: Create pack manifest**

`packs/gcloud-aiplatform/pack.yaml`:
```yaml
name: gcloud-aiplatform
version: 1
description: Vertex AI / AI Platform — models, endpoints, custom jobs, datasets
namespace: gcloud.aiplatform
author: clix
```

- [ ] **Step 2: Create profile**

`packs/gcloud-aiplatform/profiles/gcloud-aiplatform.yaml`:
```yaml
name: gcloud-aiplatform
version: 1
description: Read-only Vertex AI inspection
capabilities:
  - gcloud.aiplatform.models.list
  - gcloud.aiplatform.models.describe
  - gcloud.aiplatform.endpoints.list
  - gcloud.aiplatform.endpoints.describe
  - gcloud.aiplatform.jobs.list
  - gcloud.aiplatform.datasets.list
```

- [ ] **Step 3: Create the 6 capability manifests**

`packs/gcloud-aiplatform/capabilities/gcloud.aiplatform.models.list.yaml`:
```yaml
name: gcloud.aiplatform.models.list
version: 1
description: List Vertex AI models in a project and region
backend:
  type: subprocess
  command: gcloud
  args: ["ai", "models", "list", "--project={{ input.project }}", "--region={{ input.region }}", "--format=json"]
risk: low
sideEffectClass: readOnly
inputSchema:
  type: object
  required: [project, region]
  properties:
    project:
      type: string
      description: GCP project ID
    region:
      type: string
      description: "Region (e.g. us-central1)"
      default: us-central1
```

`packs/gcloud-aiplatform/capabilities/gcloud.aiplatform.models.describe.yaml`:
```yaml
name: gcloud.aiplatform.models.describe
version: 1
description: Describe a specific Vertex AI model
backend:
  type: subprocess
  command: gcloud
  args: ["ai", "models", "describe", "{{ input.model_id }}", "--project={{ input.project }}", "--region={{ input.region }}", "--format=json"]
risk: low
sideEffectClass: readOnly
inputSchema:
  type: object
  required: [project, region, model_id]
  properties:
    project:
      type: string
      description: GCP project ID
    region:
      type: string
      description: "Region (e.g. us-central1)"
      default: us-central1
    model_id:
      type: string
      description: Vertex AI model ID or resource name
```

`packs/gcloud-aiplatform/capabilities/gcloud.aiplatform.endpoints.list.yaml`:
```yaml
name: gcloud.aiplatform.endpoints.list
version: 1
description: List Vertex AI endpoints in a project and region
backend:
  type: subprocess
  command: gcloud
  args: ["ai", "endpoints", "list", "--project={{ input.project }}", "--region={{ input.region }}", "--format=json"]
risk: low
sideEffectClass: readOnly
inputSchema:
  type: object
  required: [project, region]
  properties:
    project:
      type: string
      description: GCP project ID
    region:
      type: string
      description: "Region (e.g. us-central1)"
      default: us-central1
```

`packs/gcloud-aiplatform/capabilities/gcloud.aiplatform.endpoints.describe.yaml`:
```yaml
name: gcloud.aiplatform.endpoints.describe
version: 1
description: Describe a specific Vertex AI endpoint
backend:
  type: subprocess
  command: gcloud
  args: ["ai", "endpoints", "describe", "{{ input.endpoint_id }}", "--project={{ input.project }}", "--region={{ input.region }}", "--format=json"]
risk: low
sideEffectClass: readOnly
inputSchema:
  type: object
  required: [project, region, endpoint_id]
  properties:
    project:
      type: string
      description: GCP project ID
    region:
      type: string
      description: "Region (e.g. us-central1)"
      default: us-central1
    endpoint_id:
      type: string
      description: Vertex AI endpoint ID or resource name
```

`packs/gcloud-aiplatform/capabilities/gcloud.aiplatform.jobs.list.yaml`:
```yaml
name: gcloud.aiplatform.jobs.list
version: 1
description: List Vertex AI custom training jobs
backend:
  type: subprocess
  command: gcloud
  args: ["ai", "custom-jobs", "list", "--project={{ input.project }}", "--region={{ input.region }}", "--format=json"]
risk: low
sideEffectClass: readOnly
inputSchema:
  type: object
  required: [project, region]
  properties:
    project:
      type: string
      description: GCP project ID
    region:
      type: string
      description: "Region (e.g. us-central1)"
      default: us-central1
```

`packs/gcloud-aiplatform/capabilities/gcloud.aiplatform.datasets.list.yaml`:
```yaml
name: gcloud.aiplatform.datasets.list
version: 1
description: List Vertex AI datasets in a project and region
backend:
  type: subprocess
  command: gcloud
  args: ["ai", "datasets", "list", "--project={{ input.project }}", "--region={{ input.region }}", "--format=json"]
risk: low
sideEffectClass: readOnly
inputSchema:
  type: object
  required: [project, region]
  properties:
    project:
      type: string
      description: GCP project ID
    region:
      type: string
      description: "Region (e.g. us-central1)"
      default: us-central1
```

- [ ] **Step 4: Install the pack and verify it loads**

```sh
./target/debug/clix pack install packs/gcloud-aiplatform
./target/debug/clix capabilities list
```

Expected stub view now shows `gcloud.aiplatform` with `6 capabilities` alongside `gcloud` and `system`.

```sh
./target/debug/clix capabilities list --namespace gcloud.aiplatform
```

Expected: all 6 `gcloud.aiplatform.*` capabilities listed.

- [ ] **Step 5: Run a live capability end-to-end**

```sh
./target/debug/clix run gcloud.aiplatform.models.list --input project=alteryxone-dev-7393 --input region=us-central1
```

Expected: `ok — receipt <uuid>` followed by JSON array (may be empty if no models in that project/region, but no error).

- [ ] **Step 6: Commit**

```sh
git add packs/gcloud-aiplatform/
git commit -m "feat(packs): add gcloud-aiplatform pack — 6 Vertex AI read-only capabilities"
```

---

### Task 5: Fix Windows `.cmd` spawn + update seed list

**Files:**
- Already fixed: `crates/clix-core/src/execution/backends/subprocess.rs` (`.cmd` fallback)
- Modify: `crates/clix-core/src/packs/seed.rs` — add gcloud-aiplatform to seed list

- [ ] **Step 1: Update seed to include gcloud-aiplatform**

Read `crates/clix-core/src/packs/seed.rs`. Find the `SEED_PACKS` constant or `seed_packs()` function and add `gcloud-aiplatform` to the list so `clix init` seeds it automatically.

The current seed list (from earlier in the codebase) references pack directories by path relative to the binary. Update the seed list to include the new pack:

```rust
// In seed.rs, add to the BUILT_IN_PACKS array:
"gcloud-aiplatform",
```

The exact change depends on the current seed.rs implementation. Read the file first, then add the entry following the same pattern as the existing packs.

- [ ] **Step 2: Commit the subprocess fix and seed update**

```sh
git add crates/clix-core/src/execution/backends/subprocess.rs
git add crates/clix-core/src/packs/seed.rs
git commit -m "fix(subprocess): .cmd fallback on Windows for gcloud and similar wrappers

feat(seed): add gcloud-aiplatform to built-in seed packs"
```

---

### Task 6: Final verification

- [ ] **Step 1: Full test suite**

```sh
cargo test 2>&1 | tail -15
```

Expected: all tests pass, 0 failures.

- [ ] **Step 2: Smoke test the full namespace flow**

```sh
./target/debug/clix capabilities list
./target/debug/clix capabilities list --namespace gcloud.aiplatform
./target/debug/clix capabilities list --all
./target/debug/clix capabilities list --json
./target/debug/clix run gcloud.aiplatform.models.list --input project=alteryxone-dev-7393 --input region=us-central1
./target/debug/clix receipts list
```

- [ ] **Step 3: Commit docs**

```sh
git add docs/agent-tool-registry.md docs/superpowers/plans/2026-04-13-namespace-tools-list.md
git commit -m "docs: agent tool registry — progressive context loading architecture"
```
