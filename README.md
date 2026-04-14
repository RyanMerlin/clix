# clix

**A secure control plane for giving AI agents access to your CLI tools.**

AI agents are increasingly capable of running real commands — deploying infrastructure, querying databases, managing cloud resources. The problem is that "give the agent access to `gcloud`" today means giving it access to *all* of `gcloud`, with your full credentials, with no audit trail.

clix changes that. You define exactly which commands an agent may run, under what conditions, with what credentials — and enforce those boundaries at the OS level, not just in software.

---

## What it does

**Curated tool access.** You install *packs* — bundles of capabilities that expose a safe slice of a CLI. A `gcloud-readonly` pack might allow `projects list` and `compute instances list`, but nothing that writes or deletes. The agent sees only what you've declared.

**Policy enforcement.** Every capability call is evaluated against a policy before it runs. Rules can allow, deny, or require human approval — per capability, per side-effect class (read-only vs mutating vs destructive), or per profile. Denials are instant and logged.

**OS-level isolation (Linux).** Subprocess capabilities run inside a jailed worker process: Linux namespaces (user, mount, network, IPC, UTS), Landlock filesystem restrictions, seccomp syscall filtering, and cgroup limits. The binary is pinned by SHA-256 at spawn time. Even if the agent tries to run the real binary directly, it won't have access to your credentials — those are owned by a separate broker process.

**Credential mediation.** `clix-broker` owns your credential files (gcloud ADC, kubeconfig, etc.) at `0700`. It mints short-lived tokens on demand and injects them directly into the worker process at execution time, so your credentials never appear in the agent's environment or filesystem.

**Full audit trail.** Every call — allowed, denied, or pending approval — is written to a local SQLite receipts database with inputs, outcome, isolation tier, and binary hash. `clix receipts list` shows you exactly what ran.

**MCP-compatible.** clix speaks [Model Context Protocol](https://modelcontextprotocol.io/), so it works as a drop-in tool server for Claude, Cursor, and any other MCP-compatible agent.

---

## Quick start

```sh
# Install
curl -fsSL https://raw.githubusercontent.com/RyanMerlin/clix/main/scripts/install.sh | sh

# Set up and install built-in packs
clix init

# See what's available
clix capabilities list
clix pack list

# Run a capability
clix run system.date

# Start as an MCP server (stdio transport, for agent tool use)
clix serve
```

All read commands support `--json` for machine-readable output:

```sh
clix capabilities list --json
clix receipts list --json
clix status --json
```

The state directory defaults to `~/.clix` (`%USERPROFILE%\.clix` on Windows). Override with `CLIX_HOME`.

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

## Serve (MCP / JSON-RPC)

```sh
# stdio transport — default for MCP tool use
clix serve

# HTTP transport
clix serve --http --port 3000

# Unix socket
clix serve --socket /tmp/clix.sock
```

The server implements MCP protocol `2024-11-05` (`tools/*`, `resources/*`) plus clix extensions for workflows, capabilities, and receipts.

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

## Docs

- [Architecture](docs/architecture.md) — system design, trust model, isolation tiers
- [Packs](docs/pack.md) — pack format reference
- [Release process](docs/release.md)
- [Roadmap](docs/design/TODO.md) — what's coming next
