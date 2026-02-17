//! Configuration loading, validation, and hot-reload.

use std::path::{Path, PathBuf};

use serde::{Deserialize, Serialize};

/// Top-level Rusty Claw configuration.
///
/// Compatible with OpenClaw's `openclaw.json` structure for migration.
#[derive(Debug, Clone, Default, Serialize, Deserialize)]
pub struct Config {
    #[serde(skip_serializing_if = "Option::is_none")]
    pub agents: Option<AgentsConfig>,

    #[serde(skip_serializing_if = "Option::is_none")]
    pub models: Option<ModelsConfig>,

    #[serde(skip_serializing_if = "Option::is_none")]
    pub channels: Option<ChannelsConfig>,

    #[serde(skip_serializing_if = "Option::is_none")]
    pub tools: Option<ToolsConfig>,

    #[serde(skip_serializing_if = "Option::is_none")]
    pub gateway: Option<GatewayConfig>,

    #[serde(skip_serializing_if = "Option::is_none")]
    pub session: Option<SessionConfig>,

    #[serde(skip_serializing_if = "Option::is_none")]
    pub cron: Option<CronConfig>,

    #[serde(skip_serializing_if = "Option::is_none")]
    pub logging: Option<LoggingConfig>,

    #[serde(skip_serializing_if = "Option::is_none")]
    pub plugins: Option<PluginsConfig>,

    #[serde(skip_serializing_if = "Option::is_none")]
    pub skills: Option<SkillsConfig>,

    #[serde(skip_serializing_if = "Option::is_none")]
    pub memory: Option<MemoryConfig>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct AgentsConfig {
    #[serde(skip_serializing_if = "Option::is_none")]
    pub defaults: Option<AgentDefaults>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct AgentDefaults {
    #[serde(skip_serializing_if = "Option::is_none")]
    pub workspace: Option<String>,

    #[serde(skip_serializing_if = "Option::is_none")]
    pub model: Option<String>,

    #[serde(skip_serializing_if = "Option::is_none")]
    pub max_tokens: Option<u32>,

    #[serde(skip_serializing_if = "Option::is_none")]
    pub temperature: Option<f64>,

    #[serde(skip_serializing_if = "Option::is_none")]
    pub max_tool_iterations: Option<u32>,

    #[serde(skip_serializing_if = "Option::is_none")]
    pub sandbox: Option<SandboxConfig>,

    /// Default thinking budget tokens override. If set, takes priority over ThinkingLevel.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub thinking_budget_tokens: Option<u32>,

    /// Maximum spawn depth for multi-agent spawning (default: 3).
    #[serde(skip_serializing_if = "Option::is_none")]
    pub max_spawn_depth: Option<u32>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct SandboxConfig {
    #[serde(default)]
    pub mode: SandboxMode,

    #[serde(default = "default_true")]
    pub restrict_to_workspace: bool,
}

fn default_true() -> bool {
    true
}

#[derive(Debug, Clone, Copy, Default, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum SandboxMode {
    Off,
    #[default]
    NonMain,
    All,
}

// --- Typed provider config ---

#[derive(Debug, Clone, Default, Serialize, Deserialize)]
pub struct ModelsConfig {
    #[serde(skip_serializing_if = "Option::is_none")]
    pub providers: Option<Vec<ProviderConfig>>,
}

/// Configuration for a single LLM provider.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ProviderConfig {
    pub id: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub api_key_env: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub api_key: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub base_url: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub default_model: Option<String>,
}

impl ProviderConfig {
    /// Resolve the API key: check `api_key` field first, then `api_key_env` environment variable.
    pub fn resolve_api_key(&self) -> Option<String> {
        resolve_secret_field(&self.api_key, &self.api_key_env)
    }
}

// --- Typed channel configs ---

#[derive(Debug, Clone, Default, Serialize, Deserialize)]
pub struct ChannelsConfig {
    #[serde(skip_serializing_if = "Option::is_none")]
    pub telegram: Option<TelegramConfig>,

    #[serde(skip_serializing_if = "Option::is_none")]
    pub discord: Option<DiscordConfig>,

    #[serde(skip_serializing_if = "Option::is_none")]
    pub slack: Option<SlackConfig>,

