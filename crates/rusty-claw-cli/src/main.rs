use clap::{Parser, Subcommand};

#[derive(Parser)]
#[command(
    name = "rusty-claw",
    about = "Personal AI assistant gateway — the full OpenClaw experience in a single Rust binary",
    version
)]
struct Cli {
    #[command(subcommand)]
    command: Commands,

    /// Config file path
    #[arg(short, long, global = true)]
    config: Option<String>,

    /// Enable verbose logging
    #[arg(short, long, global = true)]
    verbose: bool,
}

#[derive(Subcommand)]
enum Commands {
    /// Start the gateway server
    Gateway {
        /// Port to listen on (default: 18789)
        #[arg(long)]
        port: Option<u16>,

        /// Enable the Control UI
        #[arg(long)]
        ui: bool,
    },

    /// Chat with the agent (one-shot or interactive)
    Agent {
        /// Message to send (omit for interactive mode)
        #[arg(short, long)]
        message: Option<String>,

        /// Model to use
        #[arg(long)]
        model: Option<String>,

        /// Thinking level
        #[arg(long)]
        thinking: Option<String>,
    },

    /// Interactive setup wizard
    Onboard {
        /// Also install the system daemon
        #[arg(long)]
        install_daemon: bool,
    },

    /// Configuration management
    Config {
        #[command(subcommand)]
        action: ConfigAction,
    },

    /// Show system status
    Status,

    /// Diagnose common issues
    Doctor,

    /// Migrate from an OpenClaw config
    Migrate {
        /// Path to openclaw.json
        #[arg(long)]
        from: String,
    },

    /// Channel management
    Channels {
        #[command(subcommand)]
        action: ChannelAction,
    },

    /// Session management
    Sessions {
        #[command(subcommand)]
        action: SessionAction,
    },

    /// Scheduled task management
    Cron {
        #[command(subcommand)]
        action: CronAction,
    },

    /// Manage DM pairing approvals
    Pairing {
        #[command(subcommand)]
        action: PairingAction,
    },
}

#[derive(Subcommand)]
enum ConfigAction {
    /// Show current configuration
    Show,
    /// Get a specific config value
    Get { key: String },
    /// Set a config value
    Set { key: String, value: String },
}

#[derive(Subcommand)]
enum ChannelAction {
    /// Log in to a channel
    Login { channel: Option<String> },
    /// Show channel status
    Status,
    /// Log out from a channel
    Logout { channel: String },
}

#[derive(Subcommand)]
enum SessionAction {
    /// List all sessions
    List,
    /// Reset a session
    Reset { session: Option<String> },
    /// Delete a session
    Delete { session: String },
}

#[derive(Subcommand)]
enum CronAction {
    /// List scheduled jobs
    List,
    /// Add a scheduled job
    Add { schedule: String, task: String },
    /// Remove a scheduled job
    Remove { id: String },
}

#[derive(Subcommand)]
enum PairingAction {
    /// Approve a pairing request
    Approve { channel: String, code: String },
    /// Reject a pairing request
    Reject { channel: String, code: String },
    /// List pending pairing requests
    List,
}

#[tokio::main]
async fn main() -> anyhow::Result<()> {
    let cli = Cli::parse();

    // Initialize logging
    let filter = if cli.verbose { "debug" } else { "info" };
    tracing_subscriber::fmt()
        .with_env_filter(
            tracing_subscriber::EnvFilter::try_from_default_env()
                .unwrap_or_else(|_| tracing_subscriber::EnvFilter::new(filter)),
        )
        .init();

    // Load config
    let config_path = cli
        .config
        .map(std::path::PathBuf::from)
        .unwrap_or_else(rusty_claw_core::config::Config::config_dir);

    let config = rusty_claw_core::config::Config::load(&config_path)?;

    match cli.command {
        Commands::Gateway { port, ui } => {
            let port = port.unwrap_or_else(|| config.gateway_port());
            tracing::info!("Starting Rusty Claw gateway on port {port}");
            if ui {
                tracing::info!("Control UI enabled");
            }
            // TODO: Start gateway server
            tracing::warn!("Gateway not yet implemented — coming in Phase 1");
            // Block forever for now
            tokio::signal::ctrl_c().await?;
        }
        Commands::Agent { message: _, model: _, thinking: _ } => {
            tracing::info!("Starting agent");
            // TODO: Run agent (one-shot or interactive)
            tracing::warn!("Agent not yet implemented — coming in Phase 1");
        }
        Commands::Onboard { install_daemon: _ } => {
            tracing::info!("Starting onboarding wizard");
            // TODO: Interactive setup wizard
            tracing::warn!("Onboard not yet implemented — coming in Phase 1");
        }
        Commands::Status => {
            println!("Rusty Claw v{}", env!("CARGO_PKG_VERSION"));
            println!("Config: {}", config_path.display());
            println!("Workspace: {}", config.workspace_dir().display());
            println!("Gateway port: {}", config.gateway_port());
            println!("Status: not running");
        }
        Commands::Doctor => {
            tracing::info!("Running diagnostics");
            // TODO: Implement doctor checks
            tracing::warn!("Doctor not yet implemented — coming in Phase 2");
        }
        Commands::Migrate { from } => {
            tracing::info!("Migrating from OpenClaw config: {from}");
            // TODO: Config migration
            tracing::warn!("Migrate not yet implemented — coming in Phase 2");
        }
        Commands::Config { action } => match action {
            ConfigAction::Show => {
                let json = serde_json::to_string_pretty(&config)?;
                println!("{json}");
            }
            ConfigAction::Get { key: _ } => {
                tracing::warn!("Config get not yet implemented");
            }
            ConfigAction::Set { key: _, value: _ } => {
                tracing::warn!("Config set not yet implemented");
            }
        },
        Commands::Channels { action: _ } => {
            tracing::warn!("Channels commands not yet implemented — coming in Phase 1");
        }
        Commands::Sessions { action: _ } => {
            tracing::warn!("Sessions commands not yet implemented — coming in Phase 1");
        }
        Commands::Cron { action: _ } => {
            tracing::warn!("Cron commands not yet implemented — coming in Phase 2");
        }
        Commands::Pairing { action: _ } => {
            tracing::warn!("Pairing commands not yet implemented — coming in Phase 1");
        }
    }

    Ok(())
}
