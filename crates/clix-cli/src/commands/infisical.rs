use clap::Subcommand;
use anyhow::Result;
use clix_core::state::{ClixState, InfisicalConfig, home_dir};
use clix_core::secrets::test_connectivity;

#[derive(Subcommand, Debug)]
pub enum InfisicalCmd {
    /// List all configured Infisical profiles
    List {
        #[arg(long)] json: bool,
    },
    /// Add a new Infisical profile (interactive)
    Add {
        /// Profile name (e.g. "work", "personal")
        name: String,
        #[arg(long)] site_url: Option<String>,
        #[arg(long)] project_id: Option<String>,
        #[arg(long, default_value = "dev")] env: String,
        /// Read client_id and client_secret from stdin (one per line, for CI)
        #[arg(long)] stdin: bool,
    },
    /// Set the active Infisical profile
    Use {
        name: String,
    },
    /// Remove an Infisical profile
    Remove {
        name: String,
        #[arg(long)] yes: bool,
    },
    /// Test connectivity for a profile (defaults to active)
    Test {
        name: Option<String>,
    },
    /// Edit an existing Infisical profile (interactive)
    Edit {
        name: String,
        #[arg(long)] site_url: Option<String>,
        #[arg(long)] project_id: Option<String>,
        #[arg(long)] env: Option<String>,
    },
}

pub fn run_infisical(cmd: InfisicalCmd) -> Result<()> {
    match cmd {
        InfisicalCmd::List { json } => cmd_list(json),
        InfisicalCmd::Add { name, site_url, project_id, env, stdin } => cmd_add(&name, site_url, project_id, env, stdin),
        InfisicalCmd::Use { name } => cmd_use(&name),
        InfisicalCmd::Remove { name, yes } => cmd_remove(&name, yes),
        InfisicalCmd::Test { name } => cmd_test(name.as_deref()),
        InfisicalCmd::Edit { name, site_url, project_id, env } => cmd_edit(&name, site_url, project_id, env),
    }
}

fn load_state() -> Result<ClixState> {
    Ok(ClixState::load(home_dir())?)
}

fn cmd_list(use_json: bool) -> Result<()> {
    let state = load_state()?;
    let active = state.config.active_infisical.as_deref().unwrap_or("");

    if use_json {
        let profiles: Vec<serde_json::Value> = state.config.infisical_profiles.iter().map(|(name, cfg)| {
            serde_json::json!({
                "name": name,
                "active": name == active,
                "site_url": cfg.site_url,
                "project_id": cfg.default_project_id,
                "environment": cfg.default_environment,
            })
        }).collect();
        println!("{}", serde_json::to_string_pretty(&profiles)?);
    } else if state.config.infisical_profiles.is_empty() {
        println!("No Infisical profiles configured.");
        println!("Run `clix infisical add <name>` to add one.");
    } else {
        println!("{:<16} {:<8} {:<36} {}", "NAME", "ACTIVE", "SITE", "ENV");
        for (name, cfg) in &state.config.infisical_profiles {
            println!("{:<16} {:<8} {:<36} {}",
                name,
                if name == active { "*" } else { "" },
                cfg.site_url,
                cfg.default_environment,
            );
        }
    }
    Ok(())
}

fn cmd_add(name: &str, site_url: Option<String>, project_id: Option<String>, env: String, from_stdin: bool) -> Result<()> {
    let mut state = load_state()?;

    if state.config.infisical_profiles.contains_key(name) {
        anyhow::bail!("Profile '{}' already exists — use `clix infisical edit {}` to update it", name, name);
    }

    let site = site_url.unwrap_or_else(|| "https://app.infisical.com".to_string());

    let (client_id, client_secret) = read_credentials(from_stdin)?;

    let cfg = InfisicalConfig {
        site_url: site,
        client_id: None,
        client_secret: None,
        service_token: None,
        default_project_id: project_id,
        default_environment: env,
    };
    state.config.infisical_profiles.insert(name.to_string(), cfg);

    if state.config.active_infisical.is_none() {
        state.config.active_infisical = Some(name.to_string());
        println!("Profile '{}' set as active (first profile).", name);
    }

    store_creds_or_config(&mut state, name, &client_id, &client_secret)?;
    state.save_config()?;
    println!("Profile '{}' added.", name);

    let cfg_ref = state.config.infisical_profiles.get(name).unwrap();
    run_connectivity_check(cfg_ref);
    Ok(())
}

fn cmd_use(name: &str) -> Result<()> {
    let mut state = load_state()?;
    if !state.config.infisical_profiles.contains_key(name) {
        anyhow::bail!("Profile '{}' not found — run `clix infisical list` to see available profiles", name);
    }
    state.config.active_infisical = Some(name.to_string());
    state.save_config()?;
    println!("Active Infisical profile set to '{}'.", name);
    Ok(())
}

