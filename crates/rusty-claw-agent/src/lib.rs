//! Agent runtime — orchestrates LLM interactions with tool-calling loops.
//!
//! The agent runtime takes an inbound message, builds a system prompt,
//! streams the LLM response, executes tool calls, and produces a stream
//! of [`AgentEvent`]s for the gateway to broadcast.

use serde::{Deserialize, Serialize};

pub mod prompt;
pub mod runtime;
pub mod transcript;

pub use runtime::run_agent;

/// Events emitted by the agent runtime during a run.
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(tag = "type")]
pub enum AgentEvent {
    /// A chunk of assistant text ready for delivery.
    #[serde(rename = "block_reply")]
    BlockReply { text: String, is_final: bool },

    /// Reasoning/thinking content (when enabled).
    #[serde(rename = "reasoning")]
    ReasoningStream { text: String },

    /// A tool call is being made.
    #[serde(rename = "tool_call")]
    ToolCall {
        tool: String,
        params: serde_json::Value,
    },

    /// A tool call has completed.
    #[serde(rename = "tool_result")]
    ToolResult {
        tool: String,
        content: String,
        is_error: bool,
    },

    /// Streaming text delta for live typing indicators.
    #[serde(rename = "partial_reply")]
    PartialReply { delta: String },

    /// Token usage for the current run.
    #[serde(rename = "usage")]
    Usage {
        input_tokens: u64,
        output_tokens: u64,
    },

    /// An error occurred during the run.
    #[serde(rename = "error")]
    Error { kind: String, message: String },
}

/// Result of a completed agent run.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct AgentRunResult {
    pub payloads: Vec<AgentPayload>,
    pub meta: AgentRunMeta,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct AgentPayload {
    pub text: Option<String>,
    pub media_urls: Vec<String>,
    pub is_error: bool,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct AgentRunMeta {
    pub duration_ms: u64,
    pub input_tokens: u64,
    pub output_tokens: u64,
    pub tool_calls: u32,
    pub aborted: bool,
    pub stop_reason: Option<String>,
    pub error: Option<AgentRunError>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct AgentRunError {
    pub kind: AgentErrorKind,
    pub message: String,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum AgentErrorKind {
    ContextOverflow,
    CompactionFailure,
    AuthFailure,
    ProviderError,
    ToolError,
    Timeout,
    Aborted,
}
