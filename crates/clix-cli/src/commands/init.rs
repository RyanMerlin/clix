use anyhow::{anyhow, Result};
use std::path::{Path, PathBuf};
use clix_core::state::{ClixConfig, ClixState, home_dir};
use clix_core::packs::seed::seed_builtin_packs;

/// Write a `.mcp.json` at `project_dir` (defaults to cwd) for Claude Code.
///
/// Claude Code reads `.mcp.json` in the project root as a project-scoped MCP server
/// configuration. This writes a minimal entry that starts `clix serve` as a stdio
/// MCP server when Claude Code opens the project.
pub fn setup_claude_code(project_dir: Option<&Path>) -> Result<()> {
    let dir = project_dir.map(|p| p.to_path_buf())
        .unwrap_or_else(|| std::env::current_dir().unwrap_or_else(|_| PathBuf::from(".")));

    let mcp_json_path = dir.join(".mcp.json");

    // Read existing .mcp.json if present so we can merge rather than overwrite.
    let mut root: serde_json::Value = if mcp_json_path.exists() {
        let text = std::fs::read_to_string(&mcp_json_path)?;
        serde_json::from_str(&text).unwrap_or(serde_json::json!({}))
    } else {
        serde_json::json!({})
    };

    if root.get("mcpServers").is_none() {
        root["mcpServers"] = serde_json::json!({});
    }
    root["mcpServers"]["clix"] = serde_json::json!({
        "command": "clix",
        "args": ["serve"],
        "description": "clix — policy-enforced, sandboxed CLI gateway"
    });

    let json = serde_json::to_string_pretty(&root)?;
    std::fs::write(&mcp_json_path, json)?;
    println!("wrote {}", mcp_json_path.display());

    // Write CLAUDE.md integration block
    let claude_md_path = dir.join("CLAUDE.md");
    let clix_block = claude_md_snippet();
    if claude_md_path.exists() {
        let existing = std::fs::read_to_string(&claude_md_path)?;
        if existing.contains("<!-- clix-integration -->") {
            println!("{} already has clix integration block — skipped", claude_md_path.display());
        } else {
            let updated = format!("{}\n\n{}", existing.trim_end(), clix_block);
            std::fs::write(&claude_md_path, updated)?;
            println!("appended clix block to {}", claude_md_path.display());
        }
    } else {
        std::fs::write(&claude_md_path, clix_block)?;
        println!("wrote {}", claude_md_path.display());
    }

    println!();
    println!("Claude Code will now start clix as an MCP server for this project.");
    println!("Restart Claude Code (or reload MCP servers) to apply.");
    Ok(())
}

/// Write `.cursor/mcp.json` at `project_dir` for Cursor.
pub fn setup_cursor(project_dir: Option<&Path>) -> Result<()> {
    let dir = project_dir.map(|p| p.to_path_buf())
        .unwrap_or_else(|| std::env::current_dir().unwrap_or_else(|_| PathBuf::from(".")));

    let cursor_dir = dir.join(".cursor");
    std::fs::create_dir_all(&cursor_dir)?;
    let mcp_path = cursor_dir.join("mcp.json");

    let mut root: serde_json::Value = if mcp_path.exists() {
        let text = std::fs::read_to_string(&mcp_path)?;
        serde_json::from_str(&text).unwrap_or(serde_json::json!({}))
    } else {
        serde_json::json!({})
    };

    if root.get("mcpServers").is_none() {
        root["mcpServers"] = serde_json::json!({});
    }
    root["mcpServers"]["clix"] = serde_json::json!({
        "command": "clix",
        "args": ["serve"],
        "description": "clix — policy-enforced, sandboxed CLI gateway"
    });

    let json = serde_json::to_string_pretty(&root)?;
    std::fs::write(&mcp_path, json)?;
    println!("wrote {}", mcp_path.display());
    println!();
    println!("Cursor will use clix as an MCP tool server for this project.");
    println!("Restart Cursor (or reload MCP servers in Settings → MCP) to apply.");
    Ok(())
}

