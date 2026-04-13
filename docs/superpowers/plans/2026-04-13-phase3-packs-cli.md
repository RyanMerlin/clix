# clix Rust Rewrite — Phase 3: Pack Management & CLI

> **For agentic workers:** REQUIRED SUB-SKILL: Use superpowers:subagent-driven-development (recommended) or superpowers:executing-plans to implement this plan task-by-task. Steps use checkbox (`- [ ]`) syntax for tracking.

**Goal:** Implement pack management (install, discover, validate, diff, bundle, scaffold, onboard) in `clix-core`, seed the built-in YAML packs, and build the complete `clix-cli` binary with all subcommands.

**Architecture:** Pack operations are pure `clix-core` functions. The CLI is a thin `clap` v4 derive-based binary that calls `clix-core` and formats output. `--json` flag on all read commands emits machine-readable JSON.

**Prerequisites:** Phases 1 and 2 complete.

**Tech Stack:** clap v4 (derive, env), clap_complete, zip (for bundle/install), sha2 (checksum)

**Spec:** `docs/superpowers/specs/2026-04-13-rust-rewrite-design.md`

---

## File Map

```
crates/clix-core/src/
  packs/
    mod.rs            # re-exports
    install.rs        # install_pack(src) — copy dir or unzip into ~/.clix/packs/
    discover.rs       # discover_pack(path) -> DiscoverReport
    validate.rs       # validate_pack(path) -> Vec<ValidationError>
    diff.rs           # diff_pack(installed_name, new_path) -> DiffReport
    bundle.rs         # bundle_pack(path) -> zip + sha256; publish_pack
    scaffold.rs       # scaffold_pack(name, preset)
    onboard.rs        # onboard_cli(name, command) -> OnboardReport
    seed.rs           # seed_builtin_packs(state)
  loader.rs           # build_registry(state), build_workflow_registry(state)

packs/                # built-in YAML packs (committed to git)
  base/pack.yaml
  base/capabilities/system.date.yaml
  base/capabilities/system.echo.yaml
  base/profiles/base.yaml
  gcloud-readonly/pack.yaml
  gcloud-readonly/capabilities/...
  kubectl-observe/pack.yaml
  ...

crates/clix-cli/
  Cargo.toml
  src/
    main.rs
    cli.rs            # Cli struct, top-level clap derive
    output.rs         # print_json(), print_table() helpers
    commands/
      mod.rs
      init.rs
      status.rs
      run.rs
      capabilities.rs
      workflow.rs
      profile.rs
      receipts.rs
      serve.rs
      pack.rs
```

---

### Task 1: Pack install and discover

**Files:**
- Create: `crates/clix-core/src/packs/mod.rs`
- Create: `crates/clix-core/src/packs/install.rs`
- Create: `crates/clix-core/src/packs/discover.rs`

- [ ] **Step 1: Add zip and sha2 to workspace**

Add to root `Cargo.toml` `[workspace.dependencies]`:
```toml
zip  = "2"
sha2 = "0.10"
hex  = "0.4"
```

Add to `crates/clix-core/Cargo.toml`:
```toml
zip  = { workspace = true }
sha2 = { workspace = true }
hex  = { workspace = true }
```

- [ ] **Step 2: Write failing test**

`crates/clix-core/src/packs/install.rs`:
```rust
#[cfg(test)]
mod tests {
    use super::*;
    use std::fs;
    use tempfile::TempDir;

    #[test]
    fn test_install_from_directory() {
        let src = TempDir::new().unwrap();
        // Create a minimal pack
        fs::write(src.path().join("pack.yaml"), "name: test-pack\nversion: 1\n").unwrap();
        let dest = TempDir::new().unwrap();
        install_pack(src.path(), dest.path()).unwrap();
        assert!(dest.path().join("test-pack").join("pack.yaml").exists());
    }
}
```

- [ ] **Step 3: Run — expect compile failure**

```bash
cargo test -p clix-core packs 2>&1 | head -5
```

- [ ] **Step 4: Add tempfile dev-dependency**

Add to `crates/clix-core/Cargo.toml`:
```toml
[dev-dependencies]
tempfile = "3"
```

- [ ] **Step 5: Implement install.rs**

`crates/clix-core/src/packs/install.rs`:
```rust
use std::path::{Path, PathBuf};
use crate::error::{ClixError, Result};
use crate::manifest::loader::load_manifest;
use crate::manifest::pack::PackManifest;

/// Install a pack from a source directory or .clixpack.zip archive into packs_dir.
/// Returns the installed pack directory path.
pub fn install_pack(src: &Path, packs_dir: &Path) -> Result<PathBuf> {
    if src.is_file() {
        install_from_zip(src, packs_dir)
    } else if src.is_dir() {
        install_from_dir(src, packs_dir)
    } else {
        Err(ClixError::Pack(format!("pack source not found: {}", src.display())))
    }
}

fn install_from_dir(src: &Path, packs_dir: &Path) -> Result<PathBuf> {
    // Load pack manifest to get the name
    let manifest_path = src.join("pack.yaml")
        .or_else_if_missing(|| src.join("pack.json"));
    let manifest: PackManifest = load_manifest(&manifest_path)?;
    let dest = packs_dir.join(&manifest.name);
    copy_dir_all(src, &dest)?;
    Ok(dest)
}

trait OrElseIfMissing {
    fn or_else_if_missing(self, f: impl FnOnce() -> PathBuf) -> PathBuf;
}
impl OrElseIfMissing for PathBuf {
    fn or_else_if_missing(self, f: impl FnOnce() -> PathBuf) -> PathBuf {
        if self.exists() { self } else { f() }
    }
}

fn install_from_zip(zip_path: &Path, packs_dir: &Path) -> Result<PathBuf> {
    // Verify .sha256 sidecar if present
    let sha_path = zip_path.with_extension("clixpack.sha256");
    if sha_path.exists() {
        verify_checksum(zip_path, &sha_path)?;
    }

    let file = std::fs::File::open(zip_path)?;
    let mut archive = zip::ZipArchive::new(file)
        .map_err(|e| ClixError::Pack(format!("zip open: {e}")))?;

    // Read bundle.json to get pack name
    let pack_name = read_pack_name_from_zip(&mut archive)?;
    let dest = packs_dir.join(&pack_name);
    std::fs::create_dir_all(&dest)?;

    for i in 0..archive.len() {
        let mut file = archive.by_index(i)
            .map_err(|e| ClixError::Pack(format!("zip entry: {e}")))?;
        let out_path = dest.join(file.name());
        if file.name().ends_with('/') {
            std::fs::create_dir_all(&out_path)?;
        } else {
            if let Some(parent) = out_path.parent() {
                std::fs::create_dir_all(parent)?;
            }
            let mut out = std::fs::File::create(&out_path)?;
            std::io::copy(&mut file, &mut out)?;
        }
    }
    Ok(dest)
}

fn read_pack_name_from_zip(archive: &mut zip::ZipArchive<std::fs::File>) -> Result<String> {
    // Try pack.yaml, then pack.json at root level
    for name in ["pack.yaml", "pack.yml", "pack.json"] {
        if let Ok(mut f) = archive.by_name(name) {
            let mut buf = String::new();
            std::io::Read::read_to_string(&mut f, &mut buf)?;
            let manifest: PackManifest = if name.ends_with(".json") {
                serde_json::from_str(&buf)?
            } else {
                serde_yaml::from_str(&buf)?
            };
            return Ok(manifest.name);
        }
    }
    Err(ClixError::Pack("pack.yaml not found in archive".to_string()))
}

fn verify_checksum(zip_path: &Path, sha_path: &Path) -> Result<()> {
    use sha2::{Sha256, Digest};
    let data = std::fs::read(zip_path)?;
    let mut hasher = Sha256::new();
    hasher.update(&data);
    let actual = hex::encode(hasher.finalize());
    let expected = std::fs::read_to_string(sha_path)?.trim().to_string();
    let expected_hash = expected.split_whitespace().next().unwrap_or(&expected);
    if actual != expected_hash {
        return Err(ClixError::Pack(format!(
            "checksum mismatch: expected {expected_hash}, got {actual}"
        )));
    }
    Ok(())
}

pub fn copy_dir_all(src: &Path, dst: &Path) -> Result<()> {
    std::fs::create_dir_all(dst)?;
    for entry in std::fs::read_dir(src)? {
        let entry = entry?;
        let dst_path = dst.join(entry.file_name());
        if entry.file_type()?.is_dir() {
            copy_dir_all(&entry.path(), &dst_path)?;
        } else {
            std::fs::copy(entry.path(), dst_path)?;
        }
    }
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::fs;
    use tempfile::TempDir;

    #[test]
    fn test_install_from_directory() {
        let src = TempDir::new().unwrap();
        fs::write(src.path().join("pack.yaml"), "name: test-pack\nversion: 1\n").unwrap();
        let dest = TempDir::new().unwrap();
        install_pack(src.path(), dest.path()).unwrap();
        assert!(dest.path().join("test-pack").join("pack.yaml").exists());
    }
}
```

