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

    // Initialize logging (deferred until config is loaded below)
    let verbose = cli.verbose;

    // Load config
    let config_path = cli
        .config
        .map(std::path::PathBuf::from)
        .unwrap_or_else(rusty_claw_core::config::Config::config_dir);

    let config = rusty_claw_core::config::Config::load(&config_path)?;

    // Initialize logging from config
    init_logging(&config, verbose);

    match cli.command {
        Commands::Gateway { port, ui } => {
            // Validate config
            let (warnings, errors) = config.validate();
            for w in &warnings {
                tracing::warn!("Config: {w}");
            }
            if !errors.is_empty() {
                for e in &errors {
                    tracing::error!("Config: {e}");
                }
                anyhow::bail!(
                    "Configuration has {} error(s), aborting. Fix the config and retry.",
                    errors.len()
                );
            }

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

            // Load WASM plugins from workspace/plugins/ directory
            #[cfg(feature = "wasm")]
            {
                let plugins_dir = config.workspace_dir().join("plugins");
                if plugins_dir.exists() {
                    match rusty_claw_plugins::wasm_runtime::WasmPluginLoader::new() {
                        Ok(loader) => {
                            if let Ok(entries) = std::fs::read_dir(&plugins_dir) {
                                for entry in entries.flatten() {
                                    let path = entry.path();
                                    if path.extension().is_some_and(|ext| ext == "wasm") {
                                        match plugin_manager.add_wasm_plugin(&path, &loader) {
                                            Ok(()) => tracing::info!(
                                                path = %path.display(),
                                                "Loaded WASM plugin"
                                            ),
                                            Err(e) => tracing::warn!(
                                                path = %path.display(),
                                                %e,
                                                "Failed to load WASM plugin"
                                            ),
                                        }
                                    }
                                }
                            }
                        }
                        Err(e) => tracing::warn!(%e, "Failed to create WASM plugin loader"),
                    }
                }
            }

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

            // Load skills from workspace
            let skills_dir = config
                .skills
                .as_ref()
                .and_then(|s| s.dir.as_ref())
                .map(std::path::PathBuf::from)
                .unwrap_or_else(|| config.workspace_dir().join("skills"));
            let skills = rusty_claw_gateway::skills::SkillRegistry::load_from_dir(&skills_dir);

            // Create pairing store
            let pairing = rusty_claw_core::pairing::PairingStore::new(
                rusty_claw_core::pairing::PairingStore::default_path(),
            );

            // Create browser pool (if browser config exists)
            let browser = config
                .tools
                .as_ref()
                .and_then(|t| t.browser.as_ref())
                .map(|bc| Arc::new(rusty_claw_browser::BrowserPool::new(bc.clone())));

            // Create cron scheduler
            let cron_jobs = config
                .cron
                .as_ref()
                .and_then(|c| c.jobs.as_ref())
                .cloned()
                .unwrap_or_default();
            let cron = if !cron_jobs.is_empty() {
                Some(Arc::new(rusty_claw_gateway::CronScheduler::new(cron_jobs)))
            } else {
                None
            };

            // Wrap config in Arc<RwLock> for runtime mutability
            let config_rw = Arc::new(tokio::sync::RwLock::new(config));

            // Build gateway state
            let state = Arc::new(rusty_claw_gateway::GatewayState::new(
                config_rw,
                Some(config_path.clone()),
                sessions,
                channels.clone(),
                tools,
                providers,
                hooks,
                skills,
                pairing,
                browser,
                cron.clone(),
            ));

            // Start cron scheduler if configured
            if let Some(scheduler) = cron {
                scheduler.start(state.clone());
            }

            // Start channel routers
            start_channels(&state, &channels).await;

            // Start gateway
            rusty_claw_gateway::start_gateway(state, port, ui).await?;
        }

        Commands::Agent {
            message,
            model,
            thinking: _,
        } => {
            // Create provider registry and get default
            let registry = create_provider_registry(&config)?;
            let config = Arc::new(config);
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
            if let Some(ref model) = model {
                session.meta.model = Some(model.clone());
            }

            // Create empty hook registry for CLI agent mode
            let hooks = Arc::new(rusty_claw_plugins::HookRegistry::new());

            if let Some(text) = message {
                // One-shot mode
                tracing::info!("Running agent one-shot");
                run_agent_turn(
                    &mut session, &text, &config, &tools, provider, credentials, &hooks,
                )
                .await?;
            } else {
                // Interactive REPL mode
                println!("Rusty Claw v{} — Interactive Agent", env!("CARGO_PKG_VERSION"));
                println!("Model: {}", session.meta.model.as_deref().unwrap_or(&config.default_model()));
                println!("Type /quit to exit, /reset to clear history, /model <name> to switch.\n");

                let stdin = tokio::io::BufReader::new(tokio::io::stdin());
                let mut lines = tokio::io::AsyncBufReadExt::lines(stdin);

                loop {
                    // Print prompt
                    eprint!("you> ");
                    use std::io::Write;
                    let _ = std::io::stderr().flush();

                    let line = match lines.next_line().await? {
                        Some(l) => l,
                        None => break, // EOF
                    };

                    let trimmed = line.trim();
                    if trimmed.is_empty() {
                        continue;
                    }

                    match trimmed {
                        "/quit" | "/exit" | "/q" => break,
                        "/reset" => {
                            session.transcript.clear();
                            println!("[Session reset]");
                            continue;
                        }
                        cmd if cmd.starts_with("/model ") => {
                            let new_model = cmd.strip_prefix("/model ").unwrap().trim();
                            session.meta.model = Some(new_model.to_string());
                            println!("[Model switched to: {new_model}]");
                            continue;
                        }
                        "/help" => {
                            println!("  /quit    — Exit the REPL");
                            println!("  /reset   — Clear conversation history");
                            println!("  /model X — Switch to model X");
                            continue;
                        }
                        _ => {}
                    }

                    if let Err(e) = run_agent_turn(
                        &mut session, trimmed, &config, &tools, provider, credentials, &hooks,
                    )
                    .await
                    {
                        eprintln!("[Agent error: {e}]");
                    }
                }
            }
        }

        Commands::Onboard { install_daemon: _ } => {
            run_onboard_wizard(&config_path).await?;
        }

        Commands::Status => {
            println!("Rusty Claw v{}", env!("CARGO_PKG_VERSION"));
            println!("Config: {}", config_path.display());
            println!("Workspace: {}", config.workspace_dir().display());
            println!("Gateway port: {}", config.gateway_port());
            println!("Default model: {}", config.default_model());

            // Binary size
            if let Ok(exe) = std::env::current_exe() {
                if let Ok(meta) = std::fs::metadata(&exe) {
                    let size_mb = meta.len() as f64 / (1024.0 * 1024.0);
                    println!("Binary size: {size_mb:.1} MB");
                }
            }

            // Configured channels
            let channel_count = [
                config.channels.as_ref().and_then(|c| c.telegram.as_ref()).map(|_| "telegram"),
                config.channels.as_ref().and_then(|c| c.discord.as_ref()).map(|_| "discord"),
                config.channels.as_ref().and_then(|c| c.slack.as_ref()).map(|_| "slack"),
            ]
            .iter()
            .flatten()
            .count();
            println!("Channels configured: {channel_count} (+webchat)");

            // Provider count
            let provider_count = config
                .models
                .as_ref()
                .and_then(|m| m.providers.as_ref())
                .map(|p| p.len())
                .unwrap_or(0);
            println!("Providers configured: {provider_count}");

            // Skills count
            let skills_dir = config
                .skills
                .as_ref()
                .and_then(|s| s.dir.as_ref())
                .map(std::path::PathBuf::from)
                .unwrap_or_else(|| config.workspace_dir().join("skills"));
            let skill_count = if skills_dir.exists() {
                std::fs::read_dir(&skills_dir)
                    .map(|rd| rd.filter_map(|e| e.ok()).filter(|e| {
                        e.path().extension().is_some_and(|ext| ext == "yaml" || ext == "yml")
                    }).count())
                    .unwrap_or(0)
            } else {
                0
            };
            println!("Skills loaded: {skill_count}");

            // Try to check if gateway is running
            let url = format!(
                "http://localhost:{}/health",
                config.gateway_port()
            );
            match reqwest::get(&url).await {
                Ok(resp) if resp.status().is_success() => {
                    println!("Gateway: running");
                    if let Ok(body) = resp.text().await {
                        if let Ok(health) = serde_json::from_str::<serde_json::Value>(&body) {
                            if let Some(conns) = health.get("connections").and_then(|v| v.as_u64()) {
                                println!("  Active connections: {conns}");
                            }
                        }
                    }
                }
                _ => {
                    println!("Gateway: not running");
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
                let json = serde_json::to_string_pretty(&config)?;
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

/// Run a single agent turn: send a message, stream the response, print it.
async fn run_agent_turn(
    session: &mut rusty_claw_core::session::Session,
    text: &str,
    config: &Arc<rusty_claw_core::config::Config>,
    tools: &rusty_claw_tools::ToolRegistry,
    provider: &dyn rusty_claw_providers::LlmProvider,
    credentials: &rusty_claw_providers::Credentials,
    hooks: &Arc<rusty_claw_plugins::HookRegistry>,
) -> anyhow::Result<()> {
    let inbound = rusty_claw_core::types::InboundMessage::from_cli_text(text);

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
                AgentEvent::BlockReply {
                    is_final: true, ..
                } => {
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
                    eprintln!("\n[tokens: {input_tokens} in / {output_tokens} out]");
                }
                AgentEvent::Error { message, .. } => {
                    eprintln!("\n[error: {message}]");
                }
                _ => {}
            }
        }
    });

    let result = rusty_claw_agent::run_agent(
        session, inbound, config, tools, provider, credentials, event_tx, hooks,
    )
    .await?;

    let _ = printer.await;

    if let Some(error) = &result.meta.error {
        eprintln!("Agent error: {}", error.message);
    }

    Ok(())
}