fn claude_md_snippet() -> String {
    r#"<!-- clix-integration -->
## clix — sandboxed CLI gateway

clix gates CLI tools (git, kubectl, gcloud, etc.) behind policy and OS-level isolation.
It is available as an MCP server (configured in `.mcp.json`) and directly via CLI.

### Direct CLI usage (preferred for scripted tasks)
```
clix capabilities list --json        # browse tools
clix capabilities search <query> --json
clix capabilities show <name> --json # get input schema
clix run <name> -i key=val --json    # execute → {ok, result, receipt_id}
clix run <name> --dry-run --json     # policy preview, no execution
clix doctor --json                   # health
```

Exit codes: 0 ok · 1 denied · 2 needs approval

### MCP usage (automatic via `.mcp.json`)
Use the `tools/list` MCP method to discover capabilities. Prefer the namespace
drill-in pattern: call `tools/list` with no params to get namespace stubs, then
`tools/list` with `{"namespace": "git"}` to get capabilities for that group.
<!-- end clix-integration -->
"#.to_string()
}

pub fn run() -> Result<()> {
    let home = home_dir();
    let state = ClixState::from_home(home.clone());
    state.ensure_dirs()?;

    // Load or create config
    let mut config = if state.config_path.exists() {
        let text = std::fs::read_to_string(&state.config_path)?;
        serde_yaml::from_str::<ClixConfig>(&text)?   // propagate parse errors, don't swallow
    } else {
        let config = ClixConfig::default();
        println!("Created {}", state.config_path.display());
        config
    };

    // Seed built-in packs — search order:
    // 1. next to the executable (e.g. /usr/local/bin/packs or ~/.local/bin/packs)
    // 2. XDG data home: ~/.local/share/clix/packs
    // 3. ./packs relative to cwd (development / repo checkout)
    let xdg_packs = dirs::data_local_dir().map(|d| d.join("clix").join("packs"));
    let packs_src = [
        std::env::current_exe().ok().and_then(|p| p.parent().map(|d| d.join("packs"))),
        xdg_packs,
        Some(std::path::PathBuf::from("packs")),
    ].into_iter().flatten().find(|p| p.exists());
    if let Some(src) = packs_src {
        seed_builtin_packs(&state.packs_dir, &src)?;
        println!("Seeded built-in packs");
    }

    // Seed starter policy if none exists
    if !state.policy_path.exists() {
        let starter_policy = r#"# clix policy — edit this file to control what can run.
# Rules are evaluated top-to-bottom; first match wins.
# defaultAction: deny  means anything not explicitly allowed here is blocked.
#
# Starter policy: allow capabilities with no side effects or read-only side effects.
# These are safe observation commands (list, show, status, logs, inspect).
# Add explicit rules above to require approval for mutating or destructive operations.

rules:
  - sideEffectClass: none
    action: allow
    reason: No-side-effect capabilities (builtins, date, echo) are always safe.
  - sideEffectClass: readOnly
    action: allow
    reason: Read-only capabilities (list, show, inspect, logs) are safe to run.

defaultAction: deny
"#;
        std::fs::write(&state.policy_path, starter_policy)?;
        println!("Created starter policy: {}", state.policy_path.display());
    }

    // Auto-activate base profile if nothing is active
    if config.active_profiles.is_empty() {
        config.active_profiles.push("base".to_string());
        println!("Activated default profile: base");
    }

    // Write config (single write) + enforce 0600 on Linux
    let yaml = serde_yaml::to_string(&config)?;
    std::fs::write(&state.config_path, &yaml)?;
    #[cfg(target_os = "linux")]
    {
        use std::os::unix::fs::PermissionsExt;
        if let Ok(meta) = std::fs::metadata(&state.config_path) {
            let mut perms = meta.permissions();
            perms.set_mode(0o600);
            let _ = std::fs::set_permissions(&state.config_path, perms);
        }
    }

    println!("clix initialized at {}", home.display());

    // Hint about Infisical if not configured
    if config.infisical.is_none() {
        println!();
        println!("  run `clix secrets set` or press [c] in the TUI Dashboard to configure Infisical secrets");
    }

    // On Linux, ensure OS isolation is actually usable — install AppArmor profile if needed
    #[cfg(target_os = "linux")]
    {
        let restricted = std::fs::read_to_string("/proc/sys/kernel/apparmor_restrict_unprivileged_userns")
            .unwrap_or_default();
        let already_installed = std::path::Path::new("/etc/apparmor.d/clix-worker").exists();
        if restricted.trim() == "1" && !already_installed {
            println!();
            println!("Setting up OS isolation (AppArmor profile for clix-worker) — requires sudo:");
            match install_isolation() {
                Ok(_) => {}
                Err(e) => eprintln!("  warn: could not install AppArmor profile: {e}"),
            }
        }
    }

    Ok(())
}

