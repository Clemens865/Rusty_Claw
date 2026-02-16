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
    pub whatsapp: Option<serde_json::Value>,

    #[serde(skip_serializing_if = "Option::is_none")]
    pub signal: Option<serde_json::Value>,
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
pub struct SessionConfig {}

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
pub struct LoggingConfig {}

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
}