    #[serde(skip_serializing_if = "Option::is_none")]
    pub whatsapp: Option<WhatsAppConfig>,

    #[serde(skip_serializing_if = "Option::is_none")]
    pub signal: Option<SignalConfig>,

    #[serde(skip_serializing_if = "Option::is_none")]
    pub googlechat: Option<GoogleChatConfig>,

    #[serde(skip_serializing_if = "Option::is_none")]
    pub msteams: Option<MsTeamsConfig>,

    #[serde(skip_serializing_if = "Option::is_none")]
    pub matrix: Option<MatrixConfig>,

    #[serde(skip_serializing_if = "Option::is_none")]
    pub bluebubbles: Option<BlueBubblesConfig>,
}

/// Discord channel configuration.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct DiscordConfig {
    #[serde(skip_serializing_if = "Option::is_none")]
    pub bot_token: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub bot_token_env: Option<String>,
    #[serde(default)]
    pub allowed_guilds: Vec<String>,
    #[serde(default)]
    pub allowed_users: Vec<String>,
}

impl DiscordConfig {
    pub fn resolve_bot_token(&self) -> Option<String> {
        resolve_secret_field(&self.bot_token, &self.bot_token_env)
    }
}

/// Slack channel configuration.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct SlackConfig {
    #[serde(skip_serializing_if = "Option::is_none")]
    pub bot_token: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub bot_token_env: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub signing_secret: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub signing_secret_env: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub app_token: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub port: Option<u16>,
}

impl SlackConfig {
    pub fn resolve_bot_token(&self) -> Option<String> {
        resolve_secret_field(&self.bot_token, &self.bot_token_env)
    }

    pub fn resolve_signing_secret(&self) -> Option<String> {
        resolve_secret_field(&self.signing_secret, &self.signing_secret_env)
    }
}

/// Telegram channel configuration.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct TelegramConfig {
    #[serde(skip_serializing_if = "Option::is_none")]
    pub bot_token: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub bot_token_env: Option<String>,
    /// Optional list of allowed user IDs. Empty = allow all.
    #[serde(default)]
    pub allowed_users: Vec<String>,
}

impl TelegramConfig {
    /// Resolve the bot token: check `bot_token` first, then `bot_token_env` environment variable.
    pub fn resolve_bot_token(&self) -> Option<String> {
        resolve_secret_field(&self.bot_token, &self.bot_token_env)
    }
}

/// WhatsApp Business Cloud API configuration.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct WhatsAppConfig {
    #[serde(skip_serializing_if = "Option::is_none")]
    pub phone_number_id: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub access_token: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub access_token_env: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub verify_token: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub app_secret: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub app_secret_env: Option<String>,
    #[serde(default = "default_whatsapp_port")]
    pub webhook_port: u16,
}

fn default_whatsapp_port() -> u16 {
    3101
}

impl WhatsAppConfig {
    pub fn resolve_access_token(&self) -> Option<String> {
        resolve_secret_field(&self.access_token, &self.access_token_env)
    }
    pub fn resolve_app_secret(&self) -> Option<String> {
        resolve_secret_field(&self.app_secret, &self.app_secret_env)
    }
}

/// Signal channel configuration (signal-cli REST bridge).
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct SignalConfig {
    /// signal-cli REST API URL (default: http://localhost:8080).
    #[serde(default = "default_signal_api_url")]
    pub api_url: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub phone_number: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub phone_number_env: Option<String>,
    /// Poll interval in milliseconds (default: 2000).
    #[serde(default = "default_signal_poll_interval")]
    pub poll_interval_ms: u64,
}

fn default_signal_api_url() -> String {
    "http://localhost:8080".into()
}

fn default_signal_poll_interval() -> u64 {
    2000
}

impl SignalConfig {
    pub fn resolve_phone_number(&self) -> Option<String> {
        resolve_secret_field(&self.phone_number, &self.phone_number_env)
    }
}

/// Google Chat (Workspace) channel configuration.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct GoogleChatConfig {
    #[serde(skip_serializing_if = "Option::is_none")]
    pub service_account_json: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub project_id: Option<String>,
    #[serde(default = "default_googlechat_port")]
    pub webhook_port: u16,
}

fn default_googlechat_port() -> u16 {
    3102
}