- [ ] **Step 6: Implement discover.rs**

`crates/clix-core/src/packs/discover.rs`:
```rust
use std::path::Path;
use serde::Serialize;
use crate::error::Result;
use crate::manifest::loader::load_manifest;
use crate::manifest::pack::PackManifest;
use crate::manifest::capability::CapabilityManifest;
use crate::manifest::profile::ProfileManifest;
use crate::manifest::workflow::WorkflowManifest;
use crate::manifest::loader::load_dir;

#[derive(Debug, Serialize)]
pub struct DiscoverReport {
    pub pack: PackManifest,
    pub profiles: Vec<ProfileManifest>,
    pub capabilities: Vec<CapabilityManifest>,
    pub workflows: Vec<WorkflowManifest>,
    pub warnings: Vec<String>,
}

/// Inspect a pack directory without installing it.
pub fn discover_pack(path: &Path) -> Result<DiscoverReport> {
    let mut warnings = vec![];

    let manifest_path = ["pack.yaml", "pack.yml", "pack.json"]
        .iter()
        .map(|f| path.join(f))
        .find(|p| p.exists())
        .ok_or_else(|| crate::error::ClixError::Pack(
            format!("no pack.yaml found in {}", path.display())
        ))?;

    let pack: PackManifest = load_manifest(&manifest_path)?;

    let profiles: Vec<ProfileManifest> = load_dir(&path.join("profiles"))
        .unwrap_or_else(|e| { warnings.push(format!("profiles: {e}")); vec![] });
    let capabilities: Vec<CapabilityManifest> = load_dir(&path.join("capabilities"))
        .unwrap_or_else(|e| { warnings.push(format!("capabilities: {e}")); vec![] });
    let workflows: Vec<WorkflowManifest> = load_dir(&path.join("workflows"))
        .unwrap_or_else(|e| { warnings.push(format!("workflows: {e}")); vec![] });

    Ok(DiscoverReport { pack, profiles, capabilities, workflows, warnings })
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::fs;
    use tempfile::TempDir;

    #[test]
    fn test_discover_minimal_pack() {
        let dir = TempDir::new().unwrap();
        fs::write(dir.path().join("pack.yaml"), "name: my-pack\nversion: 1\n").unwrap();
        let report = discover_pack(dir.path()).unwrap();
        assert_eq!(report.pack.name, "my-pack");
        assert!(report.capabilities.is_empty());
    }
}
```

- [ ] **Step 7: Create packs/mod.rs**

`crates/clix-core/src/packs/mod.rs`:
```rust
pub mod bundle;
pub mod discover;
pub mod install;
pub mod onboard;
pub mod scaffold;
pub mod seed;
pub mod validate;
pub mod diff;

pub use discover::{discover_pack, DiscoverReport};
pub use install::install_pack;
pub use bundle::{bundle_pack, publish_pack};
pub use validate::validate_pack;
pub use diff::{diff_pack, DiffReport};
pub use scaffold::scaffold_pack;
pub use onboard::{onboard_cli, OnboardReport};
pub use seed::seed_builtin_packs;
```

Create stub files for the others (implement in next steps):
```bash
touch crates/clix-core/src/packs/bundle.rs
touch crates/clix-core/src/packs/validate.rs
touch crates/clix-core/src/packs/diff.rs
touch crates/clix-core/src/packs/scaffold.rs
touch crates/clix-core/src/packs/onboard.rs
touch crates/clix-core/src/packs/seed.rs
```

- [ ] **Step 8: Add to lib.rs**

```rust
pub mod packs;
```

- [ ] **Step 9: Run tests**

```bash
cargo test -p clix-core packs
```
Expected: 2 tests pass (install + discover)

- [ ] **Step 10: Commit**

```bash
git add crates/clix-core/src/packs/ crates/clix-core/src/lib.rs
git commit -m "feat(core): add pack install and discover"
```

---

### Task 2: Pack validate, diff, bundle, scaffold

**Files:**
- Modify: `crates/clix-core/src/packs/validate.rs`
- Modify: `crates/clix-core/src/packs/diff.rs`
- Modify: `crates/clix-core/src/packs/bundle.rs`
- Modify: `crates/clix-core/src/packs/scaffold.rs`

- [ ] **Step 1: Implement validate.rs**

`crates/clix-core/src/packs/validate.rs`:
```rust
use std::path::Path;
use crate::error::Result;
use super::discover::discover_pack;

#[derive(Debug)]
pub struct ValidationError {
    pub path: String,
    pub message: String,
}

/// Validate a pack directory: schema correctness, required files.
/// Returns list of errors (empty = valid).
pub fn validate_pack(path: &Path) -> Result<Vec<ValidationError>> {
    let mut errors = vec![];
    match discover_pack(path) {
        Err(e) => errors.push(ValidationError {
            path: path.display().to_string(),
            message: e.to_string(),
        }),
        Ok(report) => {
            // Warn on packs with no capabilities and no profiles
            if report.capabilities.is_empty() && report.profiles.is_empty() {
                errors.push(ValidationError {
                    path: "pack.yaml".to_string(),
                    message: "pack defines no capabilities and no profiles".to_string(),
                });
            }
            for warning in report.warnings {
                errors.push(ValidationError {
                    path: path.display().to_string(),
                    message: warning,
                });
            }
        }
    }
    Ok(errors)
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::fs;
    use tempfile::TempDir;

    #[test]
    fn test_validate_empty_pack_warns() {
        let dir = TempDir::new().unwrap();
        fs::write(dir.path().join("pack.yaml"), "name: empty\nversion: 1\n").unwrap();
        let errs = validate_pack(dir.path()).unwrap();
        assert!(!errs.is_empty());
        assert!(errs[0].message.contains("no capabilities"));
    }

    #[test]
    fn test_validate_missing_pack_yaml_errors() {
        let dir = TempDir::new().unwrap();
        let errs = validate_pack(dir.path()).unwrap();
        assert!(!errs.is_empty());
    }
}
```

- [ ] **Step 2: Implement diff.rs**

`crates/clix-core/src/packs/diff.rs`:
```rust
use std::path::Path;
use serde::Serialize;
use crate::error::Result;
use super::discover::discover_pack;

#[derive(Debug, Serialize)]
pub struct DiffReport {
    pub pack_name: String,
    pub version_change: Option<(u32, u32)>,
    pub capabilities_added: Vec<String>,
    pub capabilities_removed: Vec<String>,
    pub capabilities_changed: Vec<String>,
    pub profiles_added: Vec<String>,
    pub profiles_removed: Vec<String>,
    pub workflows_added: Vec<String>,
    pub workflows_removed: Vec<String>,
}

/// Compare an installed pack (by its installed directory path) with a new pack source.
pub fn diff_pack(installed: &Path, new_src: &Path) -> Result<DiffReport> {
    let old = discover_pack(installed)?;
    let new = discover_pack(new_src)?;

    let old_caps: std::collections::HashSet<_> = old.capabilities.iter().map(|c| c.name.clone()).collect();
    let new_caps: std::collections::HashSet<_> = new.capabilities.iter().map(|c| c.name.clone()).collect();

    let old_profiles: std::collections::HashSet<_> = old.profiles.iter().map(|p| p.name.clone()).collect();
    let new_profiles: std::collections::HashSet<_> = new.profiles.iter().map(|p| p.name.clone()).collect();

    let old_wf: std::collections::HashSet<_> = old.workflows.iter().map(|w| w.name.clone()).collect();
    let new_wf: std::collections::HashSet<_> = new.workflows.iter().map(|w| w.name.clone()).collect();

    // Capabilities changed = same name but different version
    let changed: Vec<String> = old.capabilities.iter()
        .filter_map(|old_cap| {
            new.capabilities.iter().find(|nc| nc.name == old_cap.name && nc.version != old_cap.version)
                .map(|_| old_cap.name.clone())
        })
        .collect();

    Ok(DiffReport {
        pack_name: old.pack.name.clone(),
        version_change: if old.pack.version != new.pack.version {
            Some((old.pack.version, new.pack.version))
        } else {
            None
        },
        capabilities_added:   new_caps.difference(&old_caps).cloned().collect(),
        capabilities_removed: old_caps.difference(&new_caps).cloned().collect(),
        capabilities_changed: changed,
        profiles_added:       new_profiles.difference(&old_profiles).cloned().collect(),
        profiles_removed:     old_profiles.difference(&new_profiles).cloned().collect(),
        workflows_added:      new_wf.difference(&old_wf).cloned().collect(),
        workflows_removed:    old_wf.difference(&new_wf).cloned().collect(),
    })
}
```

- [ ] **Step 3: Implement bundle.rs**

