use std::sync::Arc;

use clap::{Parser, Subcommand};
use rusty_claw_core::session::SessionStore;

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

    let config = Arc::new(rusty_claw_core::config::Config::load(&config_path)?);

    match cli.command {
        Commands::Gateway { port, ui } => {
            let port = port.unwrap_or_else(|| config.gateway_port());
            tracing::info!("Starting Rusty Claw gateway on port {port}");
            if ui {
                tracing::info!("Control UI enabled");
            }

            // Create session store
            let sessions: Arc<dyn rusty_claw_core::session::SessionStore> = Arc::new(
                rusty_claw_core::session_store::JsonlSessionStore::new(
                    rusty_claw_core::session_store::JsonlSessionStore::default_path(),
                ),
            );

            // Create tool registry
            let mut tools = rusty_claw_tools::ToolRegistry::new();
            rusty_claw_tools::register_builtin_tools(&mut tools);

            // Initialize plugin system
            let mut plugin_manager = rusty_claw_plugins::PluginManager::new();
            // plugin_manager.add_plugin(Box::new(rusty_claw_plugins::logging_plugin::LoggingPlugin))?;
            let plugin_regs = plugin_manager.initialize().await?;
            for tool in plugin_regs.tools {
                tools.register(tool);
            }
            let hooks = plugin_manager.hooks();
            let tools = Arc::new(tools);

            // Create provider registry
            let providers = Arc::new(create_provider_registry(&config)?);

            // Create channel registry
            let channels = Arc::new(create_channel_registry(&config));

            // Build gateway state
            let state = Arc::new(rusty_claw_gateway::GatewayState::new(
                config.clone(),
                sessions,
                channels.clone(),
                tools,
                providers,
                hooks,
            ));

            // Start channel routers
            start_channels(&state, &channels, &config).await;

            // Start gateway
            rusty_claw_gateway::start_gateway(state, port, ui).await?;
        }

        Commands::Agent {
            message,
            model,
            thinking: _,
        } => {
            let text = match message {
                Some(text) => text,
                None => {
                    tracing::info!("Interactive mode not yet implemented. Use -m \"message\"");
                    return Ok(());
                }
            };

            tracing::info!("Running agent one-shot");

            // Create provider registry and get default
            let registry = create_provider_registry(&config)?;
            let (provider, credentials) = registry
                .default()
                .ok_or_else(|| anyhow::anyhow!("No default provider configured"))?;

            // Create tools
            let mut tools = rusty_claw_tools::ToolRegistry::new();
            rusty_claw_tools::register_builtin_tools(&mut tools);

            // Create ephemeral session
            let key = rusty_claw_core::session::SessionKey {
                channel: "cli".into(),
                account_id: "local".into(),
                chat_type: rusty_claw_core::types::ChatType::Dm,
                peer_id: "local-user".into(),
                scope: rusty_claw_core::session::SessionScope::PerSender,
            };
            let mut session = rusty_claw_core::session::Session::new(key);
            if let Some(model) = model {
                session.meta.model = Some(model);
            }

            // Set up event channel and printer
            let (event_tx, mut event_rx) =
                tokio::sync::mpsc::unbounded_channel::<rusty_claw_agent::AgentEvent>();

            // Spawn event printer
            let printer = tokio::spawn(async move {
                use rusty_claw_agent::AgentEvent;
                while let Some(event) = event_rx.recv().await {
                    match event {
                        AgentEvent::PartialReply { delta } => {
                            print!("{delta}");
                            use std::io::Write;
                            let _ = std::io::stdout().flush();
                        }
                        AgentEvent::BlockReply { is_final: true, .. } => {
                            println!();
                        }
                        AgentEvent::ToolCall { tool, .. } => {
                            eprintln!("\n[tool: {tool}]");
                        }
                        AgentEvent::ToolResult {
                            tool,
                            is_error,
                            content,
                            ..
                        } => {
                            let status = if is_error { "error" } else { "ok" };
                            // Show truncated result
                            let preview = if content.len() > 200 {
                                format!("{}...", &content[..200])
                            } else {
                                content
                            };
                            eprintln!("[{tool} {status}]: {preview}");
                        }
                        AgentEvent::Usage {
                            input_tokens,
                            output_tokens,
                        } => {
                            eprintln!(
                                "\n[tokens: {input_tokens} in / {output_tokens} out]"
                            );
                        }
                        AgentEvent::Error { message, .. } => {
                            eprintln!("\n[error: {message}]");
                        }
                        _ => {}
                    }
                }
            });

            // Create inbound message
            let inbound = rusty_claw_core::types::InboundMessage::from_cli_text(&text);

            // Create empty hook registry for CLI agent mode
            let hooks = Arc::new(rusty_claw_plugins::HookRegistry::new());

            // Run agent
            let result = rusty_claw_agent::run_agent(
                &mut session,
                inbound,
                &config,
                &tools,
                provider,
                credentials,
                event_tx,
                &hooks,
            )
            .await?;

            // Wait for printer
            let _ = printer.await;

            if let Some(error) = &result.meta.error {
                eprintln!("Agent error: {}", error.message);
                std::process::exit(1);
            }
        }

        Commands::Onboard { install_daemon: _ } => {
            tracing::info!("Starting onboarding wizard");
            tracing::warn!("Onboard not yet implemented — coming in Phase 1");
        }

        Commands::Status => {
            println!("Rusty Claw v{}", env!("CARGO_PKG_VERSION"));
            println!("Config: {}", config_path.display());
            println!("Workspace: {}", config.workspace_dir().display());
            println!("Gateway port: {}", config.gateway_port());

            // Try to check if gateway is running
            let url = format!(
                "http://localhost:{}/health",
                config.gateway_port()
            );
            match reqwest::get(&url).await {
                Ok(resp) if resp.status().is_success() => {
                    println!("Status: running");
                    if let Ok(body) = resp.text().await {
                        println!("Health: {body}");
                    }
                }
                _ => {
                    println!("Status: not running");
                }
            }
        }

        Commands::Doctor => {
            println!("Rusty Claw Doctor — checking system health\n");

            // Check config file
            if config_path.exists() {
                println!("  [ok] Config file: {}", config_path.display());
            } else {
                println!("  [!!] Config file not found: {}", config_path.display());
            }

            // Check workspace directory
            let workspace = config.workspace_dir();
            if workspace.exists() {
                println!("  [ok] Workspace: {}", workspace.display());
            } else {
                println!("  [--] Workspace not created yet: {}", workspace.display());
            }

            // Check providers
            let registry = create_provider_registry(&config);
            match registry {
                Ok(reg) => {
                    for id in reg.list_ids() {
                        if let Some((_provider, creds)) = reg.get(id) {
                            let has_key = match creds {
                                rusty_claw_providers::Credentials::ApiKey { api_key } => {
                                    !api_key.is_empty()
                                }
                                _ => true,
                            };
                            if has_key {
                                println!("  [ok] Provider '{id}': API key present");
                            } else {
                                println!("  [!!] Provider '{id}': no API key");
                            }
                        }
                    }
                }
                Err(e) => println!("  [!!] Provider setup error: {e}"),
            }

            // Check gateway connectivity
            let url = format!("http://localhost:{}/health", config.gateway_port());
            match reqwest::get(&url).await {
                Ok(resp) if resp.status().is_success() => {
                    println!("  [ok] Gateway: running on port {}", config.gateway_port());
                }
                _ => {
                    println!("  [--] Gateway: not running");
                }
            }

            println!("\nDone.");
        }
        Commands::Migrate { from } => {
            println!("Migrating from OpenClaw config: {from}");
            let path = std::path::Path::new(&from);
            if !path.exists() {
                eprintln!("Error: File not found: {from}");
                std::process::exit(1);
            }

            // Try to load as OpenClaw config (JSON5 format, same structure)
            match rusty_claw_core::config::Config::load(path) {
                Ok(migrated) => {
                    let output = serde_json::to_string_pretty(&migrated)?;
                    println!("Migrated config:\n{output}");
                    println!("\nTo apply, save this to: {}", config_path.display());
                }
                Err(e) => {
                    eprintln!("Error parsing config: {e}");
                    std::process::exit(1);
                }
            }
        }
        Commands::Config { action } => match action {
            ConfigAction::Show => {
                let json = serde_json::to_string_pretty(config.as_ref())?;
                println!("{json}");
            }
            ConfigAction::Get { key } => {
                match config.get_path(&key) {
                    Some(value) => {
                        let formatted = serde_json::to_string_pretty(&value)?;
                        println!("{formatted}");
                    }
                    None => {
                        eprintln!("Config path not found: {key}");
                        std::process::exit(1);
                    }
                }
            }
            ConfigAction::Set { key: _, value: _ } => {
                eprintln!("Config set via CLI not yet supported. Edit the config file directly.");
            }
        },
        Commands::Channels { action } => {
            let channels = create_channel_registry(&config);
            match action {
                ChannelAction::Status => {
                    let ids = channels.list();
                    if ids.is_empty() {
                        println!("No channels registered.");
                    } else {
                        println!("Registered channels:");
                        for id in &ids {
                            if let Some(ch) = channels.get(id) {
                                let status = ch.status().await;
                                let state_str = if status.connected { "connected" } else { "disconnected" };
                                println!("  {} — {} ({})", id, ch.meta().label, state_str);
                            }
                        }
                    }
                }
                ChannelAction::Login { channel: _ } => {
                    println!("Channel login is handled automatically on gateway start.");
                }
                ChannelAction::Logout { channel: _ } => {
                    println!("Channel logout requires a running gateway. Use the gateway WS API.");
                }
            }
        }
        Commands::Sessions { action } => {
            let store = rusty_claw_core::session_store::JsonlSessionStore::new(
                rusty_claw_core::session_store::JsonlSessionStore::default_path(),
            );
            match action {
                SessionAction::List => {
                    match store.list().await {
                        Ok(sessions) => {
                            if sessions.is_empty() {
                                println!("No sessions found.");
                            } else {
                                println!("Sessions ({}):", sessions.len());
                                for s in &sessions {
                                    println!(
                                        "  {} | {} | {} | {}",
                                        s.key.hash_key(),
                                        s.key.channel,
                                        s.key.peer_id,
                                        s.last_updated_at.format("%Y-%m-%d %H:%M")
                                    );
                                }
                            }
                        }
                        Err(e) => eprintln!("Error listing sessions: {e}"),
                    }
                }
                SessionAction::Reset { session } => {
                    use rusty_claw_core::session::SessionStore;
                    let sessions = store.list().await?;
                    let target = session.as_deref().unwrap_or("");

                    let found = sessions.iter().find(|s| {
                        s.key.hash_key() == target || target.is_empty()
                    });

                    match found {
                        Some(s) => {
                            store.reset(&s.key).await?;
                            println!("Session {} reset.", s.key.hash_key());
                        }
                        None if target.is_empty() => {
                            println!("No sessions to reset.");
                        }
                        None => {
                            eprintln!("Session not found: {target}");
                        }
                    }
                }
                SessionAction::Delete { session } => {
                    use rusty_claw_core::session::SessionStore;
                    let sessions = store.list().await?;
                    let found = sessions.iter().find(|s| s.key.hash_key() == session);

                    match found {
                        Some(s) => {
                            store.delete(&s.key).await?;
                            println!("Session {session} deleted.");
                        }
                        None => {
                            eprintln!("Session not found: {session}");
                        }
                    }
                }
            }
        }
        Commands::Cron { action } => match action {
            CronAction::List => {
                let jobs = config
                    .cron
                    .as_ref()
                    .and_then(|c| c.jobs.as_ref())
                    .cloned()
                    .unwrap_or_default();

                if jobs.is_empty() {
                    println!("No cron jobs configured.");
                } else {
                    println!("Cron jobs ({}):", jobs.len());
                    for job in &jobs {
                        let status = if job.enabled { "enabled" } else { "disabled" };
                        println!("  {} | {} | {} | {}", job.id, job.schedule, job.task, status);
                    }
                }
            }
            CronAction::Add { schedule, task } => {
                println!("To add a cron job, add it to the config file:");
                println!("  cron.jobs: [{{ id: \"my-job\", schedule: \"{schedule}\", task: \"{task}\", enabled: true }}]");
                println!("\nConfig file: {}", config_path.display());
            }
            CronAction::Remove { id } => {
                println!("To remove cron job '{id}', edit the config file:");
                println!("  {}", config_path.display());
            }
        },
        Commands::Pairing { action } => {
            let store = rusty_claw_core::pairing::PairingStore::new(
                rusty_claw_core::pairing::PairingStore::default_path(),
            );
            match action {
                PairingAction::Approve { channel, code } => {
                    match store.approve(&channel, &code)? {
                        true => println!("Pairing approved for {channel} with code {code}"),
                        false => println!("No pending pairing found for {channel} with code {code}"),
                    }
                }
                PairingAction::Reject { channel, code } => {
                    match store.reject(&channel, &code)? {
                        true => println!("Pairing rejected for {channel} with code {code}"),
                        false => println!("No pending pairing found for {channel} with code {code}"),
                    }
                }
                PairingAction::List => {
                    let pending = store.list_pending();
                    if pending.is_empty() {
                        println!("No pending pairing requests.");
                    } else {
                        println!("Pending pairing requests:");
                        for req in &pending {
                            println!(
                                "  {} | {} | code: {} | {}",
                                req.channel,
                                req.sender_id,
                                req.code,
                                req.display_name.as_deref().unwrap_or("unknown"),
                            );
                        }
                    }
                }
            }
        }
    }

    Ok(())
}

