# clix

**A sandbox for CLI tools your coding agent runs.**

On Linux you get a real OS-level jail — namespaces, Landlock, seccomp, binary pinning. On macOS or Windows you get honest policy enforcement and a full audit trail with a clear `SANDBOX DISABLED` notice at startup. Either way, **unknown capabilities are denied by default** and every invocation is receipted.

The short version: clix stops an AI agent from `rm -rf`ing your filesystem, leaking your cloud credentials, or running anything you haven't explicitly allowed — with an audit log proving it.

> **Supported platforms for full isolation:** Linux x86_64 and aarch64.
> **Policy-only mode** (no OS jail, receipts + deny rules still work): macOS, Windows.

---

## How agents use clix — plain CLI, not MCP

> **clix is not an MCP server. Agents call it as a plain CLI tool.**

The agent runs `clix run <capability> --json` exactly like any other shell command.
There is no tool catalogue registration, no MCP handshake, no context window overhead.

```sh
# The agent calls these directly — no MCP involved
clix capabilities list --json        # what can I call?
clix capabilities show git.status --json  # what does this take?
clix run git.status --json           # run it
clix receipts list --json            # what ran?
```

**Why not MCP?** MCP requires registering every tool upfront, which bloats the context
window with a full catalogue. clix agents discover and call capabilities on demand —
the context cost is zero until a capability is actually used.

MCP transport exists as an optional integration mode for editors that require it
(`clix mcp`), but it is not the primary design and not recommended for general use.

---

## What it does

**Default deny.** Without an explicit allow rule in your policy, no capability runs. Agents can't call what you haven't declared. This is enforced by the policy engine, not "best effort."

**Curated tool access.** You install *packs* — bundles of capabilities that expose a safe slice of a CLI. A `gcloud-readonly` pack might allow `projects list` and `compute instances list`, but nothing that writes or deletes. The agent sees only what you've declared.

**OS-level isolation (Linux).** Subprocess capabilities run inside a jailed worker process: Linux namespaces (user, mount, network, IPC, UTS), Landlock filesystem restrictions, seccomp syscall filtering, and cgroup limits. The binary is pinned by SHA-256 at spawn time. Even if the agent tries to run the real binary directly, it won't have access to your credentials — those are owned by a separate broker process.

**Credential mediation.** `clix-broker` owns your credential files (gcloud ADC, kubeconfig, etc.) at `0700`. It mints short-lived tokens on demand and injects them directly into the worker process at execution time, so your credentials never appear in the agent's environment or filesystem.

**Full audit trail.** Every call — allowed, denied, or pending approval — is written to a local SQLite receipts database with inputs, outcome, isolation tier, and binary hash. `clix receipts list` shows you exactly what ran.

---

## Quick start

```sh
# Install
curl -fsSL https://raw.githubusercontent.com/RyanMerlin/clix/main/scripts/install.sh | sh

# Set up and install built-in packs
clix init

# See what's available (lean JSON — agent-friendly)
clix capabilities list --json
clix capabilities search "list" --json

# Get the full schema for one capability
clix capabilities show git.status --json

# Run a capability
clix run git.status --json
clix run git.log -i limit=10 --json

# Preview without executing
clix run git.status --dry-run --json

# Health check
clix doctor --json
```

All commands support `--json` for machine-readable, agent-parseable output.

The state directory defaults to `~/.clix` (`%USERPROFILE%\.clix` on Windows). Override with `CLIX_HOME`.

---

## Using with AI agents

clix is designed so agents interact with it as a CLI tool, not via MCP. The agent runs commands directly and pays context only for what it uses — no upfront tool catalogue registration.

The minimal agent prompt (copy into your system prompt):

```
clix is a sandboxed CLI gateway.
  clix capabilities list --json        # browse tools
  clix capabilities show <name> --json # get schema
  clix run <name> -i key=val --json    # run (returns {ok, result, receipt_id})
  clix run <name> --dry-run --json     # preview policy/isolation, no execution
  clix doctor --json                   # health
Exit codes: 0 ok, 1 denied, 2 needs approval.
```

See [docs/agent-quickstart.md](docs/agent-quickstart.md) for the complete reference.

---

## Shims

Shims let agents invoke tools as native CLI commands without any clix syntax. Once installed, `git status` in the agent's shell is a policy-enforced, sandboxed clix call.

```sh
# Install shims for any commands used by installed packs
clix init --install-shims git kubectl

# Source the activation script (adds ~/.clix/bin to PATH)
source ~/.clix/bin/activate.sh   # bash/zsh
source ~/.clix/bin/activate.fish # fish

# The gateway must be running for shim calls to work
clix serve --socket /tmp/clix-gateway.sock

# Now these route through clix transparently
git status
kubectl get pods

# Manage shims
clix shim list
clix shim uninstall git
```

---

## Profiles

Profiles let you switch the agent's permission level without restarting. A `readonly` profile might allow only read operations across all installed packs. A `deploy` profile might additionally allow specific write operations.

