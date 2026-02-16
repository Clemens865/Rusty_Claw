//! System prompt builder for the agent.

use std::path::Path;
use std::sync::Arc;

use rusty_claw_core::config::Config;
use rusty_claw_tools::ToolRegistry;

/// Build the system prompt for the agent.
pub fn build_system_prompt(_config: &Arc<Config>, tools: &ToolRegistry, workspace: &Path) -> String {
    let mut parts = Vec::new();

    parts.push("You are a helpful personal AI assistant powered by Rusty Claw.".to_string());

    // Add current time
    let now = chrono::Utc::now();
    parts.push(format!("Current time: {}", now.format("%Y-%m-%d %H:%M:%S UTC")));

    // Workspace info
    parts.push(format!("Workspace directory: {}", workspace.display()));

    // Available tools
    let tool_names = tools.list();
    if !tool_names.is_empty() {
        parts.push(format!(
            "Available tools: {}",
            tool_names.join(", ")
        ));
    }

    // Check for workspace personality files
    let soul_path = workspace.join("SOUL.md");
    if soul_path.exists() {
        if let Ok(content) = std::fs::read_to_string(&soul_path) {
            parts.push(format!("--- Personality ---\n{content}"));
        }
    }

    let agents_path = workspace.join("AGENTS.md");
    if agents_path.exists() {
        if let Ok(content) = std::fs::read_to_string(&agents_path) {
            parts.push(format!("--- Agent Instructions ---\n{content}"));
        }
    }

    parts.join("\n\n")
}
