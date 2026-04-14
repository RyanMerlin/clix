use anyhow::{anyhow, Result};
use std::path::{Path, PathBuf};
use clix_core::state::{ClixConfig, ClixState, home_dir};
use clix_core::packs::seed::seed_builtin_packs;

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

    // Seed built-in packs
    let packs_src = [
        std::env::current_exe().ok().and_then(|p| p.parent().map(|d| d.join("packs"))),
        Some(std::path::PathBuf::from("packs")),
    ].into_iter().flatten().find(|p| p.exists());
    if let Some(src) = packs_src {
        seed_builtin_packs(&state.packs_dir, &src)?;
        println!("Seeded built-in packs");
    }

    // Auto-activate base profile if nothing is active
    if config.active_profiles.is_empty() {
        config.active_profiles.push("base".to_string());
        println!("Activated default profile: base");
    }

    // Write config (single write)
    let yaml = serde_yaml::to_string(&config)?;
    std::fs::write(&state.config_path, yaml)?;

    println!("clix initialized at {}", home.display());
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

    // Suggest PATH update
    let bin_dir_str = bin_dir.to_string_lossy();
    let path_var = std::env::var("PATH").unwrap_or_default();
    if !path_var.split(':').any(|p| p == bin_dir_str.as_ref()) {
        println!();
        println!("Add the following to your shell profile to activate the shims:");
        println!("  export PATH=\"{}:$PATH\"", bin_dir_str);
        println!();
        println!("Or for fish:");
        println!("  set -Ux fish_user_paths \"{}\" $fish_user_paths", bin_dir_str);
    }

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
