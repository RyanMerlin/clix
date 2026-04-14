# clix — Next Steps & Backlog

This document captures the outstanding work after the isolation boundary and credential gateway milestone (April 2026). Items are grouped by area and roughly prioritized within each section.

---

## Isolation & Security

### Firecracker tier (`clix-isolate-fc`, feature-gated)
The `warm_worker` tier is complete. The Firecracker microVM tier is designed but not yet implemented.

- [ ] New crate `crates/clix-isolate-fc` behind `--features firecracker`
- [ ] `FirecrackerPool` keyed on `(profile, image_digest)` — pre-boots N VMs per profile
- [ ] `clix pack bake-image` — builds an ext4 rootfs from a pack manifest (pinned CLI binaries + `clix-execd` agent)
- [ ] vsock RPC for command/stream dispatch; MMDS for token handoff at VM boot
- [ ] Integration with `WorkerRegistry` dispatch path (new `IsolationTier::Firecracker` arm)
- [ ] Add benchmark: cold VM boot < 125 ms, warm dispatch < 5 ms (per spec)
- [ ] Feature-gated integration test: `clix pack bake-image` → pool boot → round-trip dispatch

### macOS / Windows isolation stubs
- [ ] macOS: `sandbox-exec` profile generation from `SandboxProfile` manifest
- [ ] Windows: AppContainer / Job Object wrapper in `crates/clix-worker` (conditional compile)
- [ ] Both: loud warning when `CLIX_ISOLATION_REQUIRE` is set and platform can't satisfy it — fail closed

### Seccomp policy hardening
- [ ] Per-capability seccomp allowlist (not just baseline deny-list) — compile from `sandbox_profile.syscalls`
- [ ] Seccomp audit mode: log-only violations before switching to deny (useful for pack authors)
- [ ] `clix pack validate` should warn when a capability requests `IsolationTier::None` but has a non-`Builtin` backend

### Network egress enforcement
- [ ] Per-worker netns + nftables egress allowlist from `sandbox_profile.network.egress_allowlist`
- [ ] `clix serve` should set up the slirp/tap for each worker at spawn time
- [ ] Default: deny-all outbound; capability manifests declare `egress_allowlist: ["api.example.com:443"]`

---

## Credential Broker (`clix-broker`)

The broker crate exists with the unix-socket protocol; the credential adoption workflow is partially complete.

- [ ] `clix init --adopt-creds` — migrate `~/.config/gcloud`, `~/.kube/config`, service-account JSONs to broker-owned `0700` directory
- [ ] `gcloud` token minter: OAuth refresh flow using broker-owned `application_default_credentials.json`
- [ ] `kubectl` token minter: `ExecCredential` callback to broker instead of kubeconfig
- [ ] Generic "inject at exec" secret store: arbitrary `KEY=value` secrets per CLI name
- [ ] `SO_PEERCRED` enforcement: broker refuses connections from UIDs other than gateway UID
- [ ] Broker daemon management: `clix serve --start-broker` / `clix serve --stop-broker`; launchd/systemd unit templates
- [ ] Token TTL and rotation: minted tokens tracked, revoked, rotated on expiry
- [ ] Add `azure` and `aws` minters behind the same `TokenMinter` trait

---

## Shim Infrastructure (`clix-shim`)

- [ ] `clix init --install-shims` — write per-capability shims to `$CLIX_HOME/bin/{gcloud,kubectl,...}`
- [ ] Shims are tiny Rust binaries that RPC into the gateway socket; running them is equivalent to `clix run <cap>`
- [ ] Activation script: prepend `$CLIX_HOME/bin` to PATH in `.bashrc`/`.zshrc`/fish config
- [ ] Shim rejects and errors loudly if the active profile disallows the capability
- [ ] `clix shim list` — show installed shims and which capabilities they map to
- [ ] `clix shim uninstall <name>` — remove a shim

---

## Execution & Policy

