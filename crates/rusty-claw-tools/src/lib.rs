//! Built-in tool implementations for the agent runtime.
//!
//! Tools are capabilities exposed to the LLM during agent runs.
//! Each tool implements the [`Tool`] trait.

use std::path::PathBuf;
use std::sync::Arc;

use async_trait::async_trait;
use serde::{Deserialize, Serialize};

use rusty_claw_browser::BrowserPool;
use rusty_claw_core::config::Config;

pub mod browser;
pub mod canvas;
pub mod edit_file;
pub mod exec;
pub mod image_generation;
pub mod memory;
pub mod path_guard;
pub mod read_file;
pub mod sessions;
pub mod transcription;
pub mod tts;
pub mod web_fetch;
pub mod web_search;
pub mod write_file;

/// Context provided to tools during execution.
pub struct ToolContext {
    pub session_key: String,
    pub workspace: PathBuf,
    pub config: Arc<Config>,
    pub restrict_to_workspace: bool,
    pub sandbox_mode: rusty_claw_core::config::SandboxMode,
    pub browser_pool: Option<Arc<BrowserPool>>,
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

    /// Access all registered tool objects.
    pub fn tools(&self) -> &[Box<dyn Tool>] {
        &self.tools
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

/// Register all built-in tools.
pub fn register_builtin_tools(registry: &mut ToolRegistry) {
    // Filesystem tools
    registry.register(Box::new(exec::ExecTool));
    registry.register(Box::new(read_file::ReadFileTool));
    registry.register(Box::new(write_file::WriteFileTool));
    registry.register(Box::new(edit_file::EditFileTool));

    // Web tools
    registry.register(Box::new(web_fetch::WebFetchTool));
    registry.register(Box::new(web_search::WebSearchTool));

    // Memory tools
    registry.register(Box::new(memory::MemoryGetTool));
    registry.register(Box::new(memory::MemorySetTool));
    registry.register(Box::new(memory::MemoryListTool));
    registry.register(Box::new(memory::MemorySearchTool));

    // Session tools
    registry.register(Box::new(sessions::SessionsListTool));
    registry.register(Box::new(sessions::SessionsSendTool));

    // Multimedia tools
    registry.register(Box::new(tts::TtsTool));
    registry.register(Box::new(image_generation::ImageGenerationTool));
    registry.register(Box::new(transcription::TranscriptionTool));

    // Browser tools
    registry.register(Box::new(browser::BrowserNavigateTool));
    registry.register(Box::new(browser::BrowserScreenshotTool));
    registry.register(Box::new(browser::BrowserClickTool));
    registry.register(Box::new(browser::BrowserExtractTextTool));
    registry.register(Box::new(browser::BrowserEvaluateJsTool));
    registry.register(Box::new(browser::BrowserWaitForTool));

    // Canvas tool
    registry.register(Box::new(canvas::CanvasTool));
}