/// Microsoft Teams (Bot Framework) channel configuration.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct MsTeamsConfig {
    #[serde(skip_serializing_if = "Option::is_none")]
    pub app_id: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub app_password: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub app_password_env: Option<String>,
    #[serde(default = "default_msteams_port")]
    pub webhook_port: u16,
}

fn default_msteams_port() -> u16 {
    3103
}

impl MsTeamsConfig {
    pub fn resolve_app_password(&self) -> Option<String> {
        resolve_secret_field(&self.app_password, &self.app_password_env)
    }
}

/// Matrix channel configuration.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct MatrixConfig {
    #[serde(skip_serializing_if = "Option::is_none")]
    pub homeserver_url: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub username: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub password: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub password_env: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub access_token: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub access_token_env: Option<String>,
}

impl MatrixConfig {
    pub fn resolve_password(&self) -> Option<String> {
        resolve_secret_field(&self.password, &self.password_env)
    }
    pub fn resolve_access_token(&self) -> Option<String> {
        resolve_secret_field(&self.access_token, &self.access_token_env)
    }
}

/// iMessage (BlueBubbles) channel configuration.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct BlueBubblesConfig {
    /// BlueBubbles API URL (default: http://localhost:1234).
    #[serde(default = "default_bluebubbles_api_url")]
    pub api_url: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub password: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub password_env: Option<String>,
}

fn default_bluebubbles_api_url() -> String {
    "http://localhost:1234".into()
}

impl BlueBubblesConfig {
    pub fn resolve_password(&self) -> Option<String> {
        resolve_secret_field(&self.password, &self.password_env)
    }
}

// --- Other configs (unchanged) ---

#[derive(Debug, Clone, Default, Serialize, Deserialize)]
pub struct ToolsConfig {
    #[serde(skip_serializing_if = "Option::is_none")]
    pub allow: Option<Vec<String>>,

    #[serde(skip_serializing_if = "Option::is_none")]
    pub deny: Option<Vec<String>>,

    /// URL for the web search API (SearXNG, Brave, etc.)
    #[serde(skip_serializing_if = "Option::is_none")]
    pub search_api_url: Option<String>,

    /// API key for the web search API (e.g. Brave Search).
    #[serde(skip_serializing_if = "Option::is_none")]
    pub search_api_key: Option<String>,

    /// Text-to-speech configuration.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub tts: Option<TtsConfig>,

    /// Image generation configuration.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub image_generation: Option<ImageGenerationConfig>,

    /// Voice transcription configuration.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub transcription: Option<TranscriptionConfig>,

    /// Exec tool configuration.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub exec: Option<ExecConfig>,

    /// Browser automation configuration.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub browser: Option<BrowserConfig>,
}

/// Text-to-speech (TTS) configuration.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct TtsConfig {
    /// TTS provider (default: "elevenlabs").
    #[serde(default = "default_tts_provider")]
    pub provider: String,

    #[serde(skip_serializing_if = "Option::is_none")]
    pub api_key: Option<String>,

    #[serde(skip_serializing_if = "Option::is_none")]
    pub api_key_env: Option<String>,

    /// Default voice ID.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub default_voice: Option<String>,

    /// Default model ID (e.g. "eleven_monolingual_v1").
    #[serde(skip_serializing_if = "Option::is_none")]
    pub default_model: Option<String>,

    /// Output format (default: "mp3_44100_128").
    #[serde(skip_serializing_if = "Option::is_none")]
    pub output_format: Option<String>,
}

fn default_tts_provider() -> String {
    "elevenlabs".into()
}

impl TtsConfig {
    pub fn resolve_api_key(&self) -> Option<String> {
        resolve_secret_field(&self.api_key, &self.api_key_env)
    }
}

/// Image generation configuration.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ImageGenerationConfig {
    /// Provider: "openai" or "stability" (default: "openai").
    #[serde(default = "default_imagegen_provider")]
    pub provider: String,

    #[serde(skip_serializing_if = "Option::is_none")]
    pub api_key: Option<String>,

    #[serde(skip_serializing_if = "Option::is_none")]
    pub api_key_env: Option<String>,

    /// Default model (e.g. "dall-e-3", "stable-diffusion-xl-1024-v1-0").
    #[serde(skip_serializing_if = "Option::is_none")]
    pub default_model: Option<String>,

    /// Default image size (e.g. "1024x1024").
    #[serde(skip_serializing_if = "Option::is_none")]
    pub default_size: Option<String>,

    /// Default quality (e.g. "standard", "hd").
    #[serde(skip_serializing_if = "Option::is_none")]
    pub default_quality: Option<String>,
}