- [ ] `RequireApproval` with a live approver: implement the approval RPC path so an approver process can grant/reject in-flight requests (currently blocks without an approver)
- [ ] Approval timeout: receipts stuck in `PendingApproval` should expire after configurable TTL
- [ ] `DenyArgs` validator: replace naive `contains()` with a proper regex/glob match to make it bypass-resistant
- [ ] Workflow conditional steps: `if:` expressions using Jinja2 over step outputs
- [ ] Workflow parallel steps: `parallel: true` on step groups
- [ ] Workflow output schema validation: validate each step's output against a declared schema before passing to the next step
- [ ] `run_capability` result streaming: for long-running subprocesses, stream stdout/stderr to the caller rather than buffering

---

## Receipts & Observability

- [ ] Receipt schema migration tooling: `clix receipts migrate` to apply SQLite schema upgrades non-destructively
- [ ] `isolation_tier`, `binary_sha256`, `token_mint_id`, `jail_config_digest` columns are defined; `binary_sha256` and `token_mint_id` are not yet populated — wire from worker dispatch result
- [ ] Receipt export: `clix receipts export --format jsonl` for shipping to SIEM/audit systems
- [ ] Metrics endpoint: `GET /metrics` (Prometheus format) on the HTTP transport — counters for calls, denials, errors per capability
- [ ] Structured logging: replace `eprintln!` in gateway/worker/broker with `tracing` spans

---

## Pack Ecosystem

- [ ] Pack signing: sign `.clixpack` files with a key pair; `clix pack install` verifies signature
- [ ] Pack registry: `clix pack search <term>` against a hosted index (analogous to crates.io for packs)
- [ ] Pack versioning: `clix pack update` to pull newer versions; manifest-level `requires_clix_version`
- [ ] More built-in packs: `aws-readonly`, `azure-readonly`, `docker-readonly`, `terraform-plan`
- [ ] `clix pack test` — run pack's declared capability smoke tests in a sandbox

---

## CLI & UX

- [ ] `clix profile create <name> --from <existing>` — clone a profile as a starting point
- [ ] `clix capabilities inspect <name>` — show full manifest, resolved backend, active policy, sandbox profile
- [ ] `clix run --dry-run` — evaluate policy and validators but skip execution; print what would happen
- [ ] `clix serve --socket` should print its PID and socket path to stderr for scripting
- [ ] TUI: capability search with policy and sandbox info visible inline
- [ ] `clix doctor` — check broker socket, worker binary, shim PATH, sandbox support, and report issues

---

## Testing & CI

- [ ] Bypass smoke test (Linux CI only): broker-owned credential inaccessible to agent UID, `clix run` succeeds, receipt shows `isolation_tier=warm_worker`
- [ ] Jail escape tests: add `mount`, `bpf`, `kexec` probes; expand to cover network-deny when egress enforcement lands
- [ ] `CLONE_NEWPID` re-enablement: once proc mount works in test environment, re-add PID namespace isolation
- [ ] Firecracker integration tests (feature-gated, Linux CI only)
- [ ] Fuzz the JSON-RPC dispatch layer (`tools/call` with arbitrary params)
- [ ] Property tests for policy evaluation: `proptest` across random capability × policy × context combinations

---

## Documentation

- [ ] `docs/isolation.md` — deep-dive on the three-tier isolation model, namespace setup, seccomp policy, Firecracker path
- [ ] `docs/broker.md` — credential adoption walkthrough, broker protocol, token minters
- [ ] `docs/shims.md` — how shims work, installation, what happens on a bypass attempt
- [ ] `docs/packs-authoring.md` — guide for writing and publishing packs; manifest reference
- [ ] Update `docs/architecture.md` to cover three-process trust model and worker registry (see below — partially done)
- [ ] Add inline doc comments to `execution/mod.rs`, `policy/evaluate.rs`, `receipts/mod.rs`

---

## Out of scope for v1 (tracked for later)

- Broker HA / multi-user (single local broker only in v1)
- Image distribution and signing for Firecracker rootfs images
- Automatic creds adoption beyond gcloud + kubectl
- Windows AppContainer / Job Object full implementation
- macOS `sandbox-exec` full implementation
