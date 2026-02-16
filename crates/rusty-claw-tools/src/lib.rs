//! Built-in tool implementations for the agent runtime.
//!
//! Tools are capabilities exposed to the LLM during agent runs.
//! Each tool implements the [`Tool`] trait.

use std::path::PathBuf;
use std::sync::Arc;

use async_trait::async_trait;
use serde::{Deserialize, Serialize};

use rusty_claw_core::config::Config;

/// Context provided to tools during execution.
pub struct ToolContext {
    pub session_key: String,
    pub workspace: PathBuf,
    pub config: Arc<Config>,
    pub restrict_to_workspace: bool,
}

/// Output from a tool execution.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ToolOutput {
    pub content: String,
    pub is_error: bool,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub media: Option<Vec<ToolMedia>>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ToolMedia {
    pub mime_type: String,
    pub data: String,
}

/// The core tool trait. Every built-in and plugin tool implements this.
#[async_trait]
pub trait Tool: Send + Sync {
    /// Tool name as exposed to the LLM (e.g., "exec", "read_file").
    fn name(&self) -> &str;

    /// JSON Schema describing the tool's parameters.
    fn parameters_schema(&self) -> serde_json::Value;

    /// Human-readable description for the LLM.
    fn description(&self) -> &str;

    /// Execute the tool with the given parameters.
    async fn execute(
        &self,
        params: serde_json::Value,
        context: &ToolContext,
    ) -> anyhow::Result<ToolOutput>;
}

/// Registry of available tools.
#[derive(Default)]
pub struct ToolRegistry {
    tools: Vec<Box<dyn Tool>>,
}

impl ToolRegistry {
    pub fn new() -> Self {
        Self::default()
    }

    pub fn register(&mut self, tool: Box<dyn Tool>) {
        self.tools.push(tool);
    }

    pub fn get(&self, name: &str) -> Option<&dyn Tool> {
        self.tools.iter().find(|t| t.name() == name).map(|t| t.as_ref())
    }

    pub fn list(&self) -> Vec<&str> {
        self.tools.iter().map(|t| t.name()).collect()
    }

    /// Generate tool definitions for the LLM API request.
    pub fn to_llm_tools(&self) -> Vec<serde_json::Value> {
        self.tools
            .iter()
            .map(|t| {
                serde_json::json!({
                    "name": t.name(),
                    "description": t.description(),
                    "input_schema": t.parameters_schema(),
                })
            })
            .collect()
    }
}