fn default_imagegen_provider() -> String {
    "openai".into()
}

impl ImageGenerationConfig {
    pub fn resolve_api_key(&self) -> Option<String> {
        resolve_secret_field(&self.api_key, &self.api_key_env)
    }
}

/// Voice transcription (speech-to-text) configuration.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct TranscriptionConfig {
    /// Provider: "groq" or "openai" (default: "groq").
    #[serde(default = "default_transcription_provider")]
    pub provider: String,

    #[serde(skip_serializing_if = "Option::is_none")]
    pub api_key: Option<String>,

    #[serde(skip_serializing_if = "Option::is_none")]
    pub api_key_env: Option<String>,

    /// Model name (e.g. "whisper-large-v3-turbo").
    #[serde(skip_serializing_if = "Option::is_none")]
    pub model: Option<String>,
}

fn default_transcription_provider() -> String {
    "groq".into()
}

impl TranscriptionConfig {
    pub fn resolve_api_key(&self) -> Option<String> {
        resolve_secret_field(&self.api_key, &self.api_key_env)
    }
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct GatewayConfig {
    #[serde(default = "default_port")]
    pub port: u16,

    #[serde(skip_serializing_if = "Option::is_none")]
    pub bind: Option<String>,

    #[serde(skip_serializing_if = "Option::is_none")]
    pub auth: Option<GatewayAuthConfig>,

    #[serde(skip_serializing_if = "Option::is_none")]
    pub tls: Option<TlsConfig>,

    #[serde(skip_serializing_if = "Option::is_none")]
    pub rate_limit: Option<RateLimitConfig>,

    #[serde(skip_serializing_if = "Option::is_none")]
    pub tailscale: Option<TailscaleConfig>,
}

fn default_port() -> u16 {
    18789
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct GatewayAuthConfig {
    /// Auth mode: "none", "token", or "password". Default: "none".
    #[serde(skip_serializing_if = "Option::is_none")]
    pub mode: Option<String>,

    #[serde(skip_serializing_if = "Option::is_none")]
    pub token: Option<String>,

    #[serde(skip_serializing_if = "Option::is_none")]
    pub token_env: Option<String>,

    #[serde(skip_serializing_if = "Option::is_none")]
    pub password: Option<String>,

    #[serde(skip_serializing_if = "Option::is_none")]
    pub password_env: Option<String>,
}

impl GatewayAuthConfig {
    /// Resolve the auth token from direct value or env var.
    pub fn resolve_token(&self) -> Option<String> {
        resolve_secret_field(&self.token, &self.token_env)
    }

    /// Resolve the auth password from direct value or env var.
    pub fn resolve_password(&self) -> Option<String> {
        resolve_secret_field(&self.password, &self.password_env)
    }

    /// Get the effective auth mode.
    pub fn effective_mode(&self) -> &str {
        self.mode.as_deref().unwrap_or("none")
    }
}

/// TLS configuration for the gateway.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct TlsConfig {
    /// Path to the TLS certificate file (PEM).
    pub cert_path: String,
    /// Path to the TLS private key file (PEM).
    pub key_path: String,
}

/// Rate limiting configuration.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct RateLimitConfig {
    /// Max WebSocket connections per IP (default: 10).
    #[serde(default = "default_max_connections_per_ip")]
    pub max_connections_per_ip: u32,
}

fn default_max_connections_per_ip() -> u32 {
    10
}

/// Exec tool configuration.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ExecConfig {
    /// Execution mode: "blocklist" (default) or "allowlist".
    #[serde(default = "default_exec_mode")]
    pub mode: String,

    /// Allowed command prefixes (only used in allowlist mode).
    #[serde(default)]
    pub allowed_commands: Vec<String>,

    /// Docker image for sandboxed execution (optional).
    #[serde(skip_serializing_if = "Option::is_none")]
    pub docker_image: Option<String>,

    /// Maximum output size in bytes (default: 100KB).
    #[serde(default = "default_max_output_bytes")]
    pub max_output_bytes: usize,
}

