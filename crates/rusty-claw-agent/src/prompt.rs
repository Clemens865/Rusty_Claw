//! System prompt builder for the agent.

use std::path::Path;
use std::sync::Arc;

use rusty_claw_core::config::Config;
use rusty_claw_core::skills::SkillDefinition;
use rusty_claw_tools::ToolRegistry;

/// Build the system prompt for the agent.
pub fn build_system_prompt(
    _config: &Arc<Config>,
    tools: &ToolRegistry,
    workspace: &Path,
    active_skills: &[&SkillDefinition],
) -> String {
    build_system_prompt_with_persona(_config, tools, workspace, active_skills, None)
}

/// Build the system prompt with an optional custom persona override.
pub fn build_system_prompt_with_persona(
    _config: &Arc<Config>,
    tools: &ToolRegistry,
    workspace: &Path,
    active_skills: &[&SkillDefinition],
    custom_system_prompt: Option<&str>,
) -> String {
    let mut parts = Vec::new();

    let identity = custom_system_prompt
        .unwrap_or("You are a helpful personal AI assistant powered by Rusty Claw.");
    parts.push(identity.to_string());

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

    let tools_path = workspace.join("TOOLS.md");
    if tools_path.exists() {
        if let Ok(content) = std::fs::read_to_string(&tools_path) {
            parts.push(format!("--- Tool Instructions ---\n{content}"));
        }
    }

    // Inject active skill prompts
    for skill in active_skills {
        if !skill.system_prompt.is_empty() {
            parts.push(format!(
                "--- Active Skill: {} ---\n{}",
                skill.name, skill.system_prompt
            ));
        }
    }

    parts.join("\n\n")
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_tools_md_loaded_in_prompt() {
        let dir = tempfile::tempdir().unwrap();
        let workspace = dir.path();
        std::fs::write(
            workspace.join("TOOLS.md"),
            "Always use exec with caution.",
        )
        .unwrap();

        let config = Arc::new(Config::default());
        let tools = ToolRegistry::new();
        let prompt = build_system_prompt(&config, &tools, workspace, &[]);
        assert!(prompt.contains("--- Tool Instructions ---"));
        assert!(prompt.contains("Always use exec with caution."));
    }

    // --- 6c-3: Persona tests ---

    #[test]
    fn test_persona_overrides_default() {
        let dir = tempfile::tempdir().unwrap();
        let workspace = dir.path();
        let config = Arc::new(Config::default());
        let tools = ToolRegistry::new();

        let custom_persona = "You are Jarvis, a sophisticated AI butler.";
        let prompt = build_system_prompt_with_persona(
            &config,
            &tools,
            workspace,
            &[],
            Some(custom_persona),
        );

        assert!(
            prompt.contains(custom_persona),
            "Prompt should contain the custom persona, got: {prompt}"
        );
        assert!(
            !prompt.contains("You are a helpful personal AI assistant powered by Rusty Claw."),
            "Prompt should NOT contain the default identity when a custom persona is set"
        );
    }

    #[test]
    fn test_persona_none_uses_default() {
        let dir = tempfile::tempdir().unwrap();
        let workspace = dir.path();
        let config = Arc::new(Config::default());
        let tools = ToolRegistry::new();

        let prompt = build_system_prompt_with_persona(
            &config,
            &tools,
            workspace,
            &[],
            None,
        );

        assert!(
            prompt.contains("You are a helpful personal AI assistant powered by Rusty Claw."),
            "Prompt should contain the default identity when no persona is set, got: {prompt}"
        );
    }
}
