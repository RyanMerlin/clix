use clap::{Parser, Subcommand};

/// All static top-level subcommand names. Dynamic capability subcommands must not use these.
pub const STATIC_COMMANDS: &[&str] = &[
    "init", "status", "version", "run", "capabilities",
    "workflow", "profile", "receipts", "serve", "pack",
];

#[derive(Parser)]
#[command(name = "clix", version, about = "Policy-first CLI control plane for agentic tool use")]
pub struct Cli {
    #[command(subcommand)]
    pub command: Commands,
}

/// Returns the static clap `Command` built from the derive macros, in builder form.
/// Call `dynamic_cli::augment_with_capabilities()` on the result to append dynamic subcommands.
pub fn base_command() -> clap::Command {
    use clap::CommandFactory;
    // Add a global --format flag so dynamic leaf commands inherit it.
    Cli::command().arg(
        clap::Arg::new("format")
            .long("format")
            .global(true)
            .value_name("FORMAT")
            .help("Output format: json (default), table, yaml, csv"),
    )
}

#[derive(Subcommand)]
pub enum Commands {
    /// Initialize clix in ~/.clix
    Init,
    /// Show clix status and configuration
    Status { #[arg(long)] json: bool },
    /// Print version information
    Version,
    /// Run a capability
    Run {
        capability: String,
        #[arg(long = "input", short = 'i', value_name = "KEY=VALUE")]
        input: Vec<String>,
        #[arg(long)] json: bool,
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
        #[arg(long)] socket: Option<String>,
        #[arg(long)] http: Option<String>,
    },
    /// Manage packs
    #[command(subcommand)]
    Pack(PackCmd),
}

#[derive(Subcommand)]
pub enum CapabilitiesCmd {
    List { #[arg(long)] json: bool },
    Show { name: String, #[arg(long)] json: bool },
}

#[derive(Subcommand)]
pub enum WorkflowCmd {
    List { #[arg(long)] json: bool },
    Run {
        name: String,
        #[arg(long = "input", short = 'i', value_name = "KEY=VALUE")]
        input: Vec<String>,
        #[arg(long)] json: bool,
    },
}

#[derive(Subcommand)]
pub enum ProfileCmd {
    List { #[arg(long)] json: bool },
    Show { name: String, #[arg(long)] json: bool },
    Activate { name: String },
    Deactivate { name: String },
}

#[derive(Subcommand)]
pub enum ReceiptsCmd {
    List {
        #[arg(long, default_value = "50")] limit: usize,
        #[arg(long)] status: Option<String>,
        #[arg(long)] json: bool,
    },
    Show { id: String, #[arg(long)] json: bool },
    Tail,
}

#[derive(Subcommand)]
pub enum PackCmd {
    List { #[arg(long)] json: bool },
    Show { name: String, #[arg(long)] json: bool },
    Discover { path: String, #[arg(long)] json: bool },
    Validate { path: String },
    Diff { installed: String, new_path: String, #[arg(long)] json: bool },
    Install { path: String },
    Bundle { path: String },
    Publish { path: String },
    Scaffold {
        name: String,
        #[arg(long, default_value = "read-only")] preset: String,
        #[arg(long)] command: Option<String>,
    },
    Onboard {
        name: String,
        #[arg(long)] command: String,
        #[arg(long)] json: bool,
    },
}
