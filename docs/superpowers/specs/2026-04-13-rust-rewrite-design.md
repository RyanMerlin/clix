# clix Rust Rewrite — Design Spec

**Date:** 2026-04-13  
**Status:** Approved  
**Replaces:** Go implementation in `internal/clix/`

---

## Overview

clix is a policy-first CLI control plane for agentic tool use. This document specifies the complete rewrite from Go to Rust. The rewrite is a hard cutover — no Go compatibility layer, no legacy bridge. The goal is a world-class MVP designed natively for the AI agent world: strongly typed, async, MCP-compatible, and audit-first.

---

## Core Decisions

| Decision | Choice | Rationale |
|---|---|---|
| Type strategy | Typed core, flexible manifest edges | Strong types on execution hot path; `serde_json::Value` at pack schema boundaries |
| Async runtime | Fully async (tokio) | I/O-bound workload: subprocess, HTTP, RPC serving, concurrent agent connections |
| Receipts storage | SQLite (sqlx) for receipts, flat YAML for everything else | Audit trail queryable; config/packs stay git-friendly and human-editable |
| Manifest format | YAML preferred, JSON accepted | YAML-first like Kubernetes; JSON accepted for tooling compat; both schema-validated |
| MCP compliance | MCP core + clix extensions | Full `tools/*` MCP compliance; `workflows/*` and `onboard/*` as declared extensions |
| Onboarding | Probe + structured `OnboardReport` | Agent-driven refinement; clix does discovery, agent does authoring |
| Sandbox | Linux Landlock enforced, no-op + documented elsewhere | `sandbox_enforced` flag on every receipt surfaces actual enforcement status |
| Architecture | Cargo workspace, typed core library + thin binaries | `clix-core` embeddable; CLI and server are thin consumers |

---

## Workspace Structure

```
clix/
├── Cargo.toml                  # workspace root
├── crates/
│   ├── clix-core/              # domain types + execution engine (no tokio)
│   │   └── src/
│   │       ├── manifest/       # Pack, Profile, Capability, Workflow types + serde
│   │       ├── policy/         # PolicyBundle, evaluate_policy(), Decision enum
│   │       ├── execution/      # run_capability, run_workflow, backends
│   │       ├── receipts/       # SQLite receipt store via sqlx
│   │       ├── secrets/        # CredentialSource, Infisical client
│   │       ├── packs/          # install, discover, validate, diff, bundle, onboard
│   │       ├── schema/         # JSON Schema validation (jsonschema crate)
│   │       ├── sandbox/        # Landlock (#[cfg(linux)]), no-op stub elsewhere
│   │       └── state.rs        # ClixState, path resolution, config load
│   ├── clix-cli/               # clap v4 binary — thin shell over clix-core
│   ├── clix-serve/             # tokio + axum MCP+extensions RPC server
│   └── clix-sandbox/           # sandbox integration tests (Linux only)
├── packs/                      # built-in pack YAML files (git-tracked)
└── docs/
```

---

## Key Dependencies

| Crate | Purpose |
|---|---|
| `serde` + `serde_json` + `serde_yaml` | Manifest I/O, both formats |
| `sqlx` (sqlite, tokio runtime) | Receipts database |
| `tokio` | Async runtime — in `clix-serve` and `clix-cli` only |
| `axum` | HTTP transport for serve mode |
| `clap` v4 (derive) | CLI subcommands, `--json` flag, completions |
| `reqwest` | Infisical + approval webhook HTTP |
| `jsonschema` | Capability input validation + manifest schema validation |
| `minijinja` | Template rendering for capability args (`{{ input.name }}`) |
| `thiserror` | Typed errors in `clix-core` |
| `anyhow` | Error handling in binaries |
| `sqlx` | SQLite receipts (async, compile-time queries) |
| `uuid` | Receipt IDs |
| `chrono` | Timestamps |
| `landlock` | Linux sandbox — `#[cfg(target_os = "linux")]` |
| `clap_complete` | Shell completion generation at build time |

---

## Domain Model

### Policy

```rust
pub enum Decision {
    Allow,
    Deny { reason: String },
    RequireApproval { reason: String, policy: PolicyMatch },
}
```

### Capability Manifest