fn default_exec_mode() -> String {
    "blocklist".into()
}

fn default_max_output_bytes() -> usize {
    100_000
}

/// Tailscale integration configuration.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct TailscaleConfig {
    /// Enable Tailscale integration.
    #[serde(default)]
    pub enabled: bool,

    /// Enable Tailscale Funnel for public exposure.
    #[serde(default)]
    pub funnel: bool,
}

/// Browser automation configuration.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct BrowserConfig {
    /// Path to Chrome/Chromium binary (auto-detected if omitted).
    #[serde(skip_serializing_if = "Option::is_none")]
    pub chrome_path: Option<String>,

    /// Run in headless mode (default: true).
    #[serde(default = "default_true")]
    pub headless: bool,

    /// Maximum concurrent browser pages (default: 5).
    #[serde(default = "default_max_pages")]
    pub max_pages: usize,

    /// Page operation timeout in ms (default: 30000).
    #[serde(default = "default_browser_timeout")]
    pub timeout_ms: u64,
}

fn default_max_pages() -> usize {
    5
}

fn default_browser_timeout() -> u64 {
    30_000
}

#[derive(Debug, Clone, Default, Serialize, Deserialize)]
pub struct SessionConfig {
    /// Maximum context tokens before compaction triggers (default: 100,000).
    #[serde(skip_serializing_if = "Option::is_none")]
    pub max_context_tokens: Option<usize>,

    /// Automatically compact transcripts when they exceed the token limit.
    #[serde(default)]
    pub auto_compact: bool,

    /// Number of recent transcript entries to keep during compaction (default: 10).
    #[serde(skip_serializing_if = "Option::is_none")]
    pub compact_keep_recent: Option<usize>,
}

#[derive(Debug, Clone, Default, Serialize, Deserialize)]
pub struct CronConfig {
    #[serde(skip_serializing_if = "Option::is_none")]
    pub jobs: Option<Vec<CronJob>>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct CronJob {
    pub id: String,
    /// Cron expression (e.g. "0 9 * * *")
    pub schedule: String,
    /// Task prompt to send to the agent
    pub task: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub session_key: Option<String>,
    #[serde(default = "default_true")]
    pub enabled: bool,
}

#[derive(Debug, Clone, Default, Serialize, Deserialize)]
pub struct LoggingConfig {
    /// Log format: "plain" (default) or "json".
    #[serde(default = "default_log_format")]
    pub format: String,

    /// Log level override (trace/debug/info/warn/error).
    #[serde(skip_serializing_if = "Option::is_none")]
    pub level: Option<String>,

    /// Per-crate log level overrides (e.g. "rusty_claw_gateway=debug").
    #[serde(default)]
    pub filters: Vec<String>,

    /// Output target: "stderr" (default) or "stdout".
    #[serde(default = "default_log_output")]
    pub output: String,
}

fn default_log_format() -> String {
    "plain".into()
}

fn default_log_output() -> String {
    "stderr".into()
}

#[derive(Debug, Clone, Default, Serialize, Deserialize)]
pub struct PluginsConfig {}

#[derive(Debug, Clone, Default, Serialize, Deserialize)]
pub struct SkillsConfig {
    /// Directory to load skill definitions from (default: "skills/" in workspace).
    #[serde(skip_serializing_if = "Option::is_none")]
    pub dir: Option<String>,

    /// Automatically activate matching skills based on context.
    #[serde(default)]
    pub auto_activate: bool,
}

#[derive(Debug, Clone, Default, Serialize, Deserialize)]
pub struct MemoryConfig {
    /// Custom memory storage directory.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub dir: Option<String>,