/// Create a provider registry from config, registering all configured providers.
fn create_provider_registry(
    config: &rusty_claw_core::config::Config,
) -> anyhow::Result<rusty_claw_providers::ProviderRegistry> {
    let provider_configs = config
        .models
        .as_ref()
        .and_then(|m| m.providers.as_ref())
        .cloned()
        .unwrap_or_default();

    // Determine the default provider ID
    let default_id = provider_configs
        .first()
        .map(|p| p.id.clone())
        .unwrap_or_else(|| "anthropic".into());

    let mut registry = rusty_claw_providers::ProviderRegistry::new(default_id);

    if provider_configs.is_empty() {
        // Fallback: create a default Anthropic provider from env
        let api_key = std::env::var("ANTHROPIC_API_KEY").unwrap_or_default();
        if api_key.is_empty() {
            tracing::warn!(
                "No API key configured. Set ANTHROPIC_API_KEY or configure models.providers in config."
            );
        }
        let provider = Arc::new(rusty_claw_providers::anthropic::AnthropicProvider::new(None));
        let credentials = rusty_claw_providers::Credentials::ApiKey { api_key };
        registry.register("anthropic".into(), provider, credentials);
    } else {
        for pc in &provider_configs {
            let api_key = pc
                .resolve_api_key()
                .or_else(|| default_env_key_for_provider(&pc.id))
                .unwrap_or_default();

            if api_key.is_empty() {
                tracing::warn!(provider = %pc.id, "No API key found for provider");
            }

            let credentials = rusty_claw_providers::Credentials::ApiKey {
                api_key: api_key.clone(),
            };

            let provider: Arc<dyn rusty_claw_providers::LlmProvider> = match pc.id.as_str() {
                "anthropic" => Arc::new(
                    rusty_claw_providers::anthropic::AnthropicProvider::new(
                        pc.base_url.as_deref(),
                    ),
                ),
                "openai" => Arc::new(
                    rusty_claw_providers::openai::OpenAiProvider::openai(
                        pc.base_url.as_deref(),
                    ),
                ),
                "openrouter" => Arc::new(
                    rusty_claw_providers::openai::OpenAiProvider::openrouter(
                        pc.base_url.as_deref(),
                    ),
                ),
                "ollama" => Arc::new(
                    rusty_claw_providers::openai::OpenAiProvider::ollama(
                        pc.base_url.as_deref(),
                    ),
                ),
                "google" | "gemini" => Arc::new(
                    rusty_claw_providers::google::GeminiProvider::new(
                        pc.base_url.as_deref(),
                    ),
                ),
                other => {
                    tracing::warn!(provider = other, "Unknown provider type, skipping");
                    continue;
                }
            };

            registry.register(pc.id.clone(), provider, credentials);
        }
    }

    Ok(registry)
}

