# clix

**An agent-optimized CLI gateway. CLIs are vastly more powerful than MCP servers — clix makes them reliable, discoverable, and safe for agent use.**

Agents talk to clix as a plain CLI tool. No MCP tool registration, no upfront context cost, no catalogue to maintain. The agent discovers capabilities on demand, calls them with typed inputs, gets structured JSON output and a receipt. On Linux, subprocess calls run inside a real OS-level jail (namespaces, Landlock, seccomp, binary pinning). On macOS / Windows you get policy enforcement and a full audit trail.

> **Full OS isolation:** Linux x86_64 and aarch64.
> **Policy + audit (no OS jail):** macOS, Windows.

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

**Why not MCP?** MCP requires registering every tool upfront. For a gateway like clix
with dozens of capabilities, that means injecting a full catalogue into every prompt —
*before the agent has called a single tool*. This is the MCP anti-pattern: paying the
context cost of every tool whether or not the agent ever uses it.

clix agents discover and call capabilities on demand. The context cost is zero until a
capability is actually used.

> clix does have an optional MCP transport (`clix mcp`) for editors that require it.
> It is a compatibility shim, not the intended usage. If your editor forces MCP, use it.
> If it doesn't, use the CLI.

---

## What it does

**Rolling discovery — zero upfront context cost.** Agents list available namespaces, drill into the one they need, and call a capability. No 500-tool catalogue injected at prompt start. Context cost is proportional to what the agent actually uses.

**Typed, reliable interfaces.** Each capability has a declared input schema, consistent JSON output, and predictable exit codes. Agents don't parse `--help` or guess flag names — they read the schema and call.

**Packs — curated CLI slices.** Install a `gcloud-readonly` pack and the agent gets `projects list`, `compute instances list`, etc. — scoped read access, nothing that writes or deletes. Packs ship as signed `.clixpack` bundles; community packs can be verified against a trusted key before install.

**Policy enforcement.** Unknown capabilities are denied by default. Policy rules (allow / deny / require-approval) are evaluated against execution context (user, environment, profile, side-effect class) before any subprocess runs. `clix run --dry-run` lets agents preview the policy decision without executing.

**OS-level isolation (Linux).** Subprocess calls run inside a jailed worker process: Linux namespaces (user, mount, network, IPC, UTS), Landlock filesystem restrictions, seccomp syscall filtering, cgroup limits, and binary SHA-256 pinning. Credential files are owned by a separate broker process at `0700` — they never appear in the agent's environment.

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
clix is a CLI gateway. Discover and call tools on demand — no upfront catalogue.
  clix capabilities list --json        # browse namespaces
  clix capabilities show <name> --json # get full schema for one capability
  clix run <name> -i key=val --json    # run it (returns {ok, result, receipt_id})
  clix run <name> --dry-run --json     # preview policy decision without executing
  clix doctor --json                   # health check
Exit codes: 0 ok, 1 denied, 2 needs approval.
```

See [docs/agent-quickstart.md](docs/agent-quickstart.md) for the complete reference.

---

## Interactive TUI

```sh
clix tui
```

A full-screen terminal interface for managing packs, profiles, capabilities, and secrets without memorising CLI flags.

- **Sidebar navigation** — up/down moves through sections; Enter enters content; Esc walks back up to the sidebar
- **Breadcrumb header** — always shows where you are (`clix › Secrets › Configure Infisical`)
- **Async operations** — Infisical connectivity tests, broker pings, and pack installs run off the draw thread; a spinner shows while work is in progress
- **Confirm before discard** — Esc on a dirty form shows a `y/n` prompt before discarding unsaved input
- **Toast notifications** — float above open dialogs so a save confirmation never ejects your wizard

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

Built-in packs (installed by `clix init`): `base`, `kubectl-observe`, `gcloud-readonly`, `gh-readonly`, `git-readonly`, `docker-observe`, `podman-observe`, `aws-readonly`, `az-readonly`, `helm-observe`.

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
  clix-cli/     # CLI binary (all subcommands) + interactive TUI
  clix-serve/   # MCP/JSON-RPC server (the gateway)
  clix-worker/  # Jailed worker process
  clix-broker/  # Credential daemon
  clix-shim/    # PATH shim binary
  clix-testkit/ # Shared integration test harness
packs/
  base/               # system.date, system.echo
  kubectl-observe/    # 8 read-only capabilities
  gcloud-readonly/    # 6 read-only capabilities
  gh-readonly/        # 5 read-only capabilities
  git-readonly/       # 4 read-only capabilities
  docker-observe/     # 6 read-only capabilities
  podman-observe/     # 5 read-only capabilities
  aws-readonly/       # 6 read-only capabilities
  az-readonly/        # 6 read-only capabilities
  helm-observe/       # 4 read-only capabilities
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
- [Roadmap](TODO.md) — what's coming next
- [Security / Threat Model](SECURITY.md) — what clix protects against and known limitations