/// Install PATH-shim binaries for a set of CLI commands.
///
/// For each `command`, copies the `clix-shim` binary to `$CLIX_HOME/bin/<command>`.
/// Prints shell activation instructions (`export PATH="$CLIX_HOME/bin:$PATH"`) if not already
/// detected in the user's shell profile.
///
/// The shim binary is located by looking next to the current executable first, then on PATH.
pub fn install_shims(commands: &[&str]) -> Result<()> {
    let home = home_dir();
    let bin_dir = home.join("bin");
    std::fs::create_dir_all(&bin_dir)?;

    let shim_src = locate_binary("clix-shim")?;

    for command in commands {
        let dest = bin_dir.join(command);
        std::fs::copy(&shim_src, &dest)
            .map_err(|e| anyhow!("copy shim to {}: {e}", dest.display()))?;

        #[cfg(unix)]
        {
            use std::os::unix::fs::PermissionsExt;
            std::fs::set_permissions(&dest, std::fs::Permissions::from_mode(0o755))?;
        }

        println!("installed shim: {} → {}", command, dest.display());
    }

    // Write activation scripts
    write_activation_scripts(&bin_dir)?;

    // Suggest PATH update
    let bin_dir_str = bin_dir.to_string_lossy();
    let path_var = std::env::var("PATH").unwrap_or_default();
    if !path_var.split(':').any(|p| p == bin_dir_str.as_ref()) {
        println!();
        println!("To activate shims, source the activation script for your shell:");
        println!("  # bash/zsh");
        println!("  source {}/activate.sh", bin_dir_str);
        println!();
        println!("  # fish");
        println!("  source {}/activate.fish", bin_dir_str);
        println!();
        println!("Or add to your shell profile permanently:");
        println!("  export PATH=\"{}:$PATH\"", bin_dir_str);
    }

    Ok(())
}

fn write_activation_scripts(bin_dir: &Path) -> Result<()> {
    let bin_dir_str = bin_dir.to_string_lossy();

    // sh/bash/zsh
    let sh_script = format!(
        "# clix shim activation — generated by `clix init --install-shims`\nexport PATH=\"{bin}:$PATH\"\n",
        bin = bin_dir_str
    );
    std::fs::write(bin_dir.join("activate.sh"), &sh_script)?;

    // fish
    let fish_script = format!(
        "# clix shim activation — generated by `clix init --install-shims`\nset -Ux fish_user_paths \"{bin}\" $fish_user_paths\n",
        bin = bin_dir_str
    );
    std::fs::write(bin_dir.join("activate.fish"), &fish_script)?;

    // PowerShell
    let ps1_script = format!(
        "# clix shim activation — generated by `clix init --install-shims`\n$env:PATH = \"{bin};\" + $env:PATH\n",
        bin = bin_dir_str
    );
    std::fs::write(bin_dir.join("activate.ps1"), &ps1_script)?;

    println!("wrote activation scripts to {}/activate.{{sh,fish,ps1}}", bin_dir_str);
    Ok(())
}

/// Migrate host credentials for a CLI into the broker-owned creds directory.
///
/// The credentials are moved (not copied) from their default locations to
/// `$CLIX_BROKER_CREDS_DIR/<cli>/` and the source files are replaced with a symlink
/// that points to a dead path — so any process running as the agent UID gets ENOENT
/// when it tries to read the original location. The broker (running as clix-broker-uid)
/// owns the new location with mode 0700.
///
/// Supported `cli` values:
///   - `gcloud`:  moves `~/.config/gcloud/application_default_credentials.json` (ADC)
///   - `kubectl`: moves `~/.kube/config`
///
/// WARNING: This changes the behaviour of the CLI for the current user — gcloud and kubectl
/// will no longer be able to authenticate without going through the broker. Run
/// `clix init --unadopt-creds <cli>` to reverse.
pub fn adopt_creds(cli: &str) -> Result<()> {
    let broker_creds_dir = broker_creds_dir();

    match cli {
        "gcloud" => adopt_gcloud_creds(&broker_creds_dir),
        "kubectl" => adopt_kubectl_creds(&broker_creds_dir),
        other => Err(anyhow!("unsupported CLI for credential adoption: `{other}`. Supported: gcloud, kubectl")),
    }
}

