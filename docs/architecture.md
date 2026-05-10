# clix Architecture

`clix` is an agent-optimized CLI gateway implemented in Rust. It makes CLIs first-class tools for AI agents: rolling discovery, typed inputs, consistent JSON output, full audit trail — without the context overhead of MCP tool registration. OS-level isolation (namespaces, Landlock, seccomp) and credential mediation are built in so agents can be given broad CLI access without trust concerns.

## Workspace

```
crates/
  clix-core/    # Pure sync library — manifests, policy, execution, receipts, secrets, packs
  clix-cli/     # tokio binary, thin dispatch layer for all subcommands
  clix-serve/   # tokio + axum — JSON-RPC / MCP server (the gateway)
  clix-worker/  # Jailed worker process — receives dispatched commands over a Unix socket
  clix-broker/  # Credential daemon — owns credential files, mints ephemeral tokens
  clix-shim/    # PATH shim binary — forwards CLI invocations through the gateway
packs/          # Built-in YAML packs (base, kubectl-observe, gcloud-readonly, gh-readonly, git-readonly, docker-observe, podman-observe, aws-readonly, az-readonly, helm-observe)
```

## Core concepts

- **Capabilities** — typed execution units wrapping external CLIs, builtins, or HTTP backends. Each has a risk level, side-effect class, input schema (JSON Schema), sandbox profile, isolation tier, and credential sources.
- **Workflows** — ordered playbooks that chain capabilities, passing outputs as inputs.
- **Profiles** — named sets of active packs + capabilities. Multiple profiles can be stacked. Switching profiles changes which capabilities are available and what policy applies.
- **Policy** — per-capability allow / deny / require-approval rules evaluated against execution context (user, environment, profile, side-effect class).
- **Packs** — installable zip bundles that ship capabilities, workflows, profiles, and schemas together.
- **Receipts** — append-only SQLite-backed execution log. Every run produces a receipt with decision, inputs, outcome, isolation tier, binary hash, and `sandbox_enforced` flag.

## Three-process trust model

```
┌──────────────┐  MCP/HTTP/stdio   ┌────────────────┐  control     ┌───────────────────────┐
│   Agent      │ ────────────────▶ │  clix-gateway  │ ───────────▶ │ warm clix-worker(s)   │
│ (+ shim bin) │                   │ (clix-serve)   │   socket     │ per (profile, binary) │
└──────────────┘                   └────────┬───────┘              │  — jailed subprocess  │
                                            │ SO_PEERCRED          │  — ephemeral token FD │
                                            ▼                      └───────────────────────┘
                                   ┌────────────────┐
                                   │  clix-broker   │  uid=clix-broker, 0700 on creds dir
                                   │  creds + mint  │  mints short-lived tokens on demand
                                   └────────────────┘
```

Each component runs under a separate OS user / trust level:

- **clix-broker** — sole owner of credential files (mode `0700`). Exposes a Unix socket; only the gateway UID may connect (verified via `SO_PEERCRED`). Mints ephemeral tokens (gcloud, kubectl, generic secrets) on demand. Credentials never appear in the agent's filesystem view.
- **clix-gateway** (`clix-serve`) — supervisor that handles all MCP/JSON-RPC traffic. Keeps a `WorkerRegistry` of warm worker processes keyed by `(profile, binary, isolation_tier)`. Evaluates policy before every dispatch.
- **clix-worker** — thin binary that enters a jail at startup (namespaces + Landlock + seccomp + cgroups) and then loops, accepting dispatch requests over its control socket. One worker per active `(profile, binary)` pair; idle workers are reaped after a configurable TTL.

**Why this prevents bypass:** the agent runs in the gateway process's environment, where raw credential files are inaccessible (owned by the broker with `0700`). If the agent runs the CLI binary directly, the shim intercepts it and routes through the gateway. The worker's jail ensures that even code executing inside the worker cannot escape to the host filesystem or network.

## Isolation tiers