/// Try to find a default API key env var for well-known providers.
fn default_env_key_for_provider(provider_id: &str) -> Option<String> {
    let env_var = match provider_id {
        "anthropic" => "ANTHROPIC_API_KEY",
        "openai" => "OPENAI_API_KEY",
        "google" => "GOOGLE_AI_API_KEY",
        "openrouter" => "OPENROUTER_API_KEY",
        _ => return None,
    };
    std::env::var(env_var).ok().filter(|v| !v.is_empty())
}

/// Create channel registry from config.
fn create_channel_registry(
    config: &rusty_claw_core::config::Config,
) -> rusty_claw_channels::ChannelRegistry {
    let mut registry = rusty_claw_channels::ChannelRegistry::new();

    // Register Telegram if configured
    if let Some(tg_config) = config
        .channels
        .as_ref()
        .and_then(|c| c.telegram.as_ref())
    {
        if let Some(token) = tg_config.resolve_bot_token() {
            let channel = rusty_claw_channels::telegram::TelegramChannel::new(
                token,
                tg_config.allowed_users.clone(),
            );
            registry.register(Box::new(channel));
            tracing::info!("Telegram channel registered");
        } else {
            tracing::warn!("Telegram configured but no bot token found");
        }
    }

    // Register Discord if configured
    if let Some(dc_config) = config
        .channels
        .as_ref()
        .and_then(|c| c.discord.as_ref())
    {
        if let Some(token) = dc_config.resolve_bot_token() {
            let channel = rusty_claw_channels::discord::DiscordChannel::new(
                token,
                dc_config.allowed_guilds.clone(),
                dc_config.allowed_users.clone(),
            );
            registry.register(Box::new(channel));
            tracing::info!("Discord channel registered");
        } else {
            tracing::warn!("Discord configured but no bot token found");
        }
    }

    // Register Slack if configured
    if let Some(sl_config) = config
        .channels
        .as_ref()
        .and_then(|c| c.slack.as_ref())
    {
        if let Some(token) = sl_config.resolve_bot_token() {
            let channel = rusty_claw_channels::slack::SlackChannel::new(
                token,
                sl_config.resolve_signing_secret(),
                sl_config.port,
            );
            registry.register(Box::new(channel));
            tracing::info!("Slack channel registered");
        } else {
            tracing::warn!("Slack configured but no bot token found");
        }
    }

    // Register WebChat (always available)
    {
        let (webchat, _inbound_tx) = rusty_claw_channels::webchat::WebChatChannel::new();
        registry.register(Box::new(webchat));
        tracing::info!("WebChat channel registered");
    }

    registry
}

/// Start all registered channels and set up message routing.
async fn start_channels(
    state: &Arc<rusty_claw_gateway::GatewayState>,
    channels: &Arc<rusty_claw_channels::ChannelRegistry>,
    _config: &Arc<rusty_claw_core::config::Config>,
) {
    for channel_id in channels.list() {
        if let Some(channel) = channels.get(channel_id) {
            let config_value = serde_json::json!({}); // Channel-specific config as needed
            match channel.start(&config_value).await {
                Ok((rx, _handle)) => {
                    tracing::info!(channel = channel_id, "Channel started");
                    rusty_claw_gateway::channel_router::start_channel_router(
                        state.clone(),
                        channel_id.to_string(),
                        rx,
                    );
                }
                Err(e) => {
                    tracing::error!(channel = channel_id, %e, "Failed to start channel");
                }
            }
        }
    }
}
