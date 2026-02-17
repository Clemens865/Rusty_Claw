//! Agent spawning tool — allows the LLM to spawn sub-agents.

use async_trait::async_trait;
use serde_json::json;

use crate::{Tool, ToolContext, ToolOutput};

pub struct AgentsSpawnTool;

#[async_trait]
impl Tool for AgentsSpawnTool {
    fn name(&self) -> &str {
        "agents_spawn"
    }

    fn description(&self) -> &str {
        "Spawn a sub-agent to handle a delegated task. Returns a session key for the child agent. The gateway will execute the child agent asynchronously."
    }

    fn parameters_schema(&self) -> serde_json::Value {
        json!({
            "type": "object",
            "properties": {
                "task": {
                    "type": "string",
                    "description": "The task description for the sub-agent to work on"
                },
                "model": {
                    "type": "string",
                    "description": "Optional model override for the sub-agent"
                }
            },
            "required": ["task"]
        })
    }

    async fn execute(
        &self,
        params: serde_json::Value,
        _context: &ToolContext,
    ) -> anyhow::Result<ToolOutput> {
        let task = params
            .get("task")
            .and_then(|v| v.as_str())
            .unwrap_or("(no task specified)")
            .to_string();

        let model = params
            .get("model")
            .and_then(|v| v.as_str())
            .map(String::from);

        // The actual spawning is handled by the gateway's agents.spawn WS method.
        // This tool returns a placeholder; the gateway intercepts agents_spawn tool
        // calls and performs the actual spawning.
        Ok(ToolOutput {
            content: json!({
                "status": "spawn_requested",
                "task": task,
                "model": model,
                "note": "The gateway will execute this spawn request"
            })
            .to_string(),
            is_error: false,
            media: None,
        })
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::path::PathBuf;
    use std::sync::Arc;

    fn test_context() -> ToolContext {
        ToolContext {
            session_key: "test".into(),
            workspace: PathBuf::from("/tmp"),
            config: Arc::new(rusty_claw_core::config::Config::default()),
            restrict_to_workspace: false,
            sandbox_mode: rusty_claw_core::config::SandboxMode::Off,
            browser_pool: None,
        }
    }

    #[tokio::test]
    async fn test_agents_spawn_output_format() {
        let tool = AgentsSpawnTool;
        let result = tool
            .execute(json!({"task": "research rust"}), &test_context())
            .await
            .unwrap();
        assert!(!result.is_error);
        assert!(result.content.contains("spawn_requested"));
        assert!(result.content.contains("research rust"));
    }

    #[tokio::test]
    async fn test_agents_spawn_with_model() {
        let tool = AgentsSpawnTool;
        let result = tool
            .execute(
                json!({"task": "summarize", "model": "claude-haiku"}),
                &test_context(),
            )
            .await
            .unwrap();
        assert!(!result.is_error);
        assert!(result.content.contains("claude-haiku"));
    }

    // --- 6c-4: Spawning depth limit test ---

    #[test]
    fn test_spawn_depth_limit() {
        use rusty_claw_core::config::{Config, AgentsConfig, AgentDefaults};

        // Default max_spawn_depth should be 3
        let config = Config::default();
        assert_eq!(config.max_spawn_depth(), 3);

        // When explicitly set, it should return the override
        let config_with_override = Config {
            agents: Some(AgentsConfig {
                defaults: Some(AgentDefaults {
                    workspace: None,
                    model: None,
                    max_tokens: None,
                    temperature: None,
                    max_tool_iterations: None,
                    sandbox: None,
                    thinking_budget_tokens: None,
                    max_spawn_depth: Some(5),
                }),
            }),
            ..Config::default()
        };
        assert_eq!(config_with_override.max_spawn_depth(), 5);

        // Setting to 1 should also work
        let config_depth_1 = Config {
            agents: Some(AgentsConfig {
                defaults: Some(AgentDefaults {
                    workspace: None,
                    model: None,
                    max_tokens: None,
                    temperature: None,
                    max_tool_iterations: None,
                    sandbox: None,
                    thinking_budget_tokens: None,
                    max_spawn_depth: Some(1),
                }),
            }),
            ..Config::default()
        };
        assert_eq!(config_depth_1.max_spawn_depth(), 1);
    }
}