```rust
pub struct CapabilityManifest {
    pub name: String,
    pub version: u32,
    pub description: Option<String>,
    pub backend: Backend,
    pub risk: RiskLevel,
    pub side_effect_class: SideEffectClass,
    pub sandbox_profile: Option<String>,
    pub approval_policy: Option<String>,
    pub input_schema: serde_json::Value,       // JSON Schema — flexible edge
    pub validators: Vec<Validator>,
    pub credentials: Vec<CredentialSource>,
}

pub enum Backend {
    Subprocess { command: String, args: Vec<String>, cwd_from_input: Option<String> },
    Builtin    { name: String },
    Remote     { url: String },
}

pub enum RiskLevel        { Low, Medium, High, Critical }
pub enum SideEffectClass  { None, ReadOnly, Additive, Mutating, Destructive }
```

### Execution Context & Result

```rust
pub struct ExecutionContext {
    pub env: String,
    pub cwd: PathBuf,
    pub user: String,
    pub profile: String,
    pub approver: Option<String>,
}

pub struct ExecutionResult {
    pub ok: bool,
    pub exit_code: i32,
    pub stdout: String,
    pub stderr: String,
    pub backend: BackendTrace,
}
```

### Receipt

```rust
pub struct Receipt {
    pub id: Uuid,
    pub kind: ReceiptKind,           // Capability | Workflow
    pub capability: String,
    pub created_at: DateTime<Utc>,
    pub status: ReceiptStatus,       // Succeeded | Failed | Denied | PendingApproval
    pub decision: Decision,
    pub input: serde_json::Value,    // user-supplied args — flexible edge
    pub context: ExecutionContext,
    pub execution: Option<ExecutionTrace>,
    pub sandbox_enforced: bool,      // true only on Linux with Landlock active
}
```

---

## Execution Pipeline

Every `tools/call` or `clix run` flows through this sequence:

```
input
  │
  ▼
validate_input_schema()       ← jsonschema, returns Vec<ValidationError>
  │
  ▼
evaluate_policy()             ← returns Decision enum
  │
  ├─ Deny           → write receipt → return ClixError::Denied
  ├─ RequireApproval
  │    ├─ webhook configured → POST ApprovalRequest, await response (reqwest)
  │    │    ├─ denied   → write receipt → return ClixError::ApprovalDenied
  │    │    └─ approved → annotate ExecutionContext, continue
  │    └─ no webhook  → write receipt (PendingApproval) → return to caller
  │
  ▼
run_validators()              ← requiredPath, denyArgs, requiredInputKey
  │
  ▼
render_args()                 ← minijinja: {{ input.namespace }}, {{ context.env }}
  │
  ▼
resolve_credentials()         ← Literal → Env → Infisical (cached per process)
  │
  ▼
execute_backend()
  ├─ Subprocess  → tokio::process::Command, inject secrets, redact from output
  ├─ Builtin     → match name, call Rust fn
  └─ Remote      → reqwest to Unix socket or HTTP daemon
  │
  ▼
write_receipt()               ← sqlx INSERT, sandbox_enforced flag, redacted output
  │
  ▼
return ExecutionOutcome
```

**Key improvements over Go:**
- `Decision` enum — exhaustive match, no stringly-typed `"deny"` comparisons
- `SecretRedactor` typed struct — holds resolved secret values, scrubs stdout/stderr before receipt write
- `minijinja` — Jinja2-compatible templates, richer than Go's `text/template`
- One auth round-trip to Infisical per process, in-memory token cache

---

## RPC / Serve Layer

### Transports

All three transports share a single `async fn dispatch(state, req) -> JsonRpcResponse`:

- **stdin/stdout** — newline-delimited JSON-RPC (default for agent embedding)
- **Unix socket** — `tokio::net::UnixListener`, one task per connection
- **HTTP** — `axum`, POST `/`

### Protocol

MCP-compliant core with clix extensions:

```
MCP core (any MCP-compatible agent):
  initialize        → server info, capabilities, sandboxEnforced status
  tools/list        → all capabilities across active profiles
  tools/call        → run_capability()
  resources/list    → installed packs as MCP resources

clix extensions (declared in initialize response):
  workflows/list    → all workflows across active profiles
  workflows/run     → run_workflow()
  onboard/probe     → probe a CLI, return OnboardReport
  packs/list        → installed packs + version + profile count
  status/get        → config, active profiles, sandbox enforcement status
```

### `initialize` Response

