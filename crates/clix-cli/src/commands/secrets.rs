use clap::Subcommand;
use anyhow::Result;
use clix_core::secrets::{test_connectivity, preview};
use clix_core::state::{ClixState, InfisicalConfig};

#[derive(Subcommand, Debug)]
pub enum SecretsCmd {
    /// Show current Infisical configuration (obfuscated)
    Show {
        #[arg(long)] yaml: bool,
        #[arg(long)] json: bool,
    },
    /// Test connectivity to Infisical
    Test,
    /// Configure Infisical credentials (updates the active profile, or creates "default")
    Set {
        #[arg(long)] site_url: Option<String>,
        #[arg(long)] project_id: Option<String>,
        #[arg(long, default_value = "dev")] env: String,
        /// Read client_id and client_secret from stdin (one per line, for CI use)
        #[arg(long)] stdin: bool,
    },
    /// Remove stored Infisical credentials from the active profile
    Unset,
    /// List secrets at a path in Infisical
    List {
        #[arg(default_value = "/")] path: String,
        #[arg(long)] project: Option<String>,
        #[arg(long)] env: Option<String>,
        #[arg(long)] plain: bool,
    },
    /// Recursive tree view of Infisical folder structure
    Tree {
        #[arg(default_value = "/")] path: String,
        #[arg(long, default_value_t = 3)] depth: u8,
        #[arg(long)] project: Option<String>,
        #[arg(long)] env: Option<String>,
    },
}

pub fn run_secrets(cmd: SecretsCmd, state: &ClixState) -> Result<()> {
    match cmd {
        SecretsCmd::Show { yaml, json } => cmd_show(state, yaml, json),
        SecretsCmd::Test => cmd_test(state),
        SecretsCmd::Set { site_url, project_id, env, stdin } => cmd_set(state, site_url, project_id, env, stdin),
        SecretsCmd::Unset => cmd_unset(state),
        SecretsCmd::List { path, project, env, plain } => cmd_list(state, &path, project.as_deref(), env.as_deref(), plain),
        SecretsCmd::Tree { path, depth, project, env } => cmd_tree(state, &path, depth, project.as_deref(), env.as_deref()),
    }
}

fn cmd_show(state: &ClixState, use_yaml: bool, use_json: bool) -> Result<()> {
    let profiles = state.config.infisical();
    let cfg = profiles.active_profile();

    // Determine credential source for the active profile
    #[cfg(target_os = "linux")]
    let from_keyring = {
        let active_name = state.config.active_infisical.as_deref().unwrap_or("default");
        clix_core::secrets::keyring::load_credentials(active_name).is_some()
    };
    #[cfg(not(target_os = "linux"))]
    let from_keyring = false;

    let source = if from_keyring { "keyring" } else { "config.yaml" };

    let site_url = cfg.map(|c| c.site_url.as_str()).unwrap_or("(not set)");
    let client_id = cfg.and_then(|c| c.client_id.as_deref()).unwrap_or("");
    let client_secret = cfg.and_then(|c| c.client_secret.as_deref()).unwrap_or("");
    let project_id = cfg.and_then(|c| c.default_project_id.as_deref()).unwrap_or("");
    let environment = cfg.map(|c| c.default_environment.as_str()).unwrap_or("dev");
    let active_name = state.config.active_infisical.as_deref().unwrap_or("(none)");

    if use_json {
        let v = serde_json::json!({
            "active_profile": active_name,
            "site_url": site_url,
            "client_id": preview(client_id),
            "client_id_source": source,
            "client_secret": preview(client_secret),
            "client_secret_source": source,
            "project_id": preview(project_id),
            "environment": environment,
        });
        println!("{}", serde_json::to_string_pretty(&v)?);
    } else if use_yaml {
        let v = serde_json::json!({
            "active_profile": active_name,
            "site_url": site_url,
            "client_id": preview(client_id),
            "client_id_source": source,
            "client_secret": preview(client_secret),
            "client_secret_source": source,
            "project_id": preview(project_id),
            "environment": environment,
        });
        println!("{}", serde_yaml::to_string(&v)?);
    } else {
        println!("Infisical configuration (profile: {}):", active_name);
        println!("  site_url        {}", site_url);
        println!("  client_id       {} ({})", preview(client_id), source);
        println!("  client_secret   {} ({})", preview(client_secret), source);
        println!("  project_id      {}", preview(project_id));
        println!("  environment     {}", environment);
    }
    Ok(())
}

