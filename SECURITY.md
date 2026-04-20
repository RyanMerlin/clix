# Security Policy

## Reporting a Vulnerability

Report exploitable bugs **privately** — do not open a public GitHub issue.

- Email: rmerlin5@pm.me, or use [GitHub private vulnerability reporting](https://github.com/rmerlin/clix/security/advisories/new).
- Response target: acknowledgement within 72 hours, patch timeline within 14 days.
- Please include: affected component, reproduction steps, impact assessment.

---

## Threat Model

clix sits between an AI agent and real CLI tools. Its job is to enforce policy,
audit every invocation, and limit blast radius when an agent misbehaves or is
compromised.

### What clix defends against (on Linux with `warm_worker` isolation)

| Actor | Asset | Attack | Mitigation |
|---|---|---|---|
| Compromised agent | Host filesystem | `rm -rf /`, exfil of secrets from disk | Worker runs in a mount namespace with `pivot_root`; only declared paths are bind-mounted; Landlock allows only the pinned binary to execute |
| Compromised agent | Host network | Exfil over raw sockets, lateral movement | Network namespace isolates the worker; no host network by default |
| Compromised agent | Credentials | Reading ambient `~/.config/gcloud`, `~/.kube/config` | Credentials are minted per-invocation by the broker and injected as env vars; no long-lived cred files inside the jail |
| Compromised agent | Other processes | Ptrace, `/proc` inspection of sibling processes | PID + IPC namespaces; seccomp deny-list blocks `ptrace`, `process_vm_readv`, `perf_event_open` |
| Rogue capability | Binary substitution | Replacing `gcloud` with a backdoored binary | Binary SHA-256 is pinned at worker spawn; mismatches abort the handshake |
| Rogue pack | Supply-chain | Unsigned pack overrides allow-listed capabilities | Ed25519 pack signing; trust store at `~/.clix/trusted-pack-keys/`; unsigned packs are rejected by default |
| Insider / misconfigured policy | Unrestricted execution | Policy allows `Destructive` side-effect caps silently | Default policy is `Deny`; capabilities must be explicitly allowed; every invocation writes a receipt |

### What clix does NOT defend against

| Limitation | Explanation |
|---|---|
| **Malicious trusted packs** | If you `clix trust-key` a key you don't control, all packs signed by it run with your policy. The trust store is the root of trust — protect it. |
| **Same-UID local attacker with shell** | An attacker already running as the same user can kill the broker, modify `~/.clix/`, or ptrace the gateway process. clix is not a privilege-escalation boundary. |
| **Kernel 0-days** | The jail relies on Linux namespaces, seccomp BPF, and Landlock. A kernel exploit that bypasses these can escape. Keep your kernel patched. |
| **macOS / Windows** | No OS-level isolation on non-Linux. clix runs in policy-only mode (receipts, deny rules, and audit log still work). Every startup prints a `SANDBOX DISABLED` banner. |
| **Firecracker / VM-level isolation** | The `firecracker` isolation tier is not yet implemented. The manifest parser rejects it with an error. |
| **Multi-tenant deployments** | clix runs as a single user. The broker authenticates callers via `SO_PEERCRED` (UID match only). Do not expose the broker socket to untrusted UIDs. |
| **Network egress from the gateway process** | The gateway itself is not jailed. Only worker subprocesses are isolated. An agent that finds an RCE in the gateway can reach the network. |

---

## Security Architecture

```
┌─────────────────────────────────────────────────────────────────┐
│ AI agent (Claude Code, cursor, custom)                          │
│   calls clix via MCP stdio transport                            │
└──────────────────────────┬──────────────────────────────────────┘
                           │ JSON-RPC (stdio)
┌──────────────────────────▼──────────────────────────────────────┐
│ clix-serve / clix-cli (gateway)                                 │
│   • Policy evaluation (default: Deny)                           │
│   • Input schema validation                                     │
│   • Credential resolution (Infisical / env / literal)           │
│   • Receipt write (SQLite)                                      │
└────────────┬──────────────────────────┬────────────────────────-┘
             │ Unix socketpair          │ Unix socket
┌────────────▼───────────────┐  ┌──────▼──────────────────────────┐
│ clix-worker (jailed)       │  │ clix-broker                      │
│   Linux jail:              │  │   • SO_PEERCRED UID check        │
│   • user/mount/net/        │  │   • Mints short-lived tokens     │
│     ipc/uts namespaces     │  │     (gcloud, kubectl, …)         │
│   • pivot_root             │  │   • Approval request queue       │
│   • Landlock Execute-only  │  └──────────────────────────────────┘
│   • seccomp BPF deny-list  │
│   • Binary SHA-256 verify  │
└────────────────────────────┘
```

### Trust boundaries

1. **Gateway → Worker**: one socket pair per worker process; handshake verifies binary SHA-256 before the first request is dispatched.
2. **Gateway → Broker**: `SO_PEERCRED` check ensures the connecting process runs as the same UID; the broker mints tokens only for explicitly registered CLI adapters.
3. **Pack → Trust store**: pack signatures are verified against Ed25519 public keys in `~/.clix/trusted-pack-keys/` before any capability from the pack is loaded.

---

## Known Limitations & In-Progress Work

- `jail_config_digest` is captured in receipts but not yet verified on re-read (tracked in `docs/.dev/design/TODO.md`).
- `binary_sha256` in `IsolatedDispatch` is populated with a random UUID as a placeholder in the non-Linux path (non-Linux has no real isolation anyway).
- ARM Linux (aarch64) skips seccomp due to syscall table differences — only namespace and Landlock enforcement applies.
