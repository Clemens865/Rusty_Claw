//! LLM provider abstraction.
//!
//! Each provider implements the [`LlmProvider`] trait to support streaming
//! chat completions from different LLM APIs (Anthropic, OpenAI, Google, etc.).

use std::pin::Pin;

use async_trait::async_trait;
use futures::Stream;
use serde::{Deserialize, Serialize};

pub mod anthropic;
pub mod openai;

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

    /// Stream a chat completion.
    async fn stream(
        &self,
        request: &CompletionRequest,
        credentials: &Credentials,
    ) -> anyhow::Result<Pin<Box<dyn Stream<Item = anyhow::Result<CompletionChunk>> + Send>>>;

    /// List available models from this provider.
    async fn list_models(&self, credentials: &Credentials) -> anyhow::Result<Vec<ModelInfo>>;
}