fn cmd_test(state: &ClixState) -> Result<()> {
    let profiles = state.config.infisical();
    let cfg = profiles.active_profile()
        .ok_or_else(|| anyhow::anyhow!("No active Infisical profile — run `clix secrets set` or `clix infisical add`"))?;

    println!("Testing Infisical connectivity to {} …", cfg.site_url);
    let report = test_connectivity(cfg);

    if report.auth_ok {
        println!("  ✓ auth ok");
        println!("  ✓ site reachable");
        if report.workspace_reachable {
            println!("  ✓ workspace reachable ({} root folders)", report.root_folder_count);
        } else {
            println!("  - workspace: no project_id configured");
        }
        if let Some(ttl) = report.token_expires_in {
            println!("  token TTL: {}s", ttl);
        }
        println!("  latency: {}ms", report.latency_ms);
    } else {
        eprintln!("  ✗ error: {}", report.error.as_deref().unwrap_or("unknown"));
        std::process::exit(1);
    }
    Ok(())
}

fn cmd_set(_state: &ClixState, site_url: Option<String>, project_id: Option<String>, env: String, from_stdin: bool) -> Result<()> {
    let (client_id, client_secret) = if from_stdin {
        let mut line1 = String::new();
        let mut line2 = String::new();
        std::io::stdin().read_line(&mut line1)?;
        std::io::stdin().read_line(&mut line2)?;
        (line1.trim().to_string(), line2.trim().to_string())
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
                eprintln!("(warning: hidden input unavailable, secret will be visible)");
                let mut buf = String::new();
                let _ = std::io::stdin().read_line(&mut buf);
                buf.trim().to_string()
            });
        (id, secret)
    };

    let home = clix_core::state::home_dir();
    let mut new_state = ClixState::load(home)?;

    // Determine which profile to update (active, or create "default")
    let profile_name = new_state.config.active_infisical
        .clone()
        .unwrap_or_else(|| "default".to_string());

    let cfg = new_state.config.infisical_profiles
        .entry(profile_name.clone())
        .or_insert_with(|| InfisicalConfig {
            site_url: "https://app.infisical.com".to_string(),
            client_id: None,
            client_secret: None,
            default_project_id: None,
            default_environment: "dev".to_string(),
        });

    if let Some(u) = site_url { cfg.site_url = u; }
    if let Some(p) = project_id { cfg.default_project_id = Some(p); }
    cfg.default_environment = env;

    if new_state.config.active_infisical.is_none() {
        new_state.config.active_infisical = Some(profile_name.clone());
    }

    // On Linux, try keyring first
    #[cfg(target_os = "linux")]
    {
        use clix_core::secrets::keyring::{store_credentials, KeyringResult};
        match store_credentials(&profile_name, &client_id, &client_secret) {
            KeyringResult::Ok => {
                println!("Credentials stored in keyring.");
            }
            KeyringResult::Unavailable(e) => {
                eprintln!("Keyring unavailable ({}), storing in config.yaml", e);
                let cfg2 = new_state.config.infisical_profiles.get_mut(&profile_name).unwrap();
                cfg2.client_id = Some(client_id.clone());
                cfg2.client_secret = Some(client_secret.clone());
            }
        }
    }
    #[cfg(not(target_os = "linux"))]
    {
        let cfg2 = new_state.config.infisical_profiles.get_mut(&profile_name).unwrap();
        cfg2.client_id = Some(client_id.clone());
        cfg2.client_secret = Some(client_secret.clone());
    }

    new_state.save_config()?;
    println!("Configuration saved (profile: {}).", profile_name);

    // Run connectivity test
    let cfg_ref = new_state.config.infisical_profiles.get(&profile_name).unwrap();
    let report = test_connectivity(cfg_ref);
    if report.auth_ok {
        println!("✓ Infisical connected ({}ms)", report.latency_ms);
    } else {
        eprintln!("✗ Infisical: {}", report.error.as_deref().unwrap_or("auth failed").chars().take(60).collect::<String>());
    }
    Ok(())
}

