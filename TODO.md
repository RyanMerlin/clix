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
- **Pack CLI commands**: `clix pack onboard`, `clix pack diff`, `clix pack publish` all implemented

### macOS sandbox
- **`sandbox/macos.rs`**: `sandbox_exec_available()`, `profile_for(SideEffectClass)`, `apply_sandbox()` no-op
- **`run_subprocess_sandboxed()`**: wraps readOnly/none capabilities in `sandbox-exec -p <inline-profile>` on macOS
- **`clix doctor`** reports `sandbox-exec (BETA)` on macOS, `landlock` on Linux

### Compiler health
- **Zero warnings** across clix-cli, clix-core, clix-broker (commit `2ee42fd`)

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
- **Auto-content focus**: Secrets, Dashboard, and Broker screens auto-enter `Focus::Content` on navigation; action keys work immediately without a manual Enter

### Multi-Infisical accounts (Slices 1–3)
- **Named account profiles**: `ClixConfig.infisical_profiles: BTreeMap<String, InfisicalConfig>` + `active_infisical: Option<String>`; legacy single `infisical:` field auto-migrates to `infisical_profiles["default"]` on first load
- **Per-profile keyring slots**: `infisical-client-id:{name}` / `infisical-client-secret:{name}`; first-load migration moves unsuffixed legacy entries to `:default`
- **Token cache per credentials**: `HashMap<(site_url, client_id), CachedToken>` replacing global single-entry
- **`InfisicalProfiles` resolver**: `run_capability`/`run_workflow` take `&InfisicalProfiles` instead of `Option<&InfisicalConfig>`
- **`Overlay::InfisicalAccounts`**: list view with add/edit/remove/set-active/test; `m` opens from Secrets screen
- **Breadcrumb badge**: active Infisical profile name shown in header when set

### Git-backed sync
- **`clix sync {init,pull,push,status}`** — full git sync CLI; `init` links or creates a git repo at `~/.clix` with `.gitignore` (excludes receipts, cache, sockets)
- **TUI Ctrl+G** — full sync (add → commit → rebase → push) from any screen; amber "out of sync" badge when ahead/behind; hidden when clean
- **`git_remote` + `git_branch`** persisted in `config.yaml`

### Storage abstraction baseline (Phase 2 Step A)
- **`trait Storage`** in `clix-core/src/storage/mod.rs`: `read_bytes`, `read_to_string`, `write`, `exists`, `is_dir`, `remove_file`, `remove_dir_all`, `mkdir_p`, `list`, `copy_dir`
- **`FsStorage`**: thin `std::fs` wrapper; default backend
- **`MemStorage`** (`#[cfg(test)]`): in-memory `HashMap<PathBuf, Vec<u8>>`; `ClixState::load_with_storage` for hermetic tests
- **`ClixState.storage: StorageRef`** wired; `state.rs`, `loader.rs`, `tui/app.rs` do_* methods all use trait instead of raw `std::fs`

---

## Next: Secrets tree browser (Slice 4)

- [ ] `widgets/secrets_tree.rs` — async lazy-expanding tree; Browse + Bind modes; `▸`/`▾`/`🔑` glyphs; `WorkRequest::LoadSecretSubtree`.
- [ ] Replace `SecretPicker` with tree browser in profile binding wizard.
- [ ] `b` on Screen::Secrets opens Browse mode for active profile's project.

---

## Next: TUI completions

### Remaining foot-guns (async — all done)
- [x] `SecretPicker::load()` — async via `LoadSecretFolders`/`LoadSecretNames` WorkPool jobs; spinner while fetching; stale-result rejection by job ID
- [x] `PackWizard` help probes — `ParseHelp` job per binary; `deliver_help()` accumulates results; `finalize_subcmds()` builds checklist when all jobs complete
- [x] `Receipts [A]pprove` — async `ApproveReceipt` WorkRequest; `approving_receipt` guard prevents double-approval; toast on result

### Dirty-tracking (done)
- [x] CapabilityCreate and PackCreate: two-step borrow pattern; Cancel arms check `is_dirty()` and route through `confirming_discard`

### Navigation polish (done)
- [x] Sidebar focus: `q` can only fire when no overlay is open — correct behavior already.
- [x] Content Esc with `confirming_discard` active dismisses the dialog only — overlay stays open so next Esc re-routes to handler, not sidebar.
- [x] Tab/BackTab cycle sidebar selection without entering content — correct; Enter/→ enters. Number keys already set Focus::Content.

### Receipts screen (done)
- [x] Receipts screen — loads up to 200 receipts from `ReceiptStore`, table with time/capability/profile/outcome columns, cursor navigation, `[A]pprove` pending via WorkPool, `r` reloads. Removed from STUB_SCREENS.

### Workflows screen
- [x] Workflows screen — left list of workflow names + right step-detail panel; backed by `WorkflowRegistry`; cursor navigation. Shipped commit `2ee42fd`.

---

## Next: Platform & distribution

### macOS sandbox (done)
- [x] `sandbox/macos.rs` — `sandbox_exec_available()`, `profile_for(SideEffectClass)`, `apply_sandbox()` no-op
- [x] `run_subprocess_sandboxed()` in `execution/backends/subprocess.rs` wraps readOnly/none capabilities in `sandbox-exec -p <inline-profile>` on macOS
- [x] `clix doctor` reports sandbox mechanism: `landlock` on Linux, `sandbox-exec` / `sandbox-exec not found` on macOS

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
- [x] `gcloud-aiplatform` — Vertex AI inspection — shipped (v0.3.0)

### Pack authoring UX
- [x] `clix pack onboard` — probes `--help`/`--version`, scaffolds a pack from discovered entry points.
- [x] `clix pack diff` — structured diff between installed and local version.
- [x] `clix pack publish` — publishes to local registry directory (bundles + writes `index.json`).

---

## Next: Agent integrations

### Demo / onboarding
- [ ] **asciinema demo** (B.8) — record `clix init` + `clix run` + `clix tui` flow for README. Requires real terminal session; not automatable in CI.
- [ ] **claude-code-gcp example** — existing stub in `examples/claude-code-gcp/`; needs real CLAUDE.md + working policy + pack combo that a user can clone and run.

### Two-tool pattern
- [ ] Validate the two-tool export (`clix tools export --format two-tool`) works end-to-end with Claude API `tool_use` loop; add a Python example script.

---

## Maintenance

- [x] Suppress compiler warnings — zero warnings across all three crates (commit `2ee42fd`).
- [ ] `clix-testkit` integration suite: add TUI smoke test that opens the TUI, navigates to Secrets, and asserts no hang within 5s on an invalid Infisical URL.
- [ ] Bump `reqwest` and `tokio` to latest stable.
- [ ] `jail_config_digest` is captured in receipts but not yet verified on re-read (SECURITY.md note).