fn adopt_gcloud_creds(broker_creds_dir: &Path) -> Result<()> {
    let src = dirs::home_dir()
        .unwrap_or_else(|| PathBuf::from("~"))
        .join(".config")
        .join("gcloud")
        .join("application_default_credentials.json");

    if !src.exists() {
        return Err(anyhow!(
            "gcloud ADC not found at {}. Run `gcloud auth application-default login` first.",
            src.display()
        ));
    }

    let dest_dir = broker_creds_dir.join("gcloud");
    std::fs::create_dir_all(&dest_dir)?;
    secure_dir(&dest_dir)?;

    let dest = dest_dir.join("adc.json");
    std::fs::copy(&src, &dest)
        .map_err(|e| anyhow!("copy ADC to broker store: {e}"))?;
    secure_file(&dest)?;

    // Replace the original with a dead symlink to make direct access fail
    // (ENOENT for anything trying to follow the symlink)
    std::fs::remove_file(&src)
        .map_err(|e| anyhow!("remove original ADC: {e}"))?;
    #[cfg(unix)]
    {
        std::os::unix::fs::symlink("/clix-broker-adopted-this-credential", &src)
            .map_err(|e| anyhow!("create dead symlink at {}: {e}", src.display()))?;
    }

    println!("gcloud ADC adopted into broker store: {}", dest.display());
    println!("Original path ({}) now points to a dead symlink.", src.display());
    println!("Direct `gcloud auth print-access-token` will fail — use `clix run` instead.");
    Ok(())
}

fn adopt_kubectl_creds(broker_creds_dir: &Path) -> Result<()> {
    let src = dirs::home_dir()
        .unwrap_or_else(|| PathBuf::from("~"))
        .join(".kube")
        .join("config");

    if !src.exists() {
        return Err(anyhow!(
            "kubeconfig not found at {}. Set up kubectl credentials first.",
            src.display()
        ));
    }

    let dest_dir = broker_creds_dir.join("kubectl");
    std::fs::create_dir_all(&dest_dir)?;
    secure_dir(&dest_dir)?;

    let dest = dest_dir.join("kubeconfig");
    std::fs::copy(&src, &dest)
        .map_err(|e| anyhow!("copy kubeconfig to broker store: {e}"))?;
    secure_file(&dest)?;

    std::fs::remove_file(&src)
        .map_err(|e| anyhow!("remove original kubeconfig: {e}"))?;
    #[cfg(unix)]
    {
        std::os::unix::fs::symlink("/clix-broker-adopted-this-credential", &src)
            .map_err(|e| anyhow!("create dead symlink at {}: {e}", src.display()))?;
    }

    println!("kubectl config adopted into broker store: {}", dest.display());
    println!("Original path ({}) now points to a dead symlink.", src.display());
    Ok(())
}

/// Adopt a service-account JSON key file into the broker credential store.
///
/// Copies the file to `$CLIX_BROKER_CREDS_DIR/gcloud/sa-<sha256_prefix>.json` with 0600
/// permissions. Replaces the original with a dead symlink. Appends an entry to
/// `$CLIX_BROKER_CREDS_DIR/gcloud/sa_registry.json`.
///
/// Returns a summary string on success.
pub fn adopt_sa_json(path: &str) -> Result<String> {
    use sha2::{Sha256, Digest};

    let src = std::path::Path::new(path);
    if !src.exists() {
        return Err(anyhow!("SA JSON file not found: {path}"));
    }

    let content = std::fs::read(src)
        .map_err(|e| anyhow!("read SA JSON: {e}"))?;

    // Parse to extract client_email
    let sa: serde_json::Value = serde_json::from_slice(&content)
        .map_err(|e| anyhow!("parse SA JSON: {e}"))?;
    let client_email = sa["client_email"].as_str()
        .ok_or_else(|| anyhow!("SA JSON missing client_email"))?
        .to_string();

    // Compute sha256 hash for dedup filename
    let mut hasher = Sha256::new();
    hasher.update(&content);
    let hash = hex::encode(hasher.finalize());
    let short_hash = &hash[..12];

    let broker_creds_dir = broker_creds_dir();
    let dest_dir = broker_creds_dir.join("gcloud");
    std::fs::create_dir_all(&dest_dir)?;
    secure_dir(&dest_dir)?;

    let dest_filename = format!("sa-{short_hash}.json");
    let dest = dest_dir.join(&dest_filename);

    std::fs::write(&dest, &content)
        .map_err(|e| anyhow!("write SA to broker store: {e}"))?;
    secure_file(&dest)?;

    // Replace original with dead symlink
    let _ = std::fs::remove_file(src);
    #[cfg(unix)]
    {
        std::os::unix::fs::symlink("/clix-broker-adopted-this-credential", src)
            .map_err(|e| anyhow!("create dead symlink at {}: {e}", src.display()))?;
    }

    // Update SA registry
    let registry_path = dest_dir.join("sa_registry.json");
    let mut registry: Vec<serde_json::Value> = if registry_path.exists() {
        let text = std::fs::read_to_string(&registry_path).unwrap_or_default();
        serde_json::from_str(&text).unwrap_or_default()
    } else {
        vec![]
    };
    let adopted_at = chrono::Utc::now().to_rfc3339();
    registry.push(serde_json::json!({
        "path": dest_filename,
        "email": client_email,
        "adopted_at": adopted_at,
    }));
    let registry_json = serde_json::to_string_pretty(&registry)?;
    std::fs::write(&registry_path, registry_json)?;
    secure_file(&registry_path)?;

    Ok(format!("SA {} adopted → {}", client_email, dest.display()))
}

