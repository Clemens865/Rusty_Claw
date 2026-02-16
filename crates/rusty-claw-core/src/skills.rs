//! Skill definitions — YAML-based skill configurations.

use std::path::{Path, PathBuf};

use serde::{Deserialize, Serialize};

/// A skill definition loaded from a YAML file.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct SkillDefinition {
    /// Unique skill name.
    pub name: String,

    /// Human-readable description.
    pub description: String,

    /// System prompt injected when this skill is active.
    #[serde(default)]
    pub system_prompt: String,

    /// Restrict agent to only these tools when skill is active.
    #[serde(default)]
    pub tools: Vec<String>,

    /// Tags for categorization and search.
    #[serde(default)]
    pub tags: Vec<String>,

    /// Usage examples.
    #[serde(default)]
    pub examples: Vec<SkillExample>,

    /// Path to the source YAML file.
    #[serde(skip)]
    pub file_path: PathBuf,
}

/// An example showing how to use a skill.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct SkillExample {
    pub input: String,
    #[serde(default)]
    pub description: String,
}

impl SkillDefinition {
    /// Load a skill definition from a YAML file.
    pub fn load_from_file(path: &Path) -> anyhow::Result<Self> {
        let content = std::fs::read_to_string(path)?;
        let mut skill: SkillDefinition = serde_yaml::from_str(&content)?;
        skill.file_path = path.to_path_buf();
        Ok(skill)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_parse_skill_yaml() {
        let yaml = r#"
name: code_review
description: Review code for bugs, style, and best practices
system_prompt: |
  You are a code reviewer. Focus on:
  - Security vulnerabilities
  - Performance issues
  - Code style and readability
tools:
  - read_file
  - exec
tags:
  - development
  - review
examples:
  - input: "Review the changes in src/main.rs"
    description: "Basic code review"
"#;
        let skill: SkillDefinition = serde_yaml::from_str(yaml).unwrap();
        assert_eq!(skill.name, "code_review");
        assert_eq!(skill.tools.len(), 2);
        assert_eq!(skill.tags.len(), 2);
        assert_eq!(skill.examples.len(), 1);
    }

    #[test]
    fn test_parse_minimal_skill() {
        let yaml = r#"
name: simple
description: A simple skill
"#;
        let skill: SkillDefinition = serde_yaml::from_str(yaml).unwrap();
        assert_eq!(skill.name, "simple");
        assert!(skill.tools.is_empty());
        assert!(skill.system_prompt.is_empty());
    }
}
