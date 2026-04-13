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
        Commands::Version => println!("clix {}", env!("CARGO_PKG_VERSION")),
        Commands::Run { capability, input, json } => commands::run::run(&capability, &input, json)?,
        Commands::Capabilities(sub) => match sub {
            CapabilitiesCmd::List { json } => commands::capabilities::list(json)?,
            CapabilitiesCmd::Show { name, json } => commands::capabilities::show(&name, json)?,
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
        Commands::Pack(sub) => match sub {
            PackCmd::List { json } => commands::pack::list(json)?,
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