fn cmd_remove(name: &str, confirmed: bool) -> Result<()> {
    let mut state = load_state()?;
    if !state.config.infisical_profiles.contains_key(name) {
        anyhow::bail!("Profile '{}' not found", name);
    }

    if !confirmed {
        print!("Remove profile '{}'? This will delete its credentials. [y/N] ", name);
        use std::io::Write;
        std::io::stdout().flush()?;
        let mut buf = String::new();
        std::io::stdin().read_line(&mut buf)?;
        if !buf.trim().eq_ignore_ascii_case("y") {
            println!("Cancelled.");
            return Ok(());
        }
    }

    state.config.infisical_profiles.remove(name);

    // Clear active if it was this profile
    if state.config.active_infisical.as_deref() == Some(name) {
        state.config.active_infisical = state.config.infisical_profiles.keys().next().cloned();
        if let Some(ref new_active) = state.config.active_infisical {
            println!("Active profile switched to '{}'.", new_active);
        } else {
            println!("No remaining profiles — active profile cleared.");
        }
    }

    #[cfg(target_os = "linux")]
    {
        use clix_core::secrets::keyring::{delete_credentials, KeyringResult};
        match delete_credentials(name) {
            KeyringResult::Ok => println!("Keyring credentials removed."),
            KeyringResult::Unavailable(_) => {}
        }
    }

    state.save_config()?;
    println!("Profile '{}' removed.", name);
    Ok(())
}

fn cmd_test(name: Option<&str>) -> Result<()> {
    let state = load_state()?;
    let profiles = state.config.infisical();

    let (profile_name, cfg) = if let Some(n) = name {
        let cfg = profiles.resolve(Some(n))
            .ok_or_else(|| anyhow::anyhow!("Profile '{}' not found", n))?;
        (n, cfg)
    } else {
        let active = state.config.active_infisical.as_deref().unwrap_or("default");
        let cfg = profiles.active_profile()
            .ok_or_else(|| anyhow::anyhow!("No active Infisical profile — run `clix infisical add`"))?;
        (active, cfg)
    };

    println!("Testing profile '{}' → {} …", profile_name, cfg.site_url);
    let report = test_connectivity(cfg);

    if report.auth_ok {
        println!("  ✓ auth ok  ({}ms)", report.latency_ms);
        if report.workspace_reachable {
            println!("  ✓ workspace reachable ({} root folders)", report.root_folder_count);
        } else {
            println!("  - workspace: no project_id configured");
        }
        if let Some(ttl) = report.token_expires_in {
            println!("  token TTL: {}s", ttl);
        }
    } else {
        eprintln!("  ✗ {}", report.error.as_deref().unwrap_or("auth failed"));
        std::process::exit(1);
    }
    Ok(())
}

fn cmd_edit(name: &str, site_url: Option<String>, project_id: Option<String>, env: Option<String>) -> Result<()> {
    let mut state = load_state()?;

    if !state.config.infisical_profiles.contains_key(name) {
        anyhow::bail!("Profile '{}' not found — use `clix infisical add {}` to create it", name, name);
    }

    let changed = site_url.is_some() || project_id.is_some() || env.is_some();

    {
        let cfg = state.config.infisical_profiles.get_mut(name).unwrap();
        if let Some(u) = site_url { cfg.site_url = u; }
        if let Some(p) = project_id { cfg.default_project_id = Some(p); }
        if let Some(e) = env { cfg.default_environment = e; }
    }

    // If no flags, prompt for new credentials
    if !changed {
        println!("Updating credentials for profile '{}'.", name);
        println!("Leave blank to keep existing values. Press Ctrl-C to cancel.");
        let (client_id, client_secret) = read_credentials(false)?;
        if !client_id.is_empty() || !client_secret.is_empty() {
            store_creds_or_config(&mut state, name, &client_id, &client_secret)?;
        }
    }

    state.save_config()?;
    println!("Profile '{}' updated.", name);
    Ok(())
}

fn read_credentials(from_stdin: bool) -> Result<(String, String)> {
    if from_stdin {
        let mut line1 = String::new();
        let mut line2 = String::new();
        std::io::stdin().read_line(&mut line1)?;
        std::io::stdin().read_line(&mut line2)?;
        Ok((line1.trim().to_string(), line2.trim().to_string()))
    } else {
        let id = {
            print!("client_id: ");
            use std::io::Write;
            std::io::stdout().flush()?;
            let mut buf = String::new();
            std::io::stdin().read_line(&mut buf)?;
            buf.trim().to_string()
        };
        let secret = rpassword::prompt_password("client_secret: ")
            .unwrap_or_else(|_| {
                eprintln!("(warning: hidden input unavailable)");
                let mut buf = String::new();
                let _ = std::io::stdin().read_line(&mut buf);
                buf.trim().to_string()
            });
        Ok((id, secret))
    }
}

fn store_creds_or_config(state: &mut ClixState, name: &str, client_id: &str, client_secret: &str) -> Result<()> {
    #[cfg(target_os = "linux")]
    {
        use clix_core::secrets::keyring::{store_credentials, KeyringResult};
        match store_credentials(name, client_id, client_secret) {
            KeyringResult::Ok => return Ok(()),
            KeyringResult::Unavailable(e) => {
                eprintln!("Keyring unavailable ({}), storing in config.yaml", e);
            }
        }
    }
    let cfg = state.config.infisical_profiles.get_mut(name).unwrap();
    cfg.client_id = Some(client_id.to_string());
    cfg.client_secret = Some(client_secret.to_string());
    Ok(())
}

fn run_connectivity_check(cfg: &InfisicalConfig) {
    let report = test_connectivity(cfg);
    if report.auth_ok {
        println!("✓ Infisical connected ({}ms)", report.latency_ms);
    } else {
        eprintln!("✗ {}", report.error.as_deref().unwrap_or("auth failed").chars().take(80).collect::<String>());
    }
}