    /// Maximum entries per namespace (0 = unlimited).
    #[serde(skip_serializing_if = "Option::is_none")]
    pub max_entries: Option<usize>,
}

/// Resolve a secret: check the direct value first, then the env-var reference.
pub fn resolve_secret_field(direct: &Option<String>, env_var: &Option<String>) -> Option<String> {
    if let Some(val) = direct {
        if !val.is_empty() {
            return Some(val.clone());
        }
    }
    if let Some(env) = env_var {
        if let Ok(val) = std::env::var(env) {
            if !val.is_empty() {
                return Some(val);
            }
        }
    }
    None
}

/// Substitute `${ENV_VAR}` patterns in a string with their environment variable values.
fn substitute_env_vars(input: &str) -> String {
    let re = regex::Regex::new(r"\$\{([^}]+)\}").unwrap();
    re.replace_all(input, |caps: &regex::Captures| {
        let var_name = &caps[1];
        std::env::var(var_name).unwrap_or_default()
    })
    .into_owned()
}

impl Config {
    /// Load config from a JSON5 file, substituting `${ENV_VAR}` references.
    pub fn load(path: &Path) -> crate::error::Result<Self> {
        if !path.exists() {
            return Ok(Self::default());
        }

        let raw = std::fs::read_to_string(path).map_err(crate::error::RustyClawError::Io)?;

        // Substitute ${ENV_VAR} references before parsing
        let substituted = substitute_env_vars(&raw);

        let config: Config = json5::from_str(&substituted)
            .map_err(|e| crate::error::RustyClawError::Config(e.to_string()))?;

        Ok(config)
    }

    /// Resolve the config directory path.
    pub fn config_dir() -> PathBuf {
        data_dir().join("config.json")
    }

    /// Resolve the workspace directory.
    pub fn workspace_dir(&self) -> PathBuf {
        self.agents
            .as_ref()
            .and_then(|a| a.defaults.as_ref())
            .and_then(|d| d.workspace.as_ref())
            .map(|w| {
                let expanded = shellexpand::tilde(w);
                PathBuf::from(expanded.as_ref())
            })
            .unwrap_or_else(|| data_dir().join("workspace"))
    }

    /// Gateway port.
    pub fn gateway_port(&self) -> u16 {
        self.gateway
            .as_ref()
            .map(|g| g.port)
            .unwrap_or(18789)
    }

    /// Get the default model name from the first provider.
    pub fn default_model(&self) -> String {
        self.models
            .as_ref()
            .and_then(|m| m.providers.as_ref())
            .and_then(|p| p.first())
            .and_then(|p| p.default_model.clone())
            .unwrap_or_else(|| "claude-sonnet-4-20250514".to_string())
    }

    /// Get the default max_tokens.
    pub fn max_tokens(&self) -> u32 {
        self.agents
            .as_ref()
            .and_then(|a| a.defaults.as_ref())
            .and_then(|d| d.max_tokens)
            .unwrap_or(4096)
    }

    /// Get the max tool iterations.
    pub fn max_tool_iterations(&self) -> u32 {
        self.agents
            .as_ref()
            .and_then(|a| a.defaults.as_ref())
            .and_then(|d| d.max_tool_iterations)
            .unwrap_or(25)
    }

    /// Get temperature setting.
    pub fn temperature(&self) -> Option<f64> {
        self.agents
            .as_ref()
            .and_then(|a| a.defaults.as_ref())
            .and_then(|d| d.temperature)
    }

    /// Get max context tokens setting.
    pub fn max_context_tokens(&self) -> usize {
        self.session
            .as_ref()
            .and_then(|s| s.max_context_tokens)
            .unwrap_or(100_000)
    }

    /// Get the number of recent entries to keep during compaction.
    pub fn compact_keep_recent(&self) -> usize {
        self.session
            .as_ref()
            .and_then(|s| s.compact_keep_recent)
            .unwrap_or(10)
    }

    /// Get the max spawn depth for multi-agent spawning.
    pub fn max_spawn_depth(&self) -> u32 {
        self.agents
            .as_ref()
            .and_then(|a| a.defaults.as_ref())
            .and_then(|d| d.max_spawn_depth)
            .unwrap_or(3)
    }

    /// Find a provider config by id.
    pub fn provider(&self, id: &str) -> Option<&ProviderConfig> {
        self.models
            .as_ref()
            .and_then(|m| m.providers.as_ref())
            .and_then(|p| p.iter().find(|pc| pc.id == id))
    }

    /// Get the first provider config.
    pub fn first_provider(&self) -> Option<&ProviderConfig> {
        self.models
            .as_ref()
            .and_then(|m| m.providers.as_ref())
            .and_then(|p| p.first())
    }

