//! Skill registry — loads and manages YAML skill definitions.

use std::collections::HashMap;
use std::path::Path;

use rusty_claw_core::skills::SkillDefinition;
use tracing::{debug, info, warn};

/// Registry of loaded skill definitions.
pub struct SkillRegistry {
    skills: HashMap<String, SkillDefinition>,
}

impl Default for SkillRegistry {
    fn default() -> Self {
        Self::new()
    }
}

impl SkillRegistry {
    pub fn new() -> Self {
        Self { skills: HashMap::new() }
    }

    /// Load all `.yaml`/`.yml` skills from a directory.
    pub fn load_from_dir(dir: &Path) -> Self {
        let mut registry = Self::new();
        if !dir.exists() {
            debug!(dir = %dir.display(), "Skills directory not found, skipping");
            return registry;
        }

        let entries = match std::fs::read_dir(dir) {
            Ok(e) => e,
            Err(e) => {
                warn!(%e, dir = %dir.display(), "Failed to read skills directory");
                return registry;
            }
        };

        for entry in entries.flatten() {
            let path = entry.path();
            let ext = path.extension().and_then(|e| e.to_str()).unwrap_or("");
            if ext == "yaml" || ext == "yml" {
                match SkillDefinition::load_from_file(&path) {
                    Ok(skill) => {
                        info!(name = %skill.name, "Loaded skill");
                        registry.skills.insert(skill.name.clone(), skill);
                    }
                    Err(e) => {
                        warn!(%e, path = %path.display(), "Failed to load skill");
                    }
                }
            }
        }

        info!(count = registry.skills.len(), "Skills loaded");
        registry
    }

    /// Reload all skills from the directory.
    pub fn reload(&mut self, dir: &Path) {
        *self = Self::load_from_dir(dir);
    }

    /// Get a skill by name.
    pub fn get(&self, name: &str) -> Option<&SkillDefinition> {
        self.skills.get(name)
    }

    /// List all skill names.
    pub fn list(&self) -> Vec<&str> {
        self.skills.keys().map(|s| s.as_str()).collect()
    }

    /// Get all skill definitions.
    pub fn all(&self) -> Vec<&SkillDefinition> {
        self.skills.values().collect()
    }
}