fn cmd_unset(state: &ClixState) -> Result<()> {
    let profile_name = state.config.active_infisical
        .as_deref()
        .unwrap_or("default")
        .to_string();

    #[cfg(target_os = "linux")]
    {
        use clix_core::secrets::keyring::{delete_credentials, KeyringResult};
        match delete_credentials(&profile_name) {
            KeyringResult::Ok => println!("Keyring credentials removed."),
            KeyringResult::Unavailable(e) => eprintln!("Keyring unavailable: {}", e),
        }
    }

    let home = clix_core::state::home_dir();
    let mut new_state = ClixState::load(home)?;
    if let Some(cfg) = new_state.config.infisical_profiles.get_mut(&profile_name) {
        cfg.client_id = None;
        cfg.client_secret = None;
    }
    new_state.save_config()?;
    println!("Infisical credentials removed from profile '{}'.", profile_name);
    Ok(())
}

fn cmd_list(state: &ClixState, path: &str, project: Option<&str>, env: Option<&str>, plain: bool) -> Result<()> {
    let profiles = state.config.infisical();
    let cfg = profiles.active_profile()
        .ok_or_else(|| anyhow::anyhow!("Infisical not configured"))?;
    let project_id = project
        .or_else(|| cfg.default_project_id.as_deref())
        .ok_or_else(|| anyhow::anyhow!("no project_id — use --project or configure default"))?;
    let environment = env.unwrap_or(&cfg.default_environment);

    let folders = clix_core::secrets::list_infisical_folders(cfg, project_id, environment, path)
        .unwrap_or_default();
    let secrets = clix_core::secrets::list_infisical_secrets(cfg, project_id, environment, path)
        .unwrap_or_default();

    for f in &folders {
        if plain {
            println!("{}/", f);
        } else {
            println!("📁 {}/", f);
        }
    }
    for s in &secrets {
        if plain {
            println!("{}", s);
        } else {
            println!("🔑 {}", s);
        }
    }
    Ok(())
}

fn cmd_tree(state: &ClixState, path: &str, depth: u8, project: Option<&str>, env: Option<&str>) -> Result<()> {
    let profiles = state.config.infisical();
    let cfg = profiles.active_profile()
        .ok_or_else(|| anyhow::anyhow!("Infisical not configured"))?;
    let project_id = project
        .or_else(|| cfg.default_project_id.as_deref())
        .ok_or_else(|| anyhow::anyhow!("no project_id — use --project or configure default"))?;
    let environment = env.unwrap_or(&cfg.default_environment);

    print_tree(cfg, project_id, environment, path, 0, depth);
    Ok(())
}

fn print_tree(cfg: &clix_core::state::InfisicalConfig, project_id: &str, environment: &str, path: &str, indent: u8, max_depth: u8) {
    let prefix = "  ".repeat(indent as usize);

    let secrets = clix_core::secrets::list_infisical_secrets(cfg, project_id, environment, path)
        .unwrap_or_default();
    for s in &secrets {
        println!("{}🔑 {}", prefix, s);
    }

    if indent >= max_depth { return; }

    let folders = clix_core::secrets::list_infisical_folders(cfg, project_id, environment, path)
        .unwrap_or_default();
    for f in &folders {
        println!("{}📁 {}/", prefix, f);
        let sub_path = if path == "/" {
            format!("/{}/", f)
        } else {
            format!("{}{}/", path.trim_end_matches('/'), f)
        };
        print_tree(cfg, project_id, environment, &sub_path, indent + 1, max_depth);
    }
}
