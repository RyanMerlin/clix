# clix — Roadmap & Next Steps

## Recently shipped (v0.3.0)

### Core hardening
- **Default-deny policy** — `PolicyBundle::default()` denies everything; `allow_all()` only in tests.
- **Binary pinning** — SHA-256 checked at every worker spawn; receipts include `binary_sha256`.
- **macOS honest-mode** — no fake sandbox; SANDBOX DISABLED banner at startup; `sandbox_enforced: false` in every receipt.
- **Receipts export** — JSONL/JSON export, `binary_sha256`, `token_mint_id`, Prometheus `/metrics`.
- **Pack signing** — Ed25519 bundle signing (`clix pack bundle --sign`), trust store, `--verify-sig` on install.
- **Broker integration** — `RequireApproval` enforcement via broker; `clix-broker` lifecycle surface.
- **Integration tests** — `clix-testkit` shared harness; suite covers policy, isolation, receipts.

### Pack ecosystem (14 packs, ~60 capabilities)
- **5 new packs**: `docker-observe`, `podman-observe`, `aws-readonly`, `az-readonly`, `helm-observe`
- **Expanded packs**: `gcloud-readonly` (6 caps), `gh-readonly` (5 caps), `kubectl-observe` (8 caps)
- **Starter policy**: `clix init` now seeds a `policy.yaml` allowing `none` + `readOnly` side-effects out of the box
- **XDG install path**: pack seeder searches `exe_dir/packs`, `~/.local/share/clix/packs`, then `./packs`
- **Docs honest**: `docs/pack.md` table reflects only real packs; phantom packs removed

### TUI overhaul (all 5 slices)
- **Infisical hang fixed**: `reqwest::blocking::Client` now has 10s timeout + 5s connect timeout on all 4 HTTP call sites in `clix-core/src/secrets/mod.rs`
- **Inline validation**: InfisicalSetup validates required fields (site_url, client_id, client_secret, environment) before any network call
- **Async work pool** (`tui/work.rs`): std::thread + mpsc (no tokio); jobs dispatched per-task; `App::tick()` drains results each frame — UI never blocks
- **Infisical save is async**: writes config to disk synchronously, dispatches `TestInfisical` job, shows spinner; inline error on failure; toast+close on success; Esc during save drops the job cleanly
- **Connectivity test async**: `t` on Secrets screen dispatches `PingConnectivity`; result arrives as toast without hanging the draw loop
- **Broker timeouts**: `try_ping` UnixStream gets 3s read/write timeout
- **Silent data-loss fixed**: `do_create_pack` and `do_edit_pack_capabilities` use `?` propagation; YAML parse errors surface as toasts instead of silently overwriting files
- **Focus zones**: `Focus{Sidebar, Content}` — sidebar up/down navigates screens, Enter/→ enters content, Esc/← returns to sidebar; sidebar highlight reflects active zone
- **Breadcrumb header**: replaces filler dashes — shows `clix › Screen › Overlay` updated on every key
- **Toast decoupled from Overlay**: `App::toast_state: Option<ToastState>` floats above all overlays — toasts no longer eject open wizards
- **Confirm-before-discard**: `confirming_discard` flag shows y/n/esc dialog when Esc pressed on dirty wizard; `is_dirty()` on InfisicalSetup, ProfileWizard, CapabilityWizard, PackWizard

---

## Next: TUI completions

### Remaining foot-guns (async — all done)
- [x] `SecretPicker::load()` — async via `LoadSecretFolders`/`LoadSecretNames` WorkPool jobs; spinner while fetching; stale-result rejection by job ID
- [x] `PackWizard` help probes — `ParseHelp` job per binary; `deliver_help()` accumulates results; `finalize_subcmds()` builds checklist when all jobs complete
- [x] `Receipts [A]pprove` — async `ApproveReceipt` WorkRequest; `approving_receipt` guard prevents double-approval; toast on result

### Dirty-tracking (done)
- [x] CapabilityCreate and PackCreate: two-step borrow pattern; Cancel arms check `is_dirty()` and route through `confirming_discard`

### Navigation polish
- [ ] Sidebar focus: `q` should prompt quit only if no dirty overlay exists.
- [ ] Content Esc with `confirming_discard` active should dismiss the confirm dialog, not also move focus to sidebar.
- [ ] Number key shortcuts in sidebar focus should also set `Focus::Content` (already done for digits; verify BackTab/Tab behavior in sidebar focus is intuitive).

### Receipts and Workflows screens
- [ ] Receipts screen (`CLIX_TUI_EXPERIMENTAL=1`) — render real receipt data from `ReceiptStore`; make `[A]pprove` async via `WorkPool`.
- [ ] Workflows screen — execute a workflow from TUI; show step-by-step progress.

---

## Next: Platform & distribution

### macOS sandbox (M9)
- [ ] **SBPL sandbox profile** for macOS subprocess capabilities — designed in architecture; not yet implemented. Replace the current "honest stub" with `sandbox-exec` subprocess wrapping using a generated profile. This gives macOS users real filesystem/network restrictions without full Linux namespaces.
- [ ] `clix doctor` should report the SBPL profile path and whether `sandbox-exec` is available.

### Install & packaging
- [ ] **Homebrew formula** — `clix`, `clix-broker`, `clix-shim` + packs in a single formula; `clix init` becomes a post-install step.
- [ ] **`~/.local/share/clix/packs`** as canonical data dir on Linux (already in seeder search path; needs packaging to copy packs there on install).
- [ ] **Windows PATH shim** — `clix-shim` currently Unix-only; Windows needs a `.cmd` wrapper pattern.

---

## Next: Packs & capabilities

### New packs (candidates)
- [x] `terraform-observe` — `validate`, `plan`, `show`, `state list`, `output` — shipped
- [x] `argocd-observe` — `app list`, `app get`, `app diff`, `app history` — shipped
- [x] `incus-readonly` — `list`, `info`, `snapshot list`, `image list` — shipped
- [ ] `k9s-observe` — pass-through to k9s with `--readonly` flag
- [ ] `pulumi-observe` — `preview`, `stack output`, `stack ls`
- [x] `gcloud-aiplatform` — Vertex AI inspection — shipped (v0.3.0)
- [ ] `npm-observe` — `list`, `audit`, `outdated` — read-only Node.js package inspection

### Pack authoring UX
- [ ] `clix pack onboard` — probe `--help`/`--version` entry points and scaffold a pack; `--json` for structured probe report (designed, not implemented).
- [ ] `clix pack diff` — structured diff between installed and local version.
- [ ] `clix pack publish` — publish to a local registry directory.

---

## Next: Agent integrations

### Demo / onboarding
- [ ] **asciinema demo** (B.8) — record `clix init` + `clix run` + `clix tui` flow for README. Requires real terminal session; not automatable in CI.
- [ ] **claude-code-gcp example** — existing stub in `examples/claude-code-gcp/`; needs real CLAUDE.md + working policy + pack combo that a user can clone and run.

### Two-tool pattern
- [ ] Validate the two-tool export (`clix tools export --format two-tool`) works end-to-end with Claude API `tool_use` loop; add a Python example script.

---

## Maintenance

- [ ] Suppress the 17 `unused import` warnings in `clix-cli` (most are pre-existing stubs from removed features).
- [ ] `clix-testkit` integration suite: add TUI smoke test that opens the TUI, navigates to Secrets, and asserts no hang within 5s on an invalid Infisical URL.
- [ ] Bump `reqwest` and `tokio` to latest stable.
- [ ] `jail_config_digest` is captured in receipts but not yet verified on re-read (SECURITY.md note).