`crates/clix-core/src/packs/bundle.rs`:
```rust
use std::path::{Path, PathBuf};
use sha2::{Sha256, Digest};
use crate::error::{ClixError, Result};
use super::install::copy_dir_all;
use crate::manifest::loader::load_manifest;
use crate::manifest::pack::PackManifest;

/// Bundle a pack directory into a .clixpack.zip archive with a .sha256 sidecar.
/// Returns the path to the created zip.
pub fn bundle_pack(pack_path: &Path, out_dir: &Path) -> Result<PathBuf> {
    let manifest_path = ["pack.yaml", "pack.yml", "pack.json"]
        .iter()
        .map(|f| pack_path.join(f))
        .find(|p| p.exists())
        .ok_or_else(|| ClixError::Pack("pack.yaml not found".to_string()))?;
    let manifest: PackManifest = load_manifest(&manifest_path)?;

    std::fs::create_dir_all(out_dir)?;
    let zip_name = format!("{}-v{}.clixpack.zip", manifest.name, manifest.version);
    let zip_path = out_dir.join(&zip_name);

    let file = std::fs::File::create(&zip_path)?;
    let mut zip = zip::ZipWriter::new(file);
    let options = zip::write::SimpleFileOptions::default()
        .compression_method(zip::CompressionMethod::Deflated);

    add_dir_to_zip(&mut zip, pack_path, pack_path, &options)?;
    zip.finish().map_err(|e| ClixError::Pack(format!("zip finish: {e}")))?;

    // Write .sha256 sidecar
    let data = std::fs::read(&zip_path)?;
    let mut hasher = Sha256::new();
    hasher.update(&data);
    let checksum = hex::encode(hasher.finalize());
    let sha_path = zip_path.with_extension("clixpack.sha256");
    std::fs::write(&sha_path, format!("{checksum}  {zip_name}\n"))?;

    Ok(zip_path)
}

fn add_dir_to_zip(
    zip: &mut zip::ZipWriter<std::fs::File>,
    base: &Path,
    current: &Path,
    options: &zip::write::SimpleFileOptions,
) -> Result<()> {
    for entry in std::fs::read_dir(current)? {
        let entry = entry?;
        let path = entry.path();
        let name = path.strip_prefix(base).unwrap().to_str().unwrap().replace('\\', "/");
        if path.is_dir() {
            zip.add_directory(&name, *options)
                .map_err(|e| ClixError::Pack(format!("zip dir: {e}")))?;
            add_dir_to_zip(zip, base, &path, options)?;
        } else {
            zip.start_file(&name, *options)
                .map_err(|e| ClixError::Pack(format!("zip file: {e}")))?;
            let mut f = std::fs::File::open(&path)?;
            std::io::copy(&mut f, zip)?;
        }
    }
    Ok(())
}

/// Copy a bundle archive to ~/.clix/bundles/published/ and update index.json.
pub fn publish_pack(zip_path: &Path, bundles_dir: &Path) -> Result<()> {
    let published = bundles_dir.join("published");
    std::fs::create_dir_all(&published)?;
    let dest = published.join(zip_path.file_name().unwrap());
    std::fs::copy(zip_path, &dest)?;

    // Update index.json
    let index_path = published.join("index.json");
    let mut index: Vec<String> = if index_path.exists() {
        serde_json::from_str(&std::fs::read_to_string(&index_path)?).unwrap_or_default()
    } else {
        vec![]
    };
    let entry = zip_path.file_name().unwrap().to_string_lossy().to_string();
    if !index.contains(&entry) {
        index.push(entry);
    }
    std::fs::write(&index_path, serde_json::to_string_pretty(&index)?)?;
    Ok(())
}
```

- [ ] **Step 4: Implement scaffold.rs**

`crates/clix-core/src/packs/scaffold.rs`:
```rust
use std::path::{Path, PathBuf};
use crate::error::Result;

#[derive(Debug, Clone, PartialEq)]
pub enum Preset { ReadOnly, ChangeControlled, Operator }

impl std::str::FromStr for Preset {
    type Err = String;
    fn from_str(s: &str) -> std::result::Result<Self, Self::Err> {
        match s {
            "read-only" | "readonly"          => Ok(Preset::ReadOnly),
            "change-controlled" | "change"    => Ok(Preset::ChangeControlled),
            "operator"                        => Ok(Preset::Operator),
            _ => Err(format!("unknown preset: {s} (use: read-only, change-controlled, operator)")),
        }
    }
}

/// Generate a minimal pack scaffold in out_dir/<name>/.
pub fn scaffold_pack(name: &str, preset: Preset, command: Option<&str>, out_dir: &Path) -> Result<PathBuf> {
    let pack_dir = out_dir.join(name);
    std::fs::create_dir_all(pack_dir.join("capabilities"))?;
    std::fs::create_dir_all(pack_dir.join("profiles"))?;
    std::fs::create_dir_all(pack_dir.join("workflows"))?;

    let cmd = command.unwrap_or(name);

    // pack.yaml
    std::fs::write(pack_dir.join("pack.yaml"), format!(
        "name: {name}\nversion: 1\ndescription: '{name} pack'\nprofiles:\n  - {name}\n"
    ))?;

    // profile
    std::fs::write(pack_dir.join("profiles").join(format!("{name}.yaml")), format!(
        "name: {name}\nversion: 1\ncapabilities:\n  - {name}.version\n"
    ))?;

    // capability based on preset
    let (cap_name, cap_content) = match preset {
        Preset::ReadOnly => (
            format!("{name}.version"),
            format!(
                "name: {name}.version\nversion: 1\ndescription: Show {cmd} version\nbackend:\n  type: subprocess\n  command: {cmd}\n  args: [\"--version\"]\nrisk: low\nsideEffectClass: readOnly\ninputSchema:\n  type: object\n  properties: {{}}\n"
            ),
        ),
        Preset::ChangeControlled => (
            format!("{name}.apply"),
            format!(
                "name: {name}.apply\nversion: 1\ndescription: Apply changes with {cmd}\nbackend:\n  type: subprocess\n  command: {cmd}\n  args: [\"apply\", \"-f\", \"{{ input.file }}\"]\nrisk: high\nsideEffectClass: mutating\napprovalPolicy: require\ninputSchema:\n  type: object\n  properties:\n    file:\n      type: string\n  required: [file]\n"
            ),
        ),
        Preset::Operator => (
            format!("{name}.status"),
            format!(
                "name: {name}.status\nversion: 1\ndescription: Show {cmd} status\nbackend:\n  type: subprocess\n  command: {cmd}\n  args: [\"status\"]\nrisk: low\nsideEffectClass: readOnly\ninputSchema:\n  type: object\n  properties: {{}}\n"
            ),
        ),
    };

    std::fs::write(pack_dir.join("capabilities").join(format!("{cap_name}.yaml")), cap_content)?;

    Ok(pack_dir)
}

#[cfg(test)]
mod tests {
    use super::*;
    use tempfile::TempDir;

    #[test]
    fn test_scaffold_readonly() {
        let dir = TempDir::new().unwrap();
        let pack_dir = scaffold_pack("mytool", Preset::ReadOnly, Some("mytool"), dir.path()).unwrap();
        assert!(pack_dir.join("pack.yaml").exists());
        assert!(pack_dir.join("profiles").join("mytool.yaml").exists());
        assert!(pack_dir.join("capabilities").join("mytool.version.yaml").exists());
    }
}
```

- [ ] **Step 5: Run tests**

```bash
cargo test -p clix-core packs
```
Expected: 5+ tests pass

- [ ] **Step 6: Commit**

```bash
git add crates/clix-core/src/packs/
git commit -m "feat(core): add pack validate, diff, bundle, scaffold"
```

---

### Task 3: Pack onboard and seed

**Files:**
- Modify: `crates/clix-core/src/packs/onboard.rs`
- Modify: `crates/clix-core/src/packs/seed.rs`

- [ ] **Step 1: Implement onboard.rs**

