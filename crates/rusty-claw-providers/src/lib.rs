//! LLM provider abstraction.
//!
//! Each provider implements the [`LlmProvider`] trait to support streaming
//! chat completions from different LLM APIs (Anthropic, OpenAI, Google, etc.).

use std::collections::HashMap;
use std::pin::Pin;
use std::sync::Arc;

use async_trait::async_trait;
use futures::Stream;
use serde::{Deserialize, Serialize};

use rusty_claw_core::session::TranscriptEntry;

pub mod anthropic;
pub mod failover;
pub mod google;
pub mod openai;
pub mod sse;

/// Supported LLM API protocols.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum ModelApi {
    AnthropicMessages,
    OpenAiCompletions,
    OpenAiResponses,
    GoogleGenerativeAi,
    BedrockConverseStream,
    Ollama,
    GithubCopilot,
}

/// Credentials for authenticating with an LLM provider.
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(tag = "type")]
pub enum Credentials {
    #[serde(rename = "api_key")]
    ApiKey { api_key: String },
    #[serde(rename = "oauth")]
    OAuth { access_token: String, refresh_token: Option<String> },
    #[serde(rename = "token")]
    Token { token: String },
}

/// Provider-agnostic tool definition.
///
/// Providers translate these into their API-specific format
/// via [`LlmProvider::format_tools`].
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ToolDefinition {
    pub name: String,
    pub description: String,
    pub parameters_schema: serde_json::Value,
}

/// A request to the LLM.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct CompletionRequest {
    pub model: String,
    pub messages: Vec<serde_json::Value>,
    pub max_tokens: u32,
    pub temperature: Option<f64>,
    pub tools: Option<Vec<serde_json::Value>>,
    pub system: Option<String>,
}

/// A streamed chunk from the LLM.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct CompletionChunk {
    pub delta: Option<String>,
    pub thinking: Option<String>,
    pub tool_use: Option<ToolUseChunk>,
    pub usage: Option<ChunkUsage>,
    pub stop_reason: Option<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ToolUseChunk {
    pub id: String,
    pub name: String,
    pub input_json: String,
}

#[derive(Debug, Clone, Default, Serialize, Deserialize)]
pub struct ChunkUsage {
    pub input_tokens: Option<u64>,
    pub output_tokens: Option<u64>,
}

/// Model metadata.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ModelInfo {
    pub id: String,
    pub name: String,
    pub api: ModelApi,
    pub reasoning: bool,
    pub context_window: u32,
    pub max_tokens: u32,
}

/// The core LLM provider trait.
#[async_trait]
pub trait LlmProvider: Send + Sync {
    /// Provider identifier (e.g., "anthropic", "openai").
    fn id(&self) -> &str;

    /// API protocol used by this provider.
    fn api(&self) -> ModelApi;

    /// Format tool definitions into this provider's API format.
    fn format_tools(&self, tools: &[ToolDefinition]) -> Vec<serde_json::Value>;

    /// Format a transcript into this provider's message format.
    fn format_messages(&self, transcript: &[TranscriptEntry]) -> Vec<serde_json::Value>;

    /// Check if a stop reason indicates the model wants to call tools.
    fn is_tool_use_stop(&self, stop_reason: &str) -> bool;

    /// Stream a chat completion.
    async fn stream(
        &self,
        request: &CompletionRequest,
        credentials: &Credentials,
    ) -> anyhow::Result<Pin<Box<dyn Stream<Item = anyhow::Result<CompletionChunk>> + Send>>>;

    /// List available models from this provider.
    async fn list_models(&self, credentials: &Credentials) -> anyhow::Result<Vec<ModelInfo>>;
}

/// Registry of named LLM providers with credentials.
///
/// Allows the gateway to support multiple providers simultaneously
/// and select one by name or fall back to the default.
pub struct ProviderRegistry {
    providers: HashMap<String, (Arc<dyn LlmProvider>, Credentials)>,
    default_id: String,
}

impl ProviderRegistry {
    pub fn new(default_id: String) -> Self {
        Self {
            providers: HashMap::new(),
            default_id,
        }
    }

    /// Register a provider with its credentials under a given ID.
    pub fn register(&mut self, id: String, provider: Arc<dyn LlmProvider>, credentials: Credentials) {
        self.providers.insert(id, (provider, credentials));
    }

    /// Look up a provider and its credentials by ID.
    pub fn get(&self, id: &str) -> Option<(&dyn LlmProvider, &Credentials)> {
        self.providers.get(id).map(|(p, c)| (p.as_ref(), c))
    }

    /// Get the default provider and credentials.
    pub fn default(&self) -> Option<(&dyn LlmProvider, &Credentials)> {
        self.get(&self.default_id)
    }

    /// The ID of the default provider.
    pub fn default_id(&self) -> &str {
        &self.default_id
    }

    /// List all registered provider IDs.
    pub fn list_ids(&self) -> Vec<&str> {
        self.providers.keys().map(|s| s.as_str()).collect()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_provider_registry_register_and_get() {
        let provider = Arc::new(anthropic::AnthropicProvider::new(None));
        let creds = Credentials::ApiKey {
            api_key: "test-key".into(),
        };
        let mut registry = ProviderRegistry::new("anthropic".into());
        registry.register("anthropic".into(), provider, creds);

        assert!(registry.get("anthropic").is_some());
        assert!(registry.get("openai").is_none());
    }

    #[test]
    fn test_provider_registry_default() {
        let provider = Arc::new(anthropic::AnthropicProvider::new(None));
        let creds = Credentials::ApiKey {
            api_key: "test-key".into(),
        };
        let mut registry = ProviderRegistry::new("anthropic".into());
        registry.register("anthropic".into(), provider, creds);

        let (p, _c) = registry.default().expect("should have default");
        assert_eq!(p.id(), "anthropic");
        assert_eq!(registry.default_id(), "anthropic");
    }

    #[test]
    fn test_provider_registry_list_ids() {
        let mut registry = ProviderRegistry::new("anthropic".into());
        let provider = Arc::new(anthropic::AnthropicProvider::new(None));
        let creds = Credentials::ApiKey {
            api_key: "k".into(),
        };
        registry.register("anthropic".into(), provider.clone(), creds.clone());
        registry.register("openai".into(), provider, creds);

        let ids = registry.list_ids();
        assert_eq!(ids.len(), 2);
        assert!(ids.contains(&"anthropic"));
        assert!(ids.contains(&"openai"));
    }
}