fn broker_creds_dir() -> PathBuf {
    std::env::var("CLIX_BROKER_CREDS_DIR")
        .map(PathBuf::from)
        .unwrap_or_else(|_| {
            dirs::data_local_dir()
                .unwrap_or_else(|| PathBuf::from("/var/lib"))
                .join("clix")
                .join("broker")
        })
}

fn secure_dir(path: &Path) -> Result<()> {
    #[cfg(unix)]
    {
        use std::os::unix::fs::PermissionsExt;
        std::fs::set_permissions(path, std::fs::Permissions::from_mode(0o700))
            .map_err(|e| anyhow!("chmod 700 {}: {e}", path.display()))?;
    }
    Ok(())
}

fn secure_file(path: &Path) -> Result<()> {
    #[cfg(unix)]
    {
        use std::os::unix::fs::PermissionsExt;
        std::fs::set_permissions(path, std::fs::Permissions::from_mode(0o600))
            .map_err(|e| anyhow!("chmod 600 {}: {e}", path.display()))?;
    }
    Ok(())
}

/// Install the AppArmor profile for clix-worker by running sudo directly.
/// Called automatically by init on Linux when the restriction is active.
fn install_isolation() -> Result<()> {
    #[cfg(not(target_os = "linux"))]
    anyhow::bail!("--install-isolation is only supported on Linux");

    #[cfg(target_os = "linux")]
    {
        const PROFILE: &str = r#"abi <abi/4.0>,
include <tunables/global>

profile clix-worker @{HOME}/.local/bin/clix-worker flags=(unconfined) {
  userns,
  @{exec_path} mr,
}

profile clix-worker-system /usr/local/bin/clix-worker flags=(unconfined) {
  userns,
  @{exec_path} mr,
}
"#;
        let dest = std::path::Path::new("/etc/apparmor.d/clix-worker");

        // Write profile to a user-owned temp path, then sudo-copy it into place
        let tmp_path = std::path::PathBuf::from(format!("/tmp/clix-worker-apparmor-{}", std::process::id()));
        std::fs::write(&tmp_path, PROFILE)
            .map_err(|e| anyhow!("write profile to temp: {e}"))?;

        println!("Installing AppArmor profile to {} ...", dest.display());

        let cp_status = std::process::Command::new("sudo")
            .args(["cp", &tmp_path.to_string_lossy(), &dest.to_string_lossy()])
            .status()
            .map_err(|e| anyhow!("sudo cp failed: {e}"))?;
        if !cp_status.success() {
            anyhow::bail!("sudo cp exited with status {cp_status}");
        }

        let parse_status = std::process::Command::new("sudo")
            .args(["apparmor_parser", "-r", &dest.to_string_lossy()])
            .status()
            .map_err(|e| anyhow!("sudo apparmor_parser failed: {e}"))?;
        let _ = std::fs::remove_file(&tmp_path);
        if !parse_status.success() {
            anyhow::bail!("sudo apparmor_parser exited with status {parse_status}");
        }

        println!("AppArmor profile loaded — OS isolation is now active for clix-worker.");
        println!("Run `clix doctor` to verify.");
        Ok(())
    }
}

fn locate_binary(name: &str) -> Result<PathBuf> {
    // First: look next to the current executable
    if let Ok(exe) = std::env::current_exe() {
        let candidate = exe.parent().map(|d| d.join(name)).unwrap_or_default();
        if candidate.exists() {
            return Ok(candidate);
        }
    }
    // Second: PATH
    let path_var = std::env::var("PATH").unwrap_or_else(|_| "/usr/bin:/bin".to_string());
    for dir in path_var.split(':') {
        let candidate = Path::new(dir).join(name);
        if candidate.exists() {
            return Ok(candidate);
        }
    }
    Err(anyhow!("`{name}` binary not found next to clix or on PATH"))
}