```sh
clix profile list
clix profile activate readonly
clix profile activate deploy
```

---

## Packs

Packs are the unit of distribution. Each pack is a YAML manifest (or a bundled `.clixpack` file) that declares capabilities, workflows, and profiles for a specific tool.

```sh
# Install a pack
clix pack install ./gcloud-readonly/
clix pack install ./my-pack.clixpack

# Validate before installing
clix pack validate ./my-pack/

# Bundle for distribution
clix pack bundle ./my-pack/ --out my-pack.clixpack

# Diff a local pack against what's installed
clix pack diff ./my-pack/ --against installed

# Scaffold a new pack
clix pack scaffold --name my-pack --commands "mycli"
```

Built-in packs: `base` (system utilities), `kubectl-observe`, `gcloud-readonly`, `gh-readonly`.

---

## AI integrations

### Claude Code / Cursor (MCP, project-scoped)

```sh
# Run from your project root:
clix init --claude-code   # writes .mcp.json + CLAUDE.md integration block
clix init --cursor        # writes .cursor/mcp.json
```

Restart the editor to load the MCP server. clix's `tools/list` returns namespace stubs by default, keeping registered-tool count low.

### Claude API / OpenAI-compatible APIs

```sh
# Two-tool pattern — ~400 tokens regardless of catalogue size (recommended)
clix tools export --format two-tool

# Full capability registration for a specific namespace
clix tools export --format claude --namespace git
clix tools export --format openai --namespace kubectl
```

See [docs/integration-claude.md](docs/integration-claude.md) for Python and TypeScript examples with the full `tool_use` loop.

### Gemini API

```sh
clix tools export --format gemini --namespace gcloud.aiplatform
```

See [docs/integration-gemini.md](docs/integration-gemini.md) for google-generativeai and google-genai SDK examples.

---

## MCP server (for editor integrations)

For Claude Desktop, Cursor, and other MCP-compatible editors:

```sh
# stdio transport — for MCP tool use in editors
clix serve

# HTTP transport
clix serve --http --port 3000

# Unix socket (also used by shims)
clix serve --socket /tmp/clix.sock
```

The server implements MCP protocol `2024-11-05` (`tools/*`, `resources/*`) plus clix extensions. Tool list defaults to namespace stubs to minimize upfront context; editors can drill into namespaces on demand.

For agents running in a terminal or subprocess, prefer the direct CLI pattern (`clix run`) over MCP — it's lighter and doesn't require a long-running server.

### One-shot JSON-RPC

Need the MCP schema from a script or agent without a running server?

```sh
clix mcp call tools/list --params '{"namespace":"git"}'
clix mcp call initialize
```

---

## Receipts

Every execution is logged — allowed, denied, or pending approval:

```sh
clix receipts list
clix receipts list --status denied
clix receipts list --json
```

Receipts include: capability name, inputs, policy decision, outcome, isolation tier, binary SHA-256, and timestamp.

---

## Build from source

```sh
cargo build -p clix-cli
cargo build --workspace
cargo test
cargo bench -p clix-core
```

Requires Rust 1.78+. The isolation features (warm worker, broker) require Linux with user namespace support.

---

## Workspace layout

```
crates/
  clix-core/    # Core library — manifests, policy, execution, receipts, secrets, packs
  clix-cli/     # CLI binary (all subcommands)
  clix-serve/   # MCP/JSON-RPC server (the gateway)
  clix-worker/  # Jailed worker process
  clix-broker/  # Credential daemon
  clix-shim/    # PATH shim binary
packs/
  base/               # system.date, system.echo
  kubectl-observe/
  gcloud-readonly/
  gh-readonly/
```

---

## Releases

Tagged releases are built by GitHub Actions for Linux (`x86_64`, `aarch64`), macOS (`x86_64`, `aarch64`), and Windows (`x86_64`). Releases include checksum sidecars, an SBOM, and GitHub provenance attestation artifacts.

Set `CLIX_STRICT_VERIFY=1` to verify the SBOM and attestations during install (requires `gh`).

---

## Examples

- [claude-code-gcp](examples/claude-code-gcp/) — Claude Code + GCP read-only access in under 10 minutes; shows pack, policy, and direct CLI invocation

## Docs

- [Agent quickstart](docs/agent-quickstart.md) — paste-into-prompt CLI reference for agents
- [Claude integration](docs/integration-claude.md) — direct CLI, two-tool API, Claude Code MCP setup
- [Gemini integration](docs/integration-gemini.md) — function declarations, google-generativeai SDK
- [Cursor integration](docs/integration-cursor.md) — MCP setup, profiles, troubleshooting
- [Architecture](docs/architecture.md) — system design, trust model, isolation tiers
- [Packs](docs/pack.md) — pack format reference
- [Release process](docs/release.md)
- [Roadmap](docs/design/TODO.md) — what's coming next
- [Security / Threat Model](SECURITY.md) — what clix protects against and known limitations