    /// Get a config value by dotted path (e.g. "gateway.port", "agents.defaults.model").
    pub fn get_path(&self, path: &str) -> Option<serde_json::Value> {
        let json = serde_json::to_value(self).ok()?;
        let mut current = &json;
        for segment in path.split('.') {
            current = current.get(segment)?;
        }
        Some(current.clone())
    }

    /// Set a config value by dotted path. Returns the modified config.
    pub fn set_path(&mut self, path: &str, value: serde_json::Value) -> anyhow::Result<()> {
        let mut json = serde_json::to_value(&*self)
            .map_err(|e| anyhow::anyhow!("Config serialization error: {e}"))?;

        let segments: Vec<&str> = path.split('.').collect();
        if segments.is_empty() {
            return Err(anyhow::anyhow!("Empty path"));
        }

        // Navigate to the parent of the target key
        let mut current = &mut json;
        for segment in &segments[..segments.len() - 1] {
            if current.get(segment).is_none() {
                current[segment] = serde_json::json!({});
            }
            current = current.get_mut(segment).unwrap();
        }

        // Set the value
        let last = segments.last().unwrap();
        current[last] = value;

        // Deserialize back
        *self = serde_json::from_value(json)
            .map_err(|e| anyhow::anyhow!("Config deserialization error: {e}"))?;
        Ok(())
    }

    /// Validate config, returning (warnings, errors).
    pub fn validate(&self) -> (Vec<String>, Vec<String>) {
        let mut warnings = Vec::new();
        let mut errors = Vec::new();

        // Check providers for API keys (skip ollama)
        if let Some(providers) = self
            .models
            .as_ref()
            .and_then(|m| m.providers.as_ref())
        {
            for p in providers {
                if p.id != "ollama" && p.resolve_api_key().is_none() {
                    warnings.push(format!(
                        "Provider '{}' has no API key configured",
                        p.id
                    ));
                }
            }
        }

        // Check TLS cert/key paths exist
        if let Some(tls) = self
            .gateway
            .as_ref()
            .and_then(|g| g.tls.as_ref())
        {
            if !Path::new(&tls.cert_path).exists() {
                errors.push(format!(
                    "TLS certificate file not found: {}",
                    tls.cert_path
                ));
            }
            if !Path::new(&tls.key_path).exists() {
                errors.push(format!(
                    "TLS key file not found: {}",
                    tls.key_path
                ));
            }
        }

        // Check port is non-zero
        if let Some(gw) = &self.gateway {
            if gw.port == 0 {
                errors.push("Gateway port cannot be 0".to_string());
            }
        }

        (warnings, errors)
    }