| Tier | Use for | Boundary | Dispatch latency |
|---|---|---|---|
| `none` | `builtin` backend only | in-process | < 1 µs |
| `warm_worker` | all subprocess capabilities | user + mount + net + ipc + uts namespaces, Landlock, seccomp, cgroups, NO_NEW_PRIVS | < 5 ms |
| `firecracker` | high-risk / untrusted CLIs (opt-in) | microVM via jailer, vsock RPC, MMDS token handoff | < 5 ms warm / ~125 ms cold |

`warm_worker` is the default for all subprocess capabilities. Builtins run in-process (`none`). Firecracker is opt-in per capability manifest and requires Linux with KVM.

## Execution pipeline

```
tools/call {name, arguments}
  → policy.evaluate(capability, context) → Allow | Deny | RequireApproval
  → validate inputs against JSON Schema
  → run pre-flight validators (DenyArgs, etc.)
  → render args via Jinja2 templates
  → broker.mint_credentials(cli_name) → ephemeral env vars
  → WorkerRegistry.dispatch(profile, binary, tier, request)
      └─ worker: enter_jail() → exec pinned binary → stream result
  → SecretRedactor.redact(stdout, stderr)
  → receipts.write(outcome, isolation_tier, binary_sha256)
  → return MCP tool result
```

For `builtin` backends (`sys.date`, `sys.echo`), the worker registry is bypassed and the handler runs in-process.

## Policy evaluation

Policy rules are evaluated in order; the first matching rule wins. Rules can match on:

- capability name (exact or glob)
- side-effect class (`ReadOnly`, `Mutating`, `Destructive`, `None`)
- environment or profile name

Actions: `Allow`, `Deny`, `RequireApproval`. The default action (no matching rule) is `Allow`.

`RequireApproval` currently blocks until an approver responds on the approval socket, or returns `approvalRequired: true` if no approver is connected.

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
| `tools/call` | Execute a capability (full policy + isolation pipeline) |
| `resources/list` | Expose receipts as MCP resources |
| `resources/read` | Read a single receipt |
| `workflows/list` | clix extension |
| `workflows/run` | clix extension |
| `capabilities/list` | clix extension |
| `receipts/list` | clix extension |
| `receipts/get` | clix extension |
| `status/get` | Health + sandbox_enforced flag |

## Secrets

Credentials are resolved at execution time, then ephemeral broker-minted tokens are merged on top:

1. **env** — plain environment variable declared in the capability manifest
2. **infisical** — fetched from Infisical via REST API using Universal Auth / machine identity, with service token as a fallback
3. **broker** — short-lived token minted by `clix-broker` immediately before dispatch (gcloud, kubectl, etc.)

A `SecretRedactor` is built from all resolved values and applied to stdout/stderr before the receipt is written. Values are sorted longest-first to prevent partial replacement.

## Sandbox (warm worker detail)

On Linux, `clix-worker` calls `enter_jail()` at startup which:

1. Forks; parent writes `uid_map`/`gid_map` from the outer namespace (avoiding the uid_map chicken-and-egg problem)
2. Child calls `unshare(USER | MOUNT | NET | IPC | UTS)` (PID ns omitted; requires mounted `/proc`)
3. Sets up a tmpfs root with RO bind mounts of the pinned binary and its library closure
4. Calls `pivot_root` to the tmpfs
5. Applies Landlock (`AccessFs::Execute` on the pinned binary)
6. Installs a seccomp BPF deny-list (ptrace, mount, bpf, kexec, new namespaces, etc.)
7. Sets `NO_NEW_PRIVS` and drops all capabilities

Binary integrity: the binary path is resolved to an absolute path at registry load time; its SHA-256 is checked at every worker spawn. Workers reject requests if the hash drifts.

On macOS and Windows the sandbox is a no-op stub with a loud warning. Every receipt records `sandbox_enforced: bool` and `isolation_tier` so agents and operators can observe enforcement status.

## State and config

| Path | Purpose |
|------|---------|
| `~/.clix/config.yaml` | Global config (approval mode, Infisical, sandbox settings) |
| `~/.clix/capabilities/` | User-installed capability manifests |
| `~/.clix/packs/` | Installed packs |
| `~/.clix/profiles/` | Profile manifests |
| `~/.clix/receipts.db` | SQLite receipts database |

Override root with `CLIX_HOME`.
