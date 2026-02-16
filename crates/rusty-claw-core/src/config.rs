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

#[derive(Debug, Clone, Default, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum SandboxMode {
    Off,
    #[default]
    NonMain,
    All,
}

#[derive(Debug, Clone, Default, Serialize, Deserialize)]
pub struct ModelsConfig {
    #[serde(skip_serializing_if = "Option::is_none")]
    pub providers: Option<serde_json::Value>,
}

#[derive(Debug, Clone, Default, Serialize, Deserialize)]
pub struct ChannelsConfig {
    #[serde(skip_serializing_if = "Option::is_none")]
    pub telegram: Option<serde_json::Value>,

    #[serde(skip_serializing_if = "Option::is_none")]
    pub discord: Option<serde_json::Value>,

    #[serde(skip_serializing_if = "Option::is_none")]
    pub slack: Option<serde_json::Value>,

    #[serde(skip_serializing_if = "Option::is_none")]
    pub whatsapp: Option<serde_json::Value>,

    #[serde(skip_serializing_if = "Option::is_none")]
    pub signal: Option<serde_json::Value>,
}

#[derive(Debug, Clone, Default, Serialize, Deserialize)]
pub struct ToolsConfig {
    #[serde(skip_serializing_if = "Option::is_none")]
    pub allow: Option<Vec<String>>,

    #[serde(skip_serializing_if = "Option::is_none")]
    pub deny: Option<Vec<String>>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct GatewayConfig {
    #[serde(default = "default_port")]
    pub port: u16,

    #[serde(skip_serializing_if = "Option::is_none")]
    pub bind: Option<String>,

    #[serde(skip_serializing_if = "Option::is_none")]
    pub auth: Option<GatewayAuthConfig>,
}

fn default_port() -> u16 {
    18789
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct GatewayAuthConfig {
    #[serde(skip_serializing_if = "Option::is_none")]
    pub mode: Option<String>,

    #[serde(skip_serializing_if = "Option::is_none")]
    pub token: Option<String>,

    #[serde(skip_serializing_if = "Option::is_none")]
    pub password: Option<String>,
}

#[derive(Debug, Clone, Default, Serialize, Deserialize)]
pub struct SessionConfig {}

#[derive(Debug, Clone, Default, Serialize, Deserialize)]
pub struct CronConfig {}

#[derive(Debug, Clone, Default, Serialize, Deserialize)]
pub struct LoggingConfig {}

#[derive(Debug, Clone, Default, Serialize, Deserialize)]
pub struct PluginsConfig {}

#[derive(Debug, Clone, Default, Serialize, Deserialize)]
pub struct SkillsConfig {}

#[derive(Debug, Clone, Default, Serialize, Deserialize)]
pub struct MemoryConfig {}

impl Config {
    /// Load config from a JSON5 file.
    pub fn load(path: &Path) -> crate::error::Result<Self> {
        if !path.exists() {
            return Ok(Self::default());
        }

        let raw = std::fs::read_to_string(path).map_err(crate::error::RustyClawError::Io)?;

        // TODO: Process $include directives
        // TODO: Substitute ${ENV_VAR} references

        let config: Config =
            json5::from_str(&raw).map_err(|e| crate::error::RustyClawError::Config(e.to_string()))?;

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
}

/// Base directory for Rusty Claw data: `~/.rusty_claw/`
pub fn data_dir() -> PathBuf {
    dirs::home_dir()
        .unwrap_or_else(|| PathBuf::from("."))
        .join(".rusty_claw")
}