`crates/clix-core/src/packs/onboard.rs`:
```rust
use std::path::Path;
use serde::Serialize;
use chrono::{DateTime, Utc};
use crate::error::Result;
use crate::manifest::capability::CapabilityManifest;
use super::scaffold::{scaffold_pack, Preset};

#[derive(Debug, Serialize)]
pub struct OnboardReport {
    pub cli: String,
    pub probed_at: DateTime<Utc>,
    pub version_output: Option<String>,
    pub help_sections: Vec<String>,
    pub inferred_subcommands: Vec<String>,
    pub suggested_preset: String,
    pub confidence: f32,
    pub suggested_capabilities: Vec<CapabilityManifest>,
    pub warnings: Vec<String>,
    pub scaffold_path: Option<std::path::PathBuf>,
}

/// Probe a CLI binary and generate a pack scaffold + OnboardReport.
pub fn onboard_cli(
    pack_name: &str,
    command: &str,
    out_dir: &Path,
) -> Result<OnboardReport> {
    let probed_at = Utc::now();
    let mut warnings = vec![];

    // Probe version
    let version_output = probe_command(command, &["--version"])
        .or_else(|_| probe_command(command, &["version"]))
        .ok();

    // Probe help
    let help_output = probe_command(command, &["--help"])
        .or_else(|_| probe_command(command, &["help"]))
        .unwrap_or_default();

    // Infer subcommands from help output (lines that look like "  subcommand   description")
    let subcommands = infer_subcommands(&help_output);

    // Infer preset: if help mentions "apply", "destroy", "delete" => change-controlled
    // if mentions "reconcile", "sync" => operator; else read-only
    let (preset, confidence) = infer_preset(&help_output, &subcommands);

    // Generate scaffold
    let scaffold_path = scaffold_pack(pack_name, preset.clone(), Some(command), out_dir).ok();
    if scaffold_path.is_none() {
        warnings.push("failed to generate scaffold".to_string());
    }

    Ok(OnboardReport {
        cli: command.to_string(),
        probed_at,
        version_output,
        help_sections: vec![help_output],
        inferred_subcommands: subcommands,
        suggested_preset: format!("{preset:?}").to_lowercase(),
        confidence,
        suggested_capabilities: vec![],
        warnings,
        scaffold_path,
    })
}

fn probe_command(command: &str, args: &[&str]) -> Result<String> {
    let output = std::process::Command::new(command)
        .args(args)
        .output()
        .map_err(|e| crate::error::ClixError::Backend(e.to_string()))?;
    Ok(String::from_utf8_lossy(&output.stdout).to_string()
        + &String::from_utf8_lossy(&output.stderr))
}

fn infer_subcommands(help: &str) -> Vec<String> {
    let mut cmds = vec![];
    for line in help.lines() {
        let trimmed = line.trim();
        // Lines like "  get       Get a resource" or "  apply      Apply changes"
        if trimmed.starts_with(|c: char| c.is_lowercase()) && trimmed.contains("  ") {
            let word = trimmed.split_whitespace().next().unwrap_or("");
            if !word.is_empty() && word.len() < 20 && !word.starts_with('-') {
                cmds.push(word.to_string());
            }
        }
    }
    cmds.dedup();
    cmds
}

fn infer_preset(help: &str, subcommands: &[String]) -> (Preset, f32) {
    let lower = help.to_lowercase();
    let has_destructive = subcommands.iter().any(|s| matches!(s.as_str(), "apply" | "delete" | "destroy" | "rm" | "remove"))
        || lower.contains("apply") || lower.contains("destroy") || lower.contains("delete");
    let has_operator = lower.contains("reconcile") || lower.contains("sync") || lower.contains("deploy");

    if has_destructive {
        (Preset::ChangeControlled, 0.7)
    } else if has_operator {
        (Preset::Operator, 0.65)
    } else {
        (Preset::ReadOnly, 0.8)
    }
}
```

- [ ] **Step 2: Implement seed.rs**

`crates/clix-core/src/packs/seed.rs`:
```rust
use std::path::Path;
use crate::error::Result;
use super::install::copy_dir_all;

/// Seed the built-in packs from the embedded packs directory into packs_dir.
/// Built-in packs are already installed if the directory exists.
pub fn seed_builtin_packs(packs_dir: &Path, builtin_packs_src: &Path) -> Result<()> {
    if !builtin_packs_src.exists() {
        return Ok(());
    }
    for entry in std::fs::read_dir(builtin_packs_src)? {
        let entry = entry?;
        if !entry.file_type()?.is_dir() {
            continue;
        }
        let pack_name = entry.file_name();
        let dest = packs_dir.join(&pack_name);
        if !dest.exists() {
            copy_dir_all(&entry.path(), &dest)?;
        }
    }
    Ok(())
}
```

- [ ] **Step 3: Commit**

```bash
git add crates/clix-core/src/packs/onboard.rs crates/clix-core/src/packs/seed.rs
git commit -m "feat(core): add pack onboard and seed"
```

---

### Task 4: Built-in pack YAML files

**Files:**
- Create: `packs/base/pack.yaml`
- Create: `packs/base/profiles/base.yaml`
- Create: `packs/base/capabilities/system.date.yaml`
- Create: `packs/base/capabilities/system.echo.yaml`
- Create: `packs/kubectl-observe/pack.yaml`
- Create: `packs/kubectl-observe/profiles/kubectl-observe.yaml`
- Create: `packs/kubectl-observe/capabilities/kubectl.get-pods.yaml`
- Create: `packs/kubectl-observe/capabilities/kubectl.get-nodes.yaml`
- Create: `packs/kubectl-observe/capabilities/kubectl.get-namespaces.yaml`
- Create: `packs/kubectl-observe/workflows/kubectl.cluster-health.yaml`
- Create: `packs/gcloud-readonly/pack.yaml`
- Create: `packs/gcloud-readonly/profiles/gcloud-readonly.yaml`
- Create: `packs/gcloud-readonly/capabilities/gcloud.list-projects.yaml`
- Create: `packs/gh-readonly/pack.yaml`
- Create: `packs/gh-readonly/profiles/gh-readonly.yaml`
- Create: `packs/gh-readonly/capabilities/gh.list-repos.yaml`

- [ ] **Step 1: Create base pack**

```bash
mkdir -p packs/base/capabilities packs/base/profiles
```

`packs/base/pack.yaml`:
```yaml
name: base
version: 1
description: Shared safe defaults — builtins only
profiles:
  - base
```

`packs/base/profiles/base.yaml`:
```yaml
name: base
version: 1
description: Safe builtin capabilities
capabilities:
  - system.date
  - system.echo
```

`packs/base/capabilities/system.date.yaml`:
```yaml
name: system.date
version: 1
description: Return the current UTC date and time
backend:
  type: builtin
  name: date
risk: low
sideEffectClass: none
inputSchema:
  type: object
  properties: {}
```

`packs/base/capabilities/system.echo.yaml`:
```yaml
name: system.echo
version: 1
description: Echo a message back
backend:
  type: builtin
  name: echo
risk: low
sideEffectClass: none
inputSchema:
  type: object
  properties:
    message:
      type: string
  required: [message]
```

- [ ] **Step 2: Create kubectl-observe pack**

```bash
mkdir -p packs/kubectl-observe/capabilities packs/kubectl-observe/profiles packs/kubectl-observe/workflows
```

`packs/kubectl-observe/pack.yaml`:
```yaml
name: kubectl-observe
version: 1
description: Read-only kubectl inspection
profiles:
  - kubectl-observe
```

`packs/kubectl-observe/profiles/kubectl-observe.yaml`:
```yaml
name: kubectl-observe
version: 1
description: Read-only kubectl inspection
capabilities:
  - kubectl.get-pods
  - kubectl.get-nodes
  - kubectl.get-namespaces
workflows:
  - kubectl.cluster-health
```

`packs/kubectl-observe/capabilities/kubectl.get-pods.yaml`:
```yaml
name: kubectl.get-pods
version: 1
description: List pods in a namespace
backend:
  type: subprocess
  command: kubectl
  args: ["get", "pods", "-n", "{{ input.namespace }}", "--output=json"]
risk: low
sideEffectClass: readOnly
inputSchema:
  type: object
  properties:
    namespace:
      type: string
      description: Kubernetes namespace
  required: [namespace]
```

`packs/kubectl-observe/capabilities/kubectl.get-nodes.yaml`:
```yaml
name: kubectl.get-nodes
version: 1
description: List cluster nodes
backend:
  type: subprocess
  command: kubectl
  args: ["get", "nodes", "--output=json"]
risk: low
sideEffectClass: readOnly
inputSchema:
  type: object
  properties: {}
```

`packs/kubectl-observe/capabilities/kubectl.get-namespaces.yaml`:
```yaml
name: kubectl.get-namespaces
version: 1
description: List all namespaces
backend:
  type: subprocess
  command: kubectl
  args: ["get", "namespaces", "--output=json"]
risk: low
sideEffectClass: readOnly
inputSchema:
  type: object
  properties: {}
```

`packs/kubectl-observe/workflows/kubectl.cluster-health.yaml`:
```yaml
name: kubectl.cluster-health
version: 1
description: Check cluster health — nodes then system pods
steps:
  - capability: kubectl.get-nodes
    input: {}
  - capability: kubectl.get-pods
    input:
      namespace: kube-system
    onFailure: continue
```

- [ ] **Step 3: Create gcloud-readonly pack**

```bash
mkdir -p packs/gcloud-readonly/capabilities packs/gcloud-readonly/profiles
```

`packs/gcloud-readonly/pack.yaml`:
```yaml
name: gcloud-readonly
version: 1
description: Read-only gcloud planning capabilities
profiles:
  - gcloud-readonly
```

`packs/gcloud-readonly/profiles/gcloud-readonly.yaml`:
```yaml
name: gcloud-readonly
version: 1
capabilities:
  - gcloud.list-projects
```