/// Interactive onboarding wizard for first-time setup.
async fn run_onboard_wizard(config_path: &std::path::Path) -> anyhow::Result<()> {
    use dialoguer::{Confirm, Input, Select};

    println!("\n  Rusty Claw v{} — Setup Wizard", env!("CARGO_PKG_VERSION"));
    println!("  ================================\n");

    // Check for existing config
    if config_path.exists() {
        let overwrite = Confirm::new()
            .with_prompt("A config file already exists. Overwrite?")
            .default(false)
            .interact()?;

        if !overwrite {
            println!("Keeping existing config. Run `rusty-claw doctor` to check your setup.");
            return Ok(());
        }
    }

    // 1. Choose primary provider
    let providers = &["Anthropic (Claude)", "OpenAI (GPT)", "Google (Gemini)", "Ollama (local)"];
    let provider_idx = Select::new()
        .with_prompt("Choose your primary AI provider")
        .items(providers)
        .default(0)
        .interact()?;

    let (provider_id, env_var_name, default_model) = match provider_idx {
        0 => ("anthropic", "ANTHROPIC_API_KEY", "claude-sonnet-4-5-20250929"),
        1 => ("openai", "OPENAI_API_KEY", "gpt-4o"),
        2 => ("google", "GOOGLE_AI_API_KEY", "gemini-2.0-flash"),
        3 => ("ollama", "", "llama3.2"),
        _ => unreachable!(),
    };

    // 2. Get API key (skip for Ollama)
    let api_key = if !env_var_name.is_empty() {
        let existing = std::env::var(env_var_name).unwrap_or_default();
        if !existing.is_empty() {
            println!("Found {env_var_name} in environment.");
            existing
        } else {
            Input::<String>::new()
                .with_prompt(format!("Enter your {provider_id} API key"))
                .interact_text()?
        }
    } else {
        String::new()
    };

    // 3. Choose workspace directory
    let default_workspace = std::env::var("HOME")
        .map(std::path::PathBuf::from)
        .unwrap_or_else(|_| std::path::PathBuf::from("."))
        .join("rusty-claw");
    let workspace: String = Input::new()
        .with_prompt("Workspace directory")
        .default(default_workspace.display().to_string())
        .interact_text()?;

    // 4. Choose gateway port
    let port: u16 = Input::new()
        .with_prompt("Gateway port")
        .default(18789)
        .interact_text()?;

    // 5. Build config JSON
    let mut config = serde_json::json!({
        "workspace": workspace,
        "gateway": {
            "port": port,
        },
        "models": {
            "default_model": default_model,
            "providers": [{
                "id": provider_id,
            }],
        },
    });

    // Add API key to provider config
    if !api_key.is_empty() {
        if let Some(providers) = config["models"]["providers"].as_array_mut() {
            if let Some(p) = providers.first_mut() {
                p["api_key_env"] = serde_json::json!(env_var_name);
            }
        }
    }

    // 6. Write config
    if let Some(parent) = config_path.parent() {
        std::fs::create_dir_all(parent)?;
    }
    let config_str = serde_json::to_string_pretty(&config)?;
    std::fs::write(config_path, &config_str)?;
    println!("\nConfig written to: {}", config_path.display());

    // 7. Create workspace directory
    std::fs::create_dir_all(&workspace)?;
    println!("Workspace created: {workspace}");

    // 8. Set env var hint
    if !api_key.is_empty() && std::env::var(env_var_name).unwrap_or_default().is_empty() {
        println!("\nAdd to your shell profile:");
        println!("  export {env_var_name}=\"{api_key}\"");
    }

    println!("\nSetup complete! Next steps:");
    println!("  1. rusty-claw doctor     — Verify your configuration");
    println!("  2. rusty-claw agent      — Chat with the agent");
    println!("  3. rusty-claw gateway --ui — Start the gateway with Control UI");
    println!();

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

    // Register WhatsApp if configured
    if let Some(wa_config) = config
        .channels
        .as_ref()
        .and_then(|c| c.whatsapp.as_ref())
    {
        if let Some(token) = wa_config.resolve_access_token() {
            let phone_number_id = wa_config.phone_number_id.clone().unwrap_or_default();
            let channel = rusty_claw_channels::whatsapp::WhatsAppChannel::new(
                rusty_claw_channels::whatsapp::WhatsAppChannelConfig {
                    phone_number_id,
                    access_token: token,
                    verify_token: wa_config.verify_token.clone(),
                    app_secret: wa_config.resolve_app_secret(),
                    webhook_port: wa_config.webhook_port,
                },
            );
            registry.register(Box::new(channel));
            tracing::info!("WhatsApp channel registered");
        } else {
            tracing::warn!("WhatsApp configured but no access token found");
        }
    }

    // Register Signal if configured
    if let Some(sig_config) = config
        .channels
        .as_ref()
        .and_then(|c| c.signal.as_ref())
    {
        if let Some(phone) = sig_config.resolve_phone_number() {
            let channel = rusty_claw_channels::signal::SignalChannel::new(
                sig_config.api_url.clone(),
                phone,
                sig_config.poll_interval_ms,
            );
            registry.register(Box::new(channel));
            tracing::info!("Signal channel registered");
        } else {
            tracing::warn!("Signal configured but no phone number found");
        }
    }

    // Register Google Chat if configured
    if let Some(gc_config) = config
        .channels
        .as_ref()
        .and_then(|c| c.googlechat.as_ref())
    {
        let project_id = gc_config.project_id.clone().unwrap_or_default();
        let channel = rusty_claw_channels::googlechat::GoogleChatChannel::new(
            project_id,
            gc_config.service_account_json.clone(),
            gc_config.webhook_port,
        );
        registry.register(Box::new(channel));
        tracing::info!("Google Chat channel registered");
    }

    // Register MS Teams if configured
    if let Some(teams_config) = config
        .channels
        .as_ref()
        .and_then(|c| c.msteams.as_ref())
    {
        if let Some(password) = teams_config.resolve_app_password() {
            let app_id = teams_config.app_id.clone().unwrap_or_default();
            let channel = rusty_claw_channels::msteams::MsTeamsChannel::new(
                app_id,
                password,
                teams_config.webhook_port,
            );
            registry.register(Box::new(channel));
            tracing::info!("MS Teams channel registered");
        } else {
            tracing::warn!("MS Teams configured but no app password found");
        }
    }

    // Register Matrix if configured
    if let Some(mx_config) = config
        .channels
        .as_ref()
        .and_then(|c| c.matrix.as_ref())
    {
        let token = mx_config.resolve_access_token().or_else(|| {
            mx_config.resolve_password()
        });
        if let Some(token) = token {
            let homeserver = mx_config
                .homeserver_url
                .clone()
                .unwrap_or_else(|| "https://matrix.org".into());
            let user_id = mx_config.username.clone();
            let channel = rusty_claw_channels::matrix::MatrixChannel::new(
                homeserver, token, user_id,
            );
            registry.register(Box::new(channel));
            tracing::info!("Matrix channel registered");
        } else {
            tracing::warn!("Matrix configured but no access token or password found");
        }
    }

    // Register BlueBubbles (iMessage) if configured
    if let Some(bb_config) = config
        .channels
        .as_ref()
        .and_then(|c| c.bluebubbles.as_ref())
    {
        if let Some(password) = bb_config.resolve_password() {
            let channel = rusty_claw_channels::bluebubbles::BlueBubblesChannel::new(
                bb_config.api_url.clone(),
                password,
            );
            registry.register(Box::new(channel));
            tracing::info!("BlueBubbles (iMessage) channel registered");
        } else {
            tracing::warn!("BlueBubbles configured but no password found");
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

/// Initialize tracing subscriber from config and verbose flag.
fn init_logging(config: &rusty_claw_core::config::Config, verbose: bool) {
    let logging = config.logging.as_ref();
    let level = if verbose {
        "debug"
    } else {
        logging
            .and_then(|l| l.level.as_deref())
            .unwrap_or("info")
    };

    // Build filter: base level + per-crate overrides
    let mut filter_str = level.to_string();
    if let Some(log_cfg) = logging {
        for f in &log_cfg.filters {
            filter_str.push(',');
            filter_str.push_str(f);
        }
    }

    let env_filter = tracing_subscriber::EnvFilter::try_from_default_env()
        .unwrap_or_else(|_| tracing_subscriber::EnvFilter::new(&filter_str));

    let is_json = logging
        .map(|l| l.format == "json")
        .unwrap_or(false);

    if is_json {
        tracing_subscriber::fmt()
            .json()
            .with_env_filter(env_filter)
            .init();
    } else {
        tracing_subscriber::fmt()
            .with_env_filter(env_filter)
            .init();
    }
}

/// Start all registered channels and set up message routing.
async fn start_channels(
    state: &Arc<rusty_claw_gateway::GatewayState>,
    channels: &Arc<rusty_claw_channels::ChannelRegistry>,
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