```json
{
  "serverInfo": { "name": "clix", "version": "0.2.0" },
  "capabilities": {
    "tools": true,
    "resources": true,
    "extensions": { "clix": { "workflows": true, "onboard": true } }
  },
  "sandboxEnforced": true
}
```

Agents that don't understand clix extensions ignore them safely.

---

## Pack Management

### Directory Layout (unchanged)

```
~/.clix/
├── config.yaml
├── policy.yaml
├── packs/
├── profiles/
├── capabilities/
├── workflows/
├── receipts.db             ← SQLite
├── bundles/
└── cache/
```

### Pack Commands

```
clix pack list
clix pack show <name>
clix pack discover <path>           # validate without installing
clix pack validate <path>           # schema + YAML lint, non-zero exit on error
clix pack diff <name> <path>        # show changes vs installed version
clix pack install <path|zip>
clix pack bundle <path>             # produce <name>-v<ver>.clixpack.zip + .sha256
clix pack publish <path>            # copy to ~/.clix/bundles/published/ + index.json
clix pack scaffold <name> --preset <>
clix pack onboard <name> --command <>
```

### OnboardReport

```rust
pub struct OnboardReport {
    pub cli: String,
    pub probed_at: DateTime<Utc>,
    pub version_output: Option<String>,
    pub help_sections: Vec<HelpSection>,
    pub inferred_subcommands: Vec<InferredSubcommand>,
    pub suggested_preset: Preset,
    pub confidence: f32,                              // 0.0–1.0
    pub suggested_capabilities: Vec<CapabilityManifest>,
    pub warnings: Vec<String>,
}
```

Agents consume this and drive refinement. clix does discovery; agents do authoring.

---

## Full CLI Surface

```
clix init
clix status                             # config, active profiles, sandbox status
clix version

clix capabilities list [--json]
clix capabilities show <name> [--json]
clix run <capability> [--input k=v] [--json]

clix workflow list [--json]
clix workflow run <name> [--input k=v] [--json]

clix profile list [--json]
clix profile show <name> [--json]
clix profile activate <name>
clix profile deactivate <name>

clix receipts list [--limit N] [--status <>] [--json]
clix receipts show <id> [--json]
clix receipts tail                      # live tail, streams JSON lines

clix serve [--socket <path>] [--http <addr>]

clix pack list [--json]
clix pack show <name> [--json]
clix pack discover <path> [--json]
clix pack validate <path>
clix pack diff <name> <path> [--json]
clix pack install <path>
clix pack bundle <path>
clix pack publish <path>
clix pack scaffold <name> --preset <>
clix pack onboard <name> --command <> [--json]
```

`--json` flag on all read commands for agent-friendly machine-readable output.  
Shell completions generated via `clap_complete` at build time.

---

## Secrets

Resolution precedence (explicit in type system):

```rust
pub enum CredentialSource {
    Env      { env_var: String,  inject_as: String },
    Literal  { value: String,    inject_as: String },  // dev/test only
    Infisical { secret_ref: InfisicalRef, inject_as: String },
}
```

Resolution order: `Literal` → `Env` → `Infisical` (network last, cached per process).

---

## Sandbox

- **Linux:** Landlock exec allowlist enforced via `landlock` crate, `#[cfg(target_os = "linux")]`
- **macOS / Windows:** No-op stub, no enforcement
- **Visibility:** `sandbox_enforced: bool` on every `Receipt` and in `status/get` RPC response — agents always know what's active
- **`clix status`** shows sandbox enforcement status prominently

---

## Error Handling

- `clix-core` uses `thiserror` — typed `ClixError` enum, every error variant is explicit
- Binaries (`clix-cli`, `clix-serve`) use `anyhow` for top-level error propagation
- No `unwrap()` in production paths — all fallible operations return `Result`

---

## Testing Strategy

- `clix-core` unit tests per module — policy evaluation, schema validation, template rendering, credential resolution all testable without I/O
- Integration tests in `clix-sandbox` (Linux CI only) for Landlock enforcement
- `clix-serve` tested with in-process tokio test harness — no real socket/HTTP needed
- Infisical integration tests behind `#[cfg(feature = "integration")]` flag, same as Go

---

## Out of Scope (Post-MVP)

- Dynamic plugin loading (`dlopen`, `dyn Backend` trait objects)
- `onboard/start` + `onboard/status` multi-turn RPC protocol (Option C)
- macOS Seatbelt sandbox
- Pack registry / remote discovery
- TLS on the HTTP transport