`packs/gcloud-readonly/capabilities/gcloud.list-projects.yaml`:
```yaml
name: gcloud.list-projects
version: 1
description: List GCP projects
backend:
  type: subprocess
  command: gcloud
  args: ["projects", "list", "--format=json"]
risk: low
sideEffectClass: readOnly
inputSchema:
  type: object
  properties: {}
```

- [ ] **Step 4: Create gh-readonly pack**

```bash
mkdir -p packs/gh-readonly/capabilities packs/gh-readonly/profiles
```

`packs/gh-readonly/pack.yaml`:
```yaml
name: gh-readonly
version: 1
description: Read-only GitHub CLI capabilities
profiles:
  - gh-readonly
```

`packs/gh-readonly/profiles/gh-readonly.yaml`:
```yaml
name: gh-readonly
version: 1
capabilities:
  - gh.list-repos
```

`packs/gh-readonly/capabilities/gh.list-repos.yaml`:
```yaml
name: gh.list-repos
version: 1
description: List GitHub repositories for the authenticated user
backend:
  type: subprocess
  command: gh
  args: ["repo", "list", "--json", "name,description,url", "--limit", "{{ input.limit | default(value=30) }}"]
risk: low
sideEffectClass: readOnly
inputSchema:
  type: object
  properties:
    limit:
      type: integer
      default: 30
```

- [ ] **Step 5: Verify packs load via discover**

```bash
cargo test -p clix-core packs::discover
# Then manually verify with:
cargo run -p clix-cli -- pack discover packs/base
```

- [ ] **Step 6: Commit**

```bash
git add packs/
git commit -m "feat: add built-in YAML packs (base, kubectl-observe, gcloud-readonly, gh-readonly)"
```

---

### Task 5: Registry loader (build_registry from state)

**Files:**
- Create: `crates/clix-core/src/loader.rs`

- [ ] **Step 1: Implement loader.rs**

`crates/clix-core/src/loader.rs`:
```rust
use crate::error::Result;
use crate::manifest::loader::{load_dir, load_manifest};
use crate::manifest::pack::PackManifest;
use crate::policy::PolicyBundle;
use crate::registry::{CapabilityRegistry, WorkflowRegistry};
use crate::state::ClixState;

/// Build a CapabilityRegistry from the active profiles in state.
/// Loads capabilities from: ~/.clix/capabilities/, each installed pack's capabilities/.
pub fn build_registry(state: &ClixState) -> Result<CapabilityRegistry> {
    let mut all_caps = load_dir(&state.capabilities_dir)?;

    // Load from installed packs
    if state.packs_dir.exists() {
        for entry in std::fs::read_dir(&state.packs_dir)? {
            let entry = entry?;
            if entry.file_type()?.is_dir() {
                let cap_dir = entry.path().join("capabilities");
                let mut pack_caps = load_dir(&cap_dir)?;
                all_caps.append(&mut pack_caps);
            }
        }
    }

    // Filter to only active profiles if profiles are configured
    let caps = if state.config.active_profiles.is_empty() {
        all_caps
    } else {
        let active_profiles = load_active_profiles(state)?;
        let allowed_caps: std::collections::HashSet<String> = active_profiles
            .iter()
            .flat_map(|p| p.capabilities.iter().cloned())
            .collect();
        if allowed_caps.is_empty() {
            all_caps // profiles defined but no capabilities listed — allow all
        } else {
            all_caps.into_iter().filter(|c| allowed_caps.contains(&c.name)).collect()
        }
    };

    Ok(CapabilityRegistry::from_vec(caps))
}

/// Build a WorkflowRegistry from installed packs and ~/.clix/workflows/.
pub fn build_workflow_registry(state: &ClixState) -> Result<WorkflowRegistry> {
    let mut all = load_dir(&state.workflows_dir)?;
    if state.packs_dir.exists() {
        for entry in std::fs::read_dir(&state.packs_dir)? {
            let entry = entry?;
            if entry.file_type()?.is_dir() {
                let wf_dir = entry.path().join("workflows");
                let mut wfs = load_dir(&wf_dir)?;
                all.append(&mut wfs);
            }
        }
    }
    Ok(WorkflowRegistry::from_vec(all))
}

/// Load PolicyBundle from ~/.clix/policy.yaml.
pub fn load_policy(state: &ClixState) -> Result<PolicyBundle> {
    if state.policy_path.exists() {
        let content = std::fs::read_to_string(&state.policy_path)?;
        Ok(serde_yaml::from_str(&content)?)
    } else {
        Ok(PolicyBundle::default())
    }
}

fn load_active_profiles(
    state: &ClixState,
) -> Result<Vec<crate::manifest::profile::ProfileManifest>> {
    let mut all_profiles: Vec<crate::manifest::profile::ProfileManifest> =
        load_dir(&state.profiles_dir)?;
    if state.packs_dir.exists() {
        for entry in std::fs::read_dir(&state.packs_dir)? {
            let entry = entry?;
            if entry.file_type()?.is_dir() {
                let profile_dir = entry.path().join("profiles");
                let mut pps = load_dir(&profile_dir)?;
                all_profiles.append(&mut pps);
            }
        }
    }
    Ok(all_profiles
        .into_iter()
        .filter(|p| state.config.active_profiles.contains(&p.name))
        .collect())
}
```

Add to `crates/clix-core/src/lib.rs`:
```rust
pub mod loader;
```

- [ ] **Step 2: Compile check**

```bash
cargo check -p clix-core
```
Expected: no errors

- [ ] **Step 3: Commit**

```bash
git add crates/clix-core/src/loader.rs crates/clix-core/src/lib.rs
git commit -m "feat(core): add registry loader from state"
```

---

### Task 6: clix-cli binary

**Files:**
- Modify: `crates/clix-cli/Cargo.toml`
- Create: `crates/clix-cli/src/main.rs`
- Create: `crates/clix-cli/src/cli.rs`
- Create: `crates/clix-cli/src/output.rs`
- Create: `crates/clix-cli/src/commands/mod.rs`
- Create: `crates/clix-cli/src/commands/init.rs`
- Create: `crates/clix-cli/src/commands/status.rs`
- Create: `crates/clix-cli/src/commands/run.rs`
- Create: `crates/clix-cli/src/commands/capabilities.rs`
- Create: `crates/clix-cli/src/commands/workflow.rs`
- Create: `crates/clix-cli/src/commands/profile.rs`
- Create: `crates/clix-cli/src/commands/receipts.rs`
- Create: `crates/clix-cli/src/commands/serve.rs`
- Create: `crates/clix-cli/src/commands/pack.rs`

- [ ] **Step 1: Update clix-cli Cargo.toml**

`crates/clix-cli/Cargo.toml`:
```toml
[package]
name    = "clix-cli"
version = "0.2.0"
edition = "2021"
default-run = "clix"

[[bin]]
name = "clix"
path = "src/main.rs"

[dependencies]
clix-core  = { path = "../clix-core" }
clix-serve = { path = "../clix-serve" }
clap       = { workspace = true }
serde_json = { workspace = true }
serde      = { workspace = true }
anyhow     = { workspace = true }
tokio      = { workspace = true }
```

- [ ] **Step 2: Create output.rs**

`crates/clix-cli/src/output.rs`:
```rust
/// Print a value as pretty JSON to stdout.
pub fn print_json(value: &impl serde::Serialize) {
    println!("{}", serde_json::to_string_pretty(value).unwrap_or_else(|e| e.to_string()));
}

/// Print a simple key: value table to stdout.
pub fn print_kv(rows: &[(&str, String)]) {
    let max_key = rows.iter().map(|(k, _)| k.len()).max().unwrap_or(0);
    for (k, v) in rows {
        println!("{:<width$}  {v}", k, width = max_key);
    }
}
```

- [ ] **Step 3: Create commands/init.rs**

`crates/clix-cli/src/commands/init.rs`:
```rust
use anyhow::Result;
use clix_core::state::{ClixState, home_dir, ClixConfig};
use clix_core::packs::seed::seed_builtin_packs;

pub fn run() -> Result<()> {
    let home = home_dir();
    let state = ClixState::from_home(home.clone());
    state.ensure_dirs()?;

    // Write default config if not present
    if !state.config_path.exists() {
        let config = ClixConfig::default();
        let yaml = serde_yaml::to_string(&config)?;
        std::fs::write(&state.config_path, yaml)?;
        println!("Created {}", state.config_path.display());
    } else {
        println!("Config already exists: {}", state.config_path.display());
    }

    // Seed built-in packs from the binary's packs dir (relative to executable or cwd)
    let packs_src = find_builtin_packs_dir();
    if let Some(src) = packs_src {
        seed_builtin_packs(&state.packs_dir, &src)?;
        println!("Seeded built-in packs");
    }

    println!("clix initialized at {}", home.display());
    Ok(())
}

fn find_builtin_packs_dir() -> Option<std::path::PathBuf> {
    // Look relative to the executable first, then cwd
    let candidates = [
        std::env::current_exe().ok().and_then(|p| p.parent().map(|d| d.join("packs"))),
        Some(std::path::PathBuf::from("packs")),
    ];
    candidates.into_iter().flatten().find(|p| p.exists())
}
```

