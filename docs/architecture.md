# clix Architecture

`clix` is a policy-first CLI control plane for agentic tool use, implemented in Rust.

## Workspace

```
crates/
  clix-core/    # Pure sync library — no tokio dependency
  clix-cli/     # tokio binary, thin dispatch layer
  clix-serve/   # tokio + axum, JSON-RPC / MCP server
packs/          # Built-in YAML packs (base, kubectl-observe, gcloud-readonly, gh-readonly)
```

## Core concepts

- **Capabilities** — typed execution units wrapping external CLIs, builtins, or HTTP backends. Each has a risk level, side-effect class, input schema (JSON Schema), and credential sources.
- **Workflows** — ordered playbooks that chain capabilities, passing outputs as inputs.
- **Profiles** — named sets of active packs + capabilities. Multiple profiles can be stacked.
- **Policy** — per-capability allow / deny / require-approval rules evaluated against execution context (user, environment, profile).
- **Packs** — installable zip bundles that ship capabilities, workflows, profiles, and schemas together.
- **Receipts** — append-only SQLite-backed execution log. Every run produces a receipt with decision, inputs, outcome, and `sandbox_enforced` flag.

## Execution pipeline

```
run_capability(name, inputs)
  → validate inputs against JSON Schema
  → evaluate policy → Allow | Deny | RequireApproval
  → run validators (pre-flight checks)
  → render args via Jinja2 templates
  → resolve credentials (env / Infisical)
  → redact secrets from all output
  → execute backend (subprocess | builtin | remote HTTP)
  → apply Landlock sandbox (Linux only)
  → write receipt to SQLite
```

## Serve layer (MCP / JSON-RPC 2.0)

The `clix serve` command exposes all capabilities as MCP-compatible tools over three transports:

- **stdio** — newline-delimited JSON, default for agent tool use
- **Unix socket** — `/tmp/clix.sock` by default
- **HTTP** — axum on configurable port

MCP protocol version: `2024-11-05`. Methods:

| Method | Description |
|--------|-------------|
| `initialize` | Handshake, returns server info + capabilities |
| `tools/list` | All capabilities as MCP tool descriptors |
| `tools/call` | Execute a capability |
| `resources/list` | Expose receipts as MCP resources |
| `resources/read` | Read a single receipt |
| `workflows/list` | clix extension |
| `workflows/run` | clix extension |
| `capabilities/list` | clix extension |
| `receipts/list` | clix extension |
| `receipts/get` | clix extension |
| `status/get` | Health + sandbox_enforced flag |

## Secrets

Credentials are resolved at execution time:

- **env** — plain environment variable
- **infisical** — fetched from Infisical via REST API using a machine identity token

A `SecretRedactor` is built from all resolved values and applied to stdout/stderr before the receipt is written. Values are sorted longest-first to prevent partial replacement.

## Sandbox

On Linux, `clix` applies a [Landlock](https://landlock.io/) exec allowlist before forking the subprocess. The allowed paths come from each capability's `sandbox.allowed_exec` list in the manifest.

On macOS and Windows the sandbox is a no-op. Every receipt records `sandbox_enforced: bool` so agents can observe whether enforcement was active.

## State and config

| Path | Purpose |
|------|---------|
| `~/.clix/config.yaml` | Global config (approval mode, Infisical, sandbox settings) |
| `~/.clix/capabilities/` | User-installed capability manifests |
| `~/.clix/packs/` | Installed packs |
| `~/.clix/profiles/` | Profile manifests |
| `~/.clix/receipts.db` | SQLite receipts database |

Override root with `CLIX_HOME`.
