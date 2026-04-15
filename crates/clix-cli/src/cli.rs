use clap::{Parser, Subcommand};

/// All static top-level subcommand names. Dynamic capability subcommands must not use these.
pub const STATIC_COMMANDS: &[&str] = &[
    "init", "status", "version", "run", "capabilities",
    "workflow", "profile", "receipts", "serve", "pack", "tui",
    "doctor", "shim", "mcp", "tools", "secrets",
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
    Init {
        /// Install PATH shims for the listed CLI commands (e.g. --install-shims gcloud kubectl).
        /// Shims are placed in ~/.clix/bin/ and intercept direct CLI invocations by the agent.
        #[arg(long = "install-shims", value_name = "COMMAND", num_args = 0..)]
        install_shims: Vec<String>,
        /// Migrate credentials for the listed CLIs into the broker-owned credential store
        /// (e.g. --adopt-creds gcloud kubectl). This moves the creds out of the agent's reach.
        #[arg(long = "adopt-creds", value_name = "CLI", num_args = 0..)]
        adopt_creds: Vec<String>,
        /// Write .mcp.json and CLAUDE.md integration block for Claude Code (project-scoped).
        /// Run from the project root. Merges with existing .mcp.json if present.
        #[arg(long = "claude-code")]
        claude_code: bool,
        /// Write .cursor/mcp.json for Cursor (project-scoped).
        /// Run from the project root. Merges with existing .cursor/mcp.json if present.
        #[arg(long = "cursor")]
        cursor: bool,
    },
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
        /// Evaluate policy and show what would happen without actually executing
        #[arg(long)] dry_run: bool,
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
    /// Launch interactive TUI
    Tui,
    /// Gateway health check for agents
    Doctor { #[arg(long)] json: bool },
    /// Export capability definitions in AI SDK formats for Claude, Gemini, or OpenAI
    #[command(subcommand)]
    Tools(ToolsCmd),
    /// Manage PATH shims
    #[command(subcommand)]
    Shim(ShimCmd),
    /// One-shot JSON-RPC call to the MCP dispatch layer (no server needed)
    #[command(subcommand)]
    Mcp(McpCmd),
    /// Manage Infisical secrets configuration
    #[command(subcommand)]
    Secrets(crate::commands::secrets::SecretsCmd),
}

#[derive(Subcommand)]
pub enum CapabilitiesCmd {
    List { #[arg(long)] json: bool },
    Show { name: String, #[arg(long)] json: bool },
    /// Search capabilities by name or description
    Search { query: String, #[arg(long)] json: bool },
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
pub enum ToolsCmd {
    /// Export capability definitions as an AI SDK tools/functions array
    Export {
        /// Output format: claude, gemini, openai, two-tool
        #[arg(long, short, default_value = "claude")]
        format: String,
        /// Limit to capabilities in this namespace (e.g. git, gcloud)
        #[arg(long)]
        namespace: Option<String>,
        /// Include all capabilities (flat list, no namespace filtering)
        #[arg(long)]
        all: bool,
    },
}

#[derive(Subcommand)]
pub enum ShimCmd {
    /// List installed shims
    List { #[arg(long)] json: bool },
    /// Remove a shim
    Uninstall { command: String },
}

#[derive(Subcommand)]
pub enum McpCmd {
    /// Send a single JSON-RPC request to the in-process MCP dispatcher and print the response
    Call {
        method: String,
        #[arg(long, value_name = "JSON")]
        params: Option<String>,
    },
}

#[derive(Subcommand)]
pub enum PackCmd {
    List {
        #[arg(long)] json: bool,
        #[arg(long)] available: bool,
    },
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