- [ ] **Step 4: Create commands/status.rs**

`crates/clix-cli/src/commands/status.rs`:
```rust
use anyhow::Result;
use clix_core::state::{ClixState, home_dir};
use clix_core::sandbox::sandbox_enforced;
use crate::output::{print_json, print_kv};

pub fn run(json: bool) -> Result<()> {
    let state = ClixState::load(home_dir())?;
    let enforced = sandbox_enforced();

    if json {
        print_json(&serde_json::json!({
            "home": state.home,
            "configPath": state.config_path,
            "activeProfiles": state.config.active_profiles,
            "defaultEnv": state.config.default_env,
            "approvalMode": format!("{:?}", state.config.approval_mode),
            "sandboxEnforced": enforced,
        }));
    } else {
        print_kv(&[
            ("home",           state.home.display().to_string()),
            ("config",         state.config_path.display().to_string()),
            ("active profiles",state.config.active_profiles.join(", ")),
            ("default env",    state.config.default_env.clone()),
            ("approval mode",  format!("{:?}", state.config.approval_mode)),
            ("sandbox",        if enforced { "enforced (Landlock)" } else { "not enforced" }.to_string()),
        ]);
    }
    Ok(())
}
```

- [ ] **Step 5: Create commands/run.rs**

`crates/clix-cli/src/commands/run.rs`:
```rust
use anyhow::{anyhow, Result};
use std::collections::HashMap;
use clix_core::state::{ClixState, home_dir};
use clix_core::loader::{build_registry, load_policy};
use clix_core::execution::run_capability;
use clix_core::policy::evaluate::ExecutionContext;
use clix_core::receipts::ReceiptStore;
use crate::output::print_json;

pub fn run(capability: &str, input_pairs: &[String], json: bool) -> Result<()> {
    let state = ClixState::load(home_dir())?;
    let registry = build_registry(&state)?;
    let policy = load_policy(&state)?;
    let store = ReceiptStore::open(&state.receipts_db)?;

    let input = parse_input_pairs(input_pairs)?;
    let ctx = ExecutionContext {
        env: state.config.default_env.clone(),
        cwd: state.config.workspace_root.clone().unwrap_or_else(|| {
            std::env::current_dir().unwrap_or_else(|_| std::path::PathBuf::from("."))
        }),
        user: whoami::username(),
        profile: state.config.active_profiles.first().cloned().unwrap_or_else(|| "default".to_string()),
        approver: None,
    };

    let outcome = run_capability(
        &registry, &policy, state.config.infisical.as_ref(), &store,
        capability, input, ctx,
    ).map_err(|e| anyhow!("{e}"))?;

    if json {
        print_json(&outcome);
    } else {
        if outcome.ok {
            println!("ok — receipt {}", outcome.receipt_id);
            if let Some(result) = &outcome.result {
                if let Some(stdout) = result["stdout"].as_str() {
                    if !stdout.is_empty() { print!("{stdout}"); }
                }
            }
        } else if outcome.approval_required {
            eprintln!("approval required — receipt {}", outcome.receipt_id);
            std::process::exit(2);
        } else {
            eprintln!("denied: {}", outcome.reason.unwrap_or_default());
            std::process::exit(1);
        }
    }
    Ok(())
}

fn parse_input_pairs(pairs: &[String]) -> Result<serde_json::Value> {
    let mut map = serde_json::Map::new();
    for pair in pairs {
        let (key, value) = pair.split_once('=')
            .ok_or_else(|| anyhow!("input must be key=value, got: {pair}"))?;
        // Try to parse value as JSON, fall back to string
        let v: serde_json::Value = serde_json::from_str(value)
            .unwrap_or_else(|_| serde_json::Value::String(value.to_string()));
        map.insert(key.to_string(), v);
    }
    Ok(serde_json::Value::Object(map))
}
```

Add `whoami` to workspace and clix-cli:
```toml
# Cargo.toml workspace.dependencies:
whoami = "1"
# crates/clix-cli/Cargo.toml:
whoami = { workspace = true }
```

- [ ] **Step 6: Create commands/capabilities.rs**

`crates/clix-cli/src/commands/capabilities.rs`:
```rust
use anyhow::Result;
use clix_core::state::{ClixState, home_dir};
use clix_core::loader::build_registry;
use crate::output::{print_json, print_kv};

pub fn list(json: bool) -> Result<()> {
    let state = ClixState::load(home_dir())?;
    let registry = build_registry(&state)?;
    let caps: Vec<_> = registry.all().into_iter().collect();

    if json {
        print_json(&caps);
    } else {
        for cap in &caps {
            println!("{:<40} {}", cap.name, cap.description.as_deref().unwrap_or(""));
        }
    }
    Ok(())
}

pub fn show(name: &str, json: bool) -> Result<()> {
    let state = ClixState::load(home_dir())?;
    let registry = build_registry(&state)?;
    match registry.get(name) {
        Some(cap) if json => { print_json(cap); }
        Some(cap) => {
            print_kv(&[
                ("name",         cap.name.clone()),
                ("version",      cap.version.to_string()),
                ("description",  cap.description.clone().unwrap_or_default()),
                ("risk",         format!("{:?}", cap.risk)),
                ("side effects", format!("{:?}", cap.side_effect_class)),
            ]);
        }
        None => anyhow::bail!("capability not found: {name}"),
    }
    Ok(())
}
```

- [ ] **Step 7: Create commands/workflow.rs**

`crates/clix-cli/src/commands/workflow.rs`:
```rust
use anyhow::{anyhow, Result};
use clix_core::state::{ClixState, home_dir};
use clix_core::loader::{build_registry, build_workflow_registry, load_policy};
use clix_core::execution::run_workflow;
use clix_core::policy::evaluate::ExecutionContext;
use clix_core::receipts::ReceiptStore;
use crate::output::print_json;
use super::run::parse_input_pairs; // reuse

pub fn list(json: bool) -> Result<()> {
    let state = ClixState::load(home_dir())?;
    let registry = build_workflow_registry(&state)?;
    let wfs: Vec<_> = registry.all().into_iter().collect();
    if json { print_json(&wfs); }
    else {
        for wf in &wfs {
            println!("{:<40} {}", wf.name, wf.description.as_deref().unwrap_or(""));
        }
    }
    Ok(())
}

pub fn run_wf(name: &str, input_pairs: &[String], json: bool) -> Result<()> {
    let state = ClixState::load(home_dir())?;
    let cap_reg = build_registry(&state)?;
    let wf_reg = build_workflow_registry(&state)?;
    let policy = load_policy(&state)?;
    let store = ReceiptStore::open(&state.receipts_db)?;
    let input = parse_input_pairs(input_pairs)?;
    let ctx = ExecutionContext {
        env: state.config.default_env.clone(),
        cwd: state.config.workspace_root.clone().unwrap_or_else(|| {
            std::env::current_dir().unwrap_or_else(|_| std::path::PathBuf::from("."))
        }),
        user: whoami::username(),
        profile: state.config.active_profiles.first().cloned().unwrap_or_else(|| "default".to_string()),
        approver: None,
    };
    let outcomes = run_workflow(&cap_reg, &wf_reg, &policy, state.config.infisical.as_ref(), &store, name, input, ctx)
        .map_err(|e| anyhow!("{e}"))?;
    if json { print_json(&outcomes); }
    else {
        for (i, o) in outcomes.iter().enumerate() {
            println!("step {}: {} — receipt {}", i + 1, if o.ok { "ok" } else { "failed" }, o.receipt_id);
        }
    }
    Ok(())
}
```

Add `whoami` import to workflow.rs:
```rust
use whoami;
```

- [ ] **Step 8: Create commands/profile.rs**

`crates/clix-cli/src/commands/profile.rs`:
```rust
use anyhow::Result;
use clix_core::state::{ClixState, home_dir};
use clix_core::manifest::loader::load_dir;
use crate::output::print_json;

pub fn list(json: bool) -> Result<()> {
    let state = ClixState::load(home_dir())?;
    let profiles: Vec<clix_core::manifest::profile::ProfileManifest> = load_dir(&state.profiles_dir)?;
    if json { print_json(&profiles); }
    else {
        for p in &profiles {
            let active = if state.config.active_profiles.contains(&p.name) { "*" } else { " " };
            println!("{active} {}", p.name);
        }
    }
    Ok(())
}

pub fn activate(name: &str) -> Result<()> {
    let mut state = ClixState::load(home_dir())?;
    if !state.config.active_profiles.contains(&name.to_string()) {
        state.config.active_profiles.push(name.to_string());
        save_config(&state)?;
        println!("activated: {name}");
    } else {
        println!("{name} already active");
    }
    Ok(())
}

pub fn deactivate(name: &str) -> Result<()> {
    let mut state = ClixState::load(home_dir())?;
    state.config.active_profiles.retain(|p| p != name);
    save_config(&state)?;
    println!("deactivated: {name}");
    Ok(())
}

fn save_config(state: &ClixState) -> Result<()> {
    let yaml = serde_yaml::to_string(&state.config)?;
    std::fs::write(&state.config_path, yaml)?;
    Ok(())
}
```

