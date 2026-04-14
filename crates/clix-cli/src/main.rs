mod cli;
mod commands;
mod dynamic_cli;
mod output;
mod tui;

use anyhow::Result;
use clap::FromArgMatches;
use cli::{Cli, Commands, CapabilitiesCmd, WorkflowCmd, ProfileCmd, ReceiptsCmd, PackCmd, ShimCmd, McpCmd};
use clix_core::execution::run_capability;
use clix_core::loader::{build_registry, load_policy};
use clix_core::policy::evaluate::ExecutionContext;
use clix_core::receipts::ReceiptStore;
use clix_core::state::{home_dir, ClixState};

#[tokio::main]
async fn main() -> Result<()> {
    // Load state + registry early so we can augment the clap tree dynamically.
    // If ~/.clix isn't initialised yet we fall back to a static-only command tree.
    let home = home_dir();
    let state = ClixState::load(home.clone()).ok();
    let registry = state.as_ref().and_then(|s| build_registry(s).ok());

    // Build the static command tree from the derive macro, then append dynamic subcommands.
    let mut cmd = cli::base_command();
    if let Some(ref reg) = registry {
        cmd = dynamic_cli::augment_with_capabilities(reg, cmd);
    }

    let matches = cmd.get_matches();

    // --- Static dispatch ---
    // If the matched subcommand is a known static command, reconstruct the
    // typed enum from ArgMatches and dispatch as before.
    if let Some((name, _)) = matches.subcommand() {
        if cli::STATIC_COMMANDS.contains(&name) {
            let cli = Cli::from_arg_matches(&matches)
                .map_err(|e| anyhow::anyhow!("{e}"))?;
            return dispatch_static(cli).await;
        }
    }

    // --- Dynamic dispatch ---
    // Walk the subcommand chain to reconstruct the capability name.
    if let Some((cap_name, leaf)) = dynamic_cli::resolve_capability_name(&matches) {
        let state = state.ok_or_else(|| anyhow::anyhow!("clix is not initialised — run `clix init` first"))?;
        let registry = registry.ok_or_else(|| anyhow::anyhow!("could not load capability registry"))?;
        let cap = registry.get(&cap_name)
            .ok_or_else(|| anyhow::anyhow!("capability not found: {cap_name}"))?;
        let inputs = dynamic_cli::extract_inputs(leaf, cap);
        let policy = load_policy(&state).unwrap_or_default();
        let store = ReceiptStore::open(&state.receipts_db)?;
        let ctx = ExecutionContext {
            env: state.config.default_env.clone(),
            cwd: std::env::current_dir().unwrap_or_else(|_| std::path::PathBuf::from(".")),
            user: whoami::username(),
            profile: state.config.active_profiles.first().cloned().unwrap_or_else(|| "default".to_string()),
            approver: None,
        };
        let json = leaf.get_flag("json");
        let format_str = matches.get_one::<String>("format").map(|s| s.as_str()).unwrap_or("json");
        let format = if json {
            output::OutputFormat::Json
        } else {
            output::OutputFormat::from_str(format_str)
        };
        let outcome = run_capability(&registry, &policy, state.config.infisical.as_ref(), &store, None, &cap_name, inputs, ctx)?;
        if format != output::OutputFormat::Json && outcome.result.is_some() {
            let result = outcome.result.as_ref().unwrap();
            print!("{}", output::format_value(result, &format));
        } else if json || outcome.result.is_none() {
            println!("{}", serde_json::to_string_pretty(&outcome)?);
        } else {
            println!("ok — receipt {}", outcome.receipt_id);
            if let Some(ref result) = outcome.result {
                if let Some(stdout) = result["stdout"].as_str() {
                    print!("{stdout}");
                }
            }
        }
        return Ok(());
    }

    // No subcommand matched — clap will have already printed help/error
    Ok(())
}

async fn dispatch_static(cli: Cli) -> Result<()> {
    match cli.command {
        Commands::Init { install_shims, adopt_creds } => {
            commands::init::run()?;
            if !install_shims.is_empty() {
                let cmds: Vec<&str> = install_shims.iter().map(String::as_str).collect();
                commands::init::install_shims(&cmds)?;
            }
            for cli in &adopt_creds {
                commands::init::adopt_creds(cli)?;
            }
        }
        Commands::Status { json } => commands::status::run(json)?,
        Commands::Version => println!("clix {}", env!("CARGO_PKG_VERSION")),
        Commands::Run { capability, input, json, dry_run } => commands::run::run(&capability, &input, json, dry_run)?,
        Commands::Capabilities(sub) => match sub {
            CapabilitiesCmd::List { json } => commands::capabilities::list(json)?,
            CapabilitiesCmd::Show { name, json } => commands::capabilities::show(&name, json)?,
            CapabilitiesCmd::Search { query, json } => commands::capabilities::search(&query, json)?,
        },
        Commands::Workflow(sub) => match sub {
            WorkflowCmd::List { json } => commands::workflow::list(json)?,
            WorkflowCmd::Run { name, input, json } => commands::workflow::run_wf(&name, &input, json)?,
        },
        Commands::Profile(sub) => match sub {
            ProfileCmd::List { json } => commands::profile::list(json)?,
            ProfileCmd::Show { name, .. } => { let _ = name; println!("(use pack show for details)"); }
            ProfileCmd::Activate { name } => commands::profile::activate(&name)?,
            ProfileCmd::Deactivate { name } => commands::profile::deactivate(&name)?,
        },
        Commands::Receipts(sub) => match sub {
            ReceiptsCmd::List { limit, status, json } => commands::receipts::list(limit, status.as_deref(), json)?,
            ReceiptsCmd::Show { id, json } => commands::receipts::show(&id, json)?,
            ReceiptsCmd::Tail => commands::receipts::tail()?,
        },
        Commands::Serve { socket, http } => commands::serve::run(socket, http).await?,
        Commands::Tui => commands::tui::run()?,
        Commands::Doctor { json } => commands::doctor::run(json)?,
        Commands::Shim(sub) => match sub {
            ShimCmd::List { json } => commands::shim::list(json)?,
            ShimCmd::Uninstall { command } => commands::shim::uninstall(&command)?,
        },
        Commands::Mcp(sub) => match sub {
            McpCmd::Call { method, params } => commands::mcp::call(&method, params.as_deref()).await?,
        },
        Commands::Pack(sub) => match sub {
            PackCmd::List { json, available } => commands::pack::list(json, available)?,
            PackCmd::Show { name, json } => commands::pack::show(&name, json)?,
            PackCmd::Discover { path, json } => commands::pack::discover(&path, json)?,
            PackCmd::Validate { path } => commands::pack::validate(&path)?,
            PackCmd::Diff { installed, new_path, json } => commands::pack::diff(&installed, &new_path, json)?,
            PackCmd::Install { path } => commands::pack::install(&path)?,
            PackCmd::Bundle { path } => commands::pack::bundle(&path)?,
            PackCmd::Publish { path } => commands::pack::publish(&path)?,
            PackCmd::Scaffold { name, preset, command } => commands::pack::scaffold(&name, &preset, command.as_deref())?,
            PackCmd::Onboard { name, command, json } => commands::pack::onboard(&name, &command, json)?,
        },
    }
    Ok(())
}