    /// Save config to a file.
    pub fn save(&self, path: &Path) -> anyhow::Result<()> {
        let json = serde_json::to_string_pretty(self)?;
        std::fs::write(path, json)?;
        Ok(())
    }
}

/// Base directory for Rusty Claw data: `~/.rusty_claw/`
pub fn data_dir() -> PathBuf {
    dirs::home_dir()
        .unwrap_or_else(|| PathBuf::from("."))
        .join(".rusty_claw")
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_env_var_substitution() {
        // SAFETY: test-only, single-threaded test runner
        unsafe { std::env::set_var("TEST_RC_KEY", "sk-test-123") };
        let input = r#"{"key": "${TEST_RC_KEY}", "other": "plain"}"#;
        let result = substitute_env_vars(input);
        assert!(result.contains("sk-test-123"));
        assert!(result.contains("plain"));
        unsafe { std::env::remove_var("TEST_RC_KEY") };
    }

    #[test]
    fn test_env_var_missing() {
        let input = r#"{"key": "${NONEXISTENT_VAR_RC_TEST}"}"#;
        let result = substitute_env_vars(input);
        assert!(result.contains(r#""""#)); // empty string
    }

    #[test]
    fn test_default_config() {
        let config = Config::default();
        assert_eq!(config.gateway_port(), 18789);
        assert_eq!(config.max_tokens(), 4096);
        assert_eq!(config.max_tool_iterations(), 25);
    }

    #[test]
    fn test_provider_resolve_api_key() {
        // SAFETY: test-only, single-threaded test runner
        unsafe { std::env::set_var("TEST_RC_API_KEY", "from-env") };
        let provider = ProviderConfig {
            id: "test".into(),
            api_key_env: Some("TEST_RC_API_KEY".into()),
            api_key: None,
            base_url: None,
            default_model: None,
        };
        assert_eq!(provider.resolve_api_key(), Some("from-env".into()));

        let provider2 = ProviderConfig {
            id: "test".into(),
            api_key_env: Some("TEST_RC_API_KEY".into()),
            api_key: Some("direct-key".into()),
            base_url: None,
            default_model: None,
        };
        // Direct key takes priority
        assert_eq!(provider2.resolve_api_key(), Some("direct-key".into()));
        unsafe { std::env::remove_var("TEST_RC_API_KEY") };
    }

    #[test]
    fn test_telegram_resolve_bot_token() {
        // SAFETY: test-only, single-threaded test runner
        unsafe { std::env::set_var("TEST_RC_TG_TOKEN", "bot-token-123") };
        let tg = TelegramConfig {
            bot_token: None,
            bot_token_env: Some("TEST_RC_TG_TOKEN".into()),
            allowed_users: vec![],
        };
        assert_eq!(tg.resolve_bot_token(), Some("bot-token-123".into()));
        unsafe { std::env::remove_var("TEST_RC_TG_TOKEN") };
    }

    // --- 6b-2: Structured Logging tests ---

    #[test]
    fn test_logging_config_defaults() {
        // Deserialize an empty logging config to get the serde defaults
        let json_str = r#"{ "logging": {} }"#;
        let config: Config = json5::from_str(json_str).unwrap();
        let logging = config.logging.expect("logging should be present");
        assert_eq!(logging.format, "plain");
        assert!(logging.level.is_none());
        assert_eq!(logging.output, "stderr");
        assert!(logging.filters.is_empty());
    }

    #[test]
    fn test_logging_config_json_deser() {
        let json_str = r#"{
            "logging": {
                "format": "json",
                "level": "debug",
                "output": "stdout"
            }
        }"#;
        let config: Config = json5::from_str(json_str).unwrap();
        let logging = config.logging.expect("logging should be present");
        assert_eq!(logging.format, "json");
        assert_eq!(logging.level.as_deref(), Some("debug"));
        assert_eq!(logging.output, "stdout");
    }

    #[test]
    fn test_logging_config_filters() {
        let json_str = r#"{
            "logging": {
                "filters": [
                    "rusty_claw_gateway=debug",
                    "rusty_claw_agent=trace"
                ]
            }
        }"#;
        let config: Config = json5::from_str(json_str).unwrap();
        let logging = config.logging.expect("logging should be present");
        assert_eq!(logging.filters.len(), 2);
        assert_eq!(logging.filters[0], "rusty_claw_gateway=debug");
        assert_eq!(logging.filters[1], "rusty_claw_agent=trace");
    }

    // --- 6b-5: Config Validation tests ---

    #[test]
    fn test_validate_missing_api_key_warns() {
        let config = Config {
            models: Some(ModelsConfig {
                providers: Some(vec![ProviderConfig {
                    id: "anthropic".into(),
                    api_key: None,
                    api_key_env: None,
                    base_url: None,
                    default_model: None,
                }]),
            }),
            ..Config::default()
        };
        let (warnings, _errors) = config.validate();
        assert!(
            warnings.iter().any(|w| w.contains("anthropic") && w.to_lowercase().contains("key")),
            "Expected a warning about missing API key for anthropic, got: {warnings:?}"
        );
    }

    #[test]
    fn test_validate_bad_tls_errors() {
        let config = Config {
            gateway: Some(GatewayConfig {
                port: 18789,
                bind: None,
                auth: None,
                tls: Some(TlsConfig {
                    cert_path: "/nonexistent/path/cert.pem".into(),
                    key_path: "/nonexistent/path/key.pem".into(),
                }),
                rate_limit: None,
                tailscale: None,
            }),
            ..Config::default()
        };
        let (_warnings, errors) = config.validate();
        assert!(
            !errors.is_empty(),
            "Expected errors for nonexistent TLS paths, got none"
        );
        assert!(
            errors.iter().any(|e| e.contains("cert")),
            "Expected an error about cert file, got: {errors:?}"
        );
    }
}