- [ ] **Step 9: Create commands/receipts.rs**

`crates/clix-cli/src/commands/receipts.rs`:
```rust
use anyhow::{anyhow, Result};
use clix_core::state::{ClixState, home_dir};
use clix_core::receipts::ReceiptStore;
use crate::output::print_json;

pub fn list(limit: usize, status: Option<&str>, json: bool) -> Result<()> {
    let state = ClixState::load(home_dir())?;
    let store = ReceiptStore::open(&state.receipts_db)?;
    let receipts = store.list(limit, status)?;
    if json { print_json(&receipts); }
    else {
        for r in &receipts {
            println!("{} {} {} {}", r.id, r.created_at.format("%Y-%m-%dT%H:%M:%SZ"), r.status, r.capability);
        }
    }
    Ok(())
}

pub fn show(id: &str, json: bool) -> Result<()> {
    let state = ClixState::load(home_dir())?;
    let store = ReceiptStore::open(&state.receipts_db)?;
    match store.get(id)? {
        Some(r) if json => { print_json(&r); }
        Some(r) => {
            println!("id:          {}", r.id);
            println!("capability:  {}", r.capability);
            println!("status:      {}", r.status);
            println!("created:     {}", r.created_at);
            println!("sandbox:     {}", r.sandbox_enforced);
        }
        None => anyhow::bail!("receipt not found: {id}"),
    }
    Ok(())
}

/// Tail new receipts from SQLite — poll every second, emit JSON lines.
pub fn tail() -> Result<()> {
    let state = ClixState::load(home_dir())?;
    let store = ReceiptStore::open(&state.receipts_db)?;
    let mut last_seen = String::new();
    loop {
        let receipts = store.list(50, None)?;
        for r in receipts.iter().rev() {
            let id = r.id.to_string();
            if id > last_seen {
                println!("{}", serde_json::to_string(r).unwrap_or_default());
                last_seen = id;
            }
        }
        std::thread::sleep(std::time::Duration::from_secs(1));
    }
}
```

- [ ] **Step 10: Create commands/serve.rs**

`crates/clix-cli/src/commands/serve.rs`:
```rust
use anyhow::Result;

pub async fn run(socket: Option<String>, http: Option<String>) -> Result<()> {
    // Delegates to clix-serve (Phase 4)
    // Stub for now — Phase 4 will fill this in
    let addr = http.as_deref().unwrap_or("").to_string();
    let sock = socket.as_deref().unwrap_or("").to_string();
    if !sock.is_empty() {
        println!("serve socket: {sock} (implement in Phase 4)");
    } else if !addr.is_empty() {
        println!("serve http: {addr} (implement in Phase 4)");
    } else {
        println!("serve stdio (implement in Phase 4)");
    }
    Ok(())
}
```

- [ ] **Step 11: Create commands/pack.rs**

`crates/clix-cli/src/commands/pack.rs`:
```rust
use anyhow::Result;
use std::path::Path;
use clix_core::state::{ClixState, home_dir};
use clix_core::packs::{
    discover_pack, install_pack, validate_pack, diff_pack,
    bundle_pack, publish_pack, scaffold_pack, onboard_cli,
};
use clix_core::packs::scaffold::Preset;
use crate::output::print_json;

pub fn list(json: bool) -> Result<()> {
    let state = ClixState::load(home_dir())?;
    if !state.packs_dir.exists() {
        return Ok(());
    }
    let mut packs = vec![];
    for entry in std::fs::read_dir(&state.packs_dir)? {
        let entry = entry?;
        if entry.file_type()?.is_dir() {
            let pack_file = entry.path().join("pack.yaml");
            if pack_file.exists() {
                let content = std::fs::read_to_string(&pack_file)?;
                if let Ok(p) = serde_yaml::from_str::<clix_core::manifest::pack::PackManifest>(&content) {
                    packs.push(p);
                }
            }
        }
    }
    packs.sort_by(|a, b| a.name.cmp(&b.name));
    if json { print_json(&packs); }
    else {
        for p in &packs {
            println!("{:<30} v{}  {}", p.name, p.version, p.description.as_deref().unwrap_or(""));
        }
    }
    Ok(())
}

pub fn show(name: &str, json: bool) -> Result<()> {
    let state = ClixState::load(home_dir())?;
    let pack_dir = state.packs_dir.join(name);
    anyhow::ensure!(pack_dir.exists(), "pack not found: {name}");
    let report = discover_pack(&pack_dir)?;
    if json { print_json(&report); }
    else {
        println!("name:         {}", report.pack.name);
        println!("version:      {}", report.pack.version);
        println!("capabilities: {}", report.capabilities.len());
        println!("profiles:     {}", report.profiles.len());
        println!("workflows:    {}", report.workflows.len());
    }
    Ok(())
}

pub fn discover(path: &str, json: bool) -> Result<()> {
    let report = discover_pack(Path::new(path))?;
    if json { print_json(&report); }
    else {
        println!("pack:         {}", report.pack.name);
        println!("capabilities: {}", report.capabilities.iter().map(|c| c.name.as_str()).collect::<Vec<_>>().join(", "));
        for w in &report.warnings { eprintln!("warn: {w}"); }
    }
    Ok(())
}

pub fn validate(path: &str) -> Result<()> {
    let errors = validate_pack(Path::new(path))?;
    if errors.is_empty() {
        println!("ok");
        Ok(())
    } else {
        for e in &errors { eprintln!("error: [{}] {}", e.path, e.message); }
        anyhow::bail!("{} validation error(s)", errors.len())
    }
}

pub fn diff(installed_name: &str, new_path: &str, json: bool) -> Result<()> {
    let state = ClixState::load(home_dir())?;
    let installed = state.packs_dir.join(installed_name);
    anyhow::ensure!(installed.exists(), "installed pack not found: {installed_name}");
    let report = diff_pack(&installed, Path::new(new_path))?;
    if json { print_json(&report); }
    else {
        if let Some((old, new)) = report.version_change { println!("version: {old} → {new}"); }
        if !report.capabilities_added.is_empty()   { println!("+ capabilities: {}", report.capabilities_added.join(", ")); }
        if !report.capabilities_removed.is_empty() { println!("- capabilities: {}", report.capabilities_removed.join(", ")); }
        if !report.capabilities_changed.is_empty() { println!("~ capabilities: {}", report.capabilities_changed.join(", ")); }
    }
    Ok(())
}

pub fn install(path: &str) -> Result<()> {
    let state = ClixState::load(home_dir())?;
    let dest = install_pack(Path::new(path), &state.packs_dir)?;
    println!("installed: {}", dest.display());
    Ok(())
}

pub fn bundle(path: &str) -> Result<()> {
    let zip = bundle_pack(Path::new(path), Path::new("."))?;
    println!("bundled: {}", zip.display());
    Ok(())
}

pub fn publish(path: &str) -> Result<()> {
    let state = ClixState::load(home_dir())?;
    publish_pack(Path::new(path), &state.bundles_dir)?;
    println!("published");
    Ok(())
}

pub fn scaffold(name: &str, preset_str: &str, command: Option<&str>) -> Result<()> {
    let preset: Preset = preset_str.parse().map_err(|e: String| anyhow::anyhow!(e))?;
    let dir = scaffold_pack(name, preset, command, Path::new("."))?;
    println!("scaffolded: {}", dir.display());
    Ok(())
}

pub fn onboard(name: &str, command: &str, json: bool) -> Result<()> {
    let report = onboard_cli(name, command, Path::new("."))?;
    if json { print_json(&report); }
    else {
        println!("cli:       {}", report.cli);
        println!("preset:    {} (confidence {:.0}%)", report.suggested_preset, report.confidence * 100.0);
        println!("subcommands: {}", report.inferred_subcommands.join(", "));
        if let Some(p) = &report.scaffold_path { println!("scaffold:  {}", p.display()); }
        for w in &report.warnings { eprintln!("warn: {w}"); }
    }
    Ok(())
}
```

- [ ] **Step 12: Create commands/mod.rs**

`crates/clix-cli/src/commands/mod.rs`:
```rust
pub mod capabilities;
pub mod init;
pub mod pack;
pub mod profile;
pub mod receipts;
pub mod run;
pub mod serve;
pub mod workflow;
```

- [ ] **Step 13: Create cli.rs**

`crates/clix-cli/src/cli.rs`:
```rust
use clap::{Parser, Subcommand};

#[derive(Parser)]
#[command(name = "clix", version, about = "Policy-first CLI control plane for agentic tool use")]
pub struct Cli {
    #[command(subcommand)]
    pub command: Commands,
}

#[derive(Subcommand)]
pub enum Commands {
    /// Initialize clix in ~/.clix
    Init,

    /// Show clix status and configuration
    Status {
        #[arg(long)]
        json: bool,
    },

    /// Print version information
    Version,

    /// Run a capability
    Run {
        /// Capability name (e.g. sys.date, kubectl.get-pods)
        capability: String,
        /// Input key=value pairs
        #[arg(long = "input", short = 'i', value_name = "KEY=VALUE")]
        input: Vec<String>,
        #[arg(long)]
        json: bool,
    },

    /// Manage capabilities
    #[command(subcommand)]
    Capabilities(CapabilitiesCmd),

    /// Manage and run workflows
    #[command(subcommand)]
    Workflow(WorkflowCmd),

    /// Manage profiles
    #[command(subcommand)]
    Profile(ProfileCmd),

    /// View execution receipts
    #[command(subcommand)]
    Receipts(ReceiptsCmd),

    /// Start the JSON-RPC server
    Serve {
        #[arg(long)]
        socket: Option<String>,
        #[arg(long)]
        http: Option<String>,
    },

    /// Manage packs
    #[command(subcommand)]
    Pack(PackCmd),
}

#[derive(Subcommand)]
pub enum CapabilitiesCmd {
    /// List all capabilities
    List { #[arg(long)] json: bool },
    /// Show a capability
    Show { name: String, #[arg(long)] json: bool },
}

#[derive(Subcommand)]
pub enum WorkflowCmd {
    /// List all workflows
    List { #[arg(long)] json: bool },
    /// Run a workflow
    Run {
        name: String,
        #[arg(long = "input", short = 'i', value_name = "KEY=VALUE")]
        input: Vec<String>,
        #[arg(long)] json: bool,
    },
}

#[derive(Subcommand)]
pub enum ProfileCmd {
    /// List all profiles
    List { #[arg(long)] json: bool },
    /// Show a profile
    Show { name: String, #[arg(long)] json: bool },
    /// Activate a profile
    Activate { name: String },
    /// Deactivate a profile
    Deactivate { name: String },
}

#[derive(Subcommand)]
pub enum ReceiptsCmd {
    /// List receipts
    List {
        #[arg(long, default_value = "50")] limit: usize,
        #[arg(long)] status: Option<String>,
        #[arg(long)] json: bool,
    },
    /// Show a receipt
    Show { id: String, #[arg(long)] json: bool },
    /// Tail new receipts (live stream)
    Tail,
}

#[derive(Subcommand)]
pub enum PackCmd {
    /// List installed packs
    List { #[arg(long)] json: bool },
    /// Show a pack
    Show { name: String, #[arg(long)] json: bool },
    /// Discover a pack directory without installing
    Discover { path: String, #[arg(long)] json: bool },
    /// Validate a pack directory
    Validate { path: String },
    /// Diff installed pack vs new source
    Diff { installed: String, new_path: String, #[arg(long)] json: bool },
    /// Install a pack from directory or .clixpack.zip
    Install { path: String },
    /// Create a distributable bundle archive
    Bundle { path: String },
    /// Publish a bundle to the local registry
    Publish { path: String },
    /// Scaffold a new pack from a preset
    Scaffold {
        name: String,
        #[arg(long, default_value = "read-only")] preset: String,
        #[arg(long)] command: Option<String>,
    },
    /// Probe a CLI and generate a pack scaffold
    Onboard {
        name: String,
        #[arg(long)] command: String,
        #[arg(long)] json: bool,
    },
}
```

- [ ] **Step 14: Create main.rs**

`crates/clix-cli/src/main.rs`:
```rust
mod cli;
mod commands;
mod output;

use anyhow::Result;
use clap::Parser;
use cli::{Cli, Commands, CapabilitiesCmd, WorkflowCmd, ProfileCmd, ReceiptsCmd, PackCmd};

#[tokio::main]
async fn main() -> Result<()> {
    let cli = Cli::parse();
    match cli.command {
        Commands::Init => commands::init::run()?,
        Commands::Status { json } => commands::status::run(json)?,
        Commands::Version => {
            println!("clix {}", env!("CARGO_PKG_VERSION"));
        }
        Commands::Run { capability, input, json } => {
            commands::run::run(&capability, &input, json)?;
        }
        Commands::Capabilities(sub) => match sub {
            CapabilitiesCmd::List { json }        => commands::capabilities::list(json)?,
            CapabilitiesCmd::Show { name, json }  => commands::capabilities::show(&name, json)?,
        },
        Commands::Workflow(sub) => match sub {
            WorkflowCmd::List { json }                     => commands::workflow::list(json)?,
            WorkflowCmd::Run { name, input, json }         => commands::workflow::run_wf(&name, &input, json)?,
        },
        Commands::Profile(sub) => match sub {
            ProfileCmd::List { json }              => commands::profile::list(json)?,
            ProfileCmd::Show { name, .. }          => { let _ = name; println!("show profile (todo)"); }
            ProfileCmd::Activate { name }          => commands::profile::activate(&name)?,
            ProfileCmd::Deactivate { name }        => commands::profile::deactivate(&name)?,
        },
        Commands::Receipts(sub) => match sub {
            ReceiptsCmd::List { limit, status, json } => commands::receipts::list(limit, status.as_deref(), json)?,
            ReceiptsCmd::Show { id, json }            => commands::receipts::show(&id, json)?,
            ReceiptsCmd::Tail                         => commands::receipts::tail()?,
        },
        Commands::Serve { socket, http } => {
            commands::serve::run(socket, http).await?;
        }
        Commands::Pack(sub) => match sub {
            PackCmd::List { json }                            => commands::pack::list(json)?,
            PackCmd::Show { name, json }                      => commands::pack::show(&name, json)?,
            PackCmd::Discover { path, json }                  => commands::pack::discover(&path, json)?,
            PackCmd::Validate { path }                        => commands::pack::validate(&path)?,
            PackCmd::Diff { installed, new_path, json }       => commands::pack::diff(&installed, &new_path, json)?,
            PackCmd::Install { path }                         => commands::pack::install(&path)?,
            PackCmd::Bundle { path }                          => commands::pack::bundle(&path)?,
            PackCmd::Publish { path }                         => commands::pack::publish(&path)?,
            PackCmd::Scaffold { name, preset, command }       => commands::pack::scaffold(&name, &preset, command.as_deref())?,
            PackCmd::Onboard { name, command, json }          => commands::pack::onboard(&name, &command, json)?,
        },
    }
    Ok(())
}
```

- [ ] **Step 15: Build and verify**

```bash
cargo build -p clix-cli
```
Expected: compiles successfully

```bash
cargo run -p clix-cli -- version
```
Expected: `clix 0.2.0`

```bash
cargo run -p clix-cli -- init
```
Expected: initializes ~/.clix, seeds packs

```bash
cargo run -p clix-cli -- run sys.date
```
Expected: prints current UTC date

```bash
cargo run -p clix-cli -- capabilities list
```
Expected: lists system.date, system.echo (and kubectl/gcloud caps if seeded)

```bash
cargo run -p clix-cli -- run sys.date --json
```
Expected: JSON output with `ok`, `receiptId`, `result`

- [ ] **Step 16: Commit**

```bash
git add crates/clix-cli/ Cargo.toml
git commit -m "feat(cli): add complete clix-cli binary with all subcommands"
```

---

### Task 7: Phase 3 wrap-up

- [ ] **Step 1: Run all tests**

```bash
cargo test
```
Expected: all pass

- [ ] **Step 2: Run clippy**

```bash
cargo clippy -- -D warnings
```
Fix any warnings.

- [ ] **Step 3: Smoke test CLI**

```bash
cargo run -p clix-cli -- status
cargo run -p clix-cli -- pack list
cargo run -p clix-cli -- pack discover packs/base --json
cargo run -p clix-cli -- pack validate packs/base
cargo run -p clix-cli -- capabilities list --json
cargo run -p clix-cli -- run sys.echo --input message=hello --json
```
Expected: all commands produce output without panicking.

- [ ] **Step 4: Final commit**

```bash
git add -A
git commit -m "feat: Phase 3 complete — pack management and clix-cli binary"
```

---

## Phase 3 Complete

Produces: Complete `clix pack *` commands, built-in YAML packs, and a fully working `clix` CLI binary. Agents can run capabilities, manage packs, and query receipts.

**Next:** `docs/superpowers/plans/2026-04-13-phase4-serve.md` — async tokio serve layer with MCP compliance and clix extensions.
