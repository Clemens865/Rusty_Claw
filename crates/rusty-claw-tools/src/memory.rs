//! Memory tools — file-based key-value memory for the agent.
//!
//! Storage: `~/.rusty_claw/memory/{namespace}.json`
//! Each namespace is a JSON object mapping keys to string values.

use std::collections::HashMap;
use std::path::{Path, PathBuf};

use async_trait::async_trait;
use serde::Deserialize;
use tracing::debug;

use crate::{Tool, ToolContext, ToolOutput};

fn memory_dir(context: &ToolContext) -> PathBuf {
    context
        .config
        .memory
        .as_ref()
        .and_then(|m| m.dir.as_ref())
        .map(PathBuf::from)
        .unwrap_or_else(|| rusty_claw_core::config::data_dir().join("memory"))
}

fn namespace_path(base: &Path, namespace: &str) -> PathBuf {
    // Sanitize namespace to prevent path traversal
    let safe_name: String = namespace
        .chars()
        .map(|c| if c.is_alphanumeric() || c == '_' || c == '-' { c } else { '_' })
        .collect();
    base.join(format!("{safe_name}.json"))
}

fn load_namespace(path: &Path) -> HashMap<String, String> {
    if !path.exists() {
        return HashMap::new();
    }
    std::fs::read_to_string(path)
        .ok()
        .and_then(|s| serde_json::from_str(&s).ok())
        .unwrap_or_default()
}

fn save_namespace(path: &Path, data: &HashMap<String, String>) -> anyhow::Result<()> {
    if let Some(parent) = path.parent() {
        std::fs::create_dir_all(parent)?;
    }
    let json = serde_json::to_string_pretty(data)?;
    std::fs::write(path, json)?;
    Ok(())
}

// --- MemoryGetTool ---

pub struct MemoryGetTool;

#[derive(Deserialize)]
struct GetParams {
    key: String,
    #[serde(default = "default_namespace")]
    namespace: String,
}

fn default_namespace() -> String {
    "default".into()
}

#[async_trait]
impl Tool for MemoryGetTool {
    fn name(&self) -> &str {
        "memory_get"
    }

    fn description(&self) -> &str {
        "Retrieve a value from persistent memory by key."
    }

    fn parameters_schema(&self) -> serde_json::Value {
        serde_json::json!({
            "type": "object",
            "properties": {
                "key": {
                    "type": "string",
                    "description": "The key to look up"
                },
                "namespace": {
                    "type": "string",
                    "description": "Memory namespace (default: 'default')"
                }
            },
            "required": ["key"]
        })
    }

    async fn execute(
        &self,
        params: serde_json::Value,
        context: &ToolContext,
    ) -> anyhow::Result<ToolOutput> {
        let p: GetParams = serde_json::from_value(params)?;
        debug!(key = %p.key, namespace = %p.namespace, "memory_get");

        let dir = memory_dir(context);
        let path = namespace_path(&dir, &p.namespace);
        let data = load_namespace(&path);

        match data.get(&p.key) {
            Some(val) => Ok(ToolOutput {
                content: val.clone(),
                is_error: false,
                media: None,
            }),
            None => Ok(ToolOutput {
                content: format!("Key '{}' not found in namespace '{}'", p.key, p.namespace),
                is_error: false,
                media: None,
            }),
        }
    }
}

// --- MemorySetTool ---

pub struct MemorySetTool;

#[derive(Deserialize)]
struct SetParams {
    key: String,
    value: String,
    #[serde(default = "default_namespace")]
    namespace: String,
}

#[async_trait]
impl Tool for MemorySetTool {
    fn name(&self) -> &str {
        "memory_set"
    }

    fn description(&self) -> &str {
        "Store a key-value pair in persistent memory. This persists across sessions."
    }

    fn parameters_schema(&self) -> serde_json::Value {
        serde_json::json!({
            "type": "object",
            "properties": {
                "key": {
                    "type": "string",
                    "description": "The key to store"
                },
                "value": {
                    "type": "string",
                    "description": "The value to store"
                },
                "namespace": {
                    "type": "string",
                    "description": "Memory namespace (default: 'default')"
                }
            },
            "required": ["key", "value"]
        })
    }

    async fn execute(
        &self,
        params: serde_json::Value,
        context: &ToolContext,
    ) -> anyhow::Result<ToolOutput> {
        let p: SetParams = serde_json::from_value(params)?;
        debug!(key = %p.key, namespace = %p.namespace, "memory_set");

        let dir = memory_dir(context);
        let path = namespace_path(&dir, &p.namespace);
        let mut data = load_namespace(&path);
        data.insert(p.key.clone(), p.value);
        save_namespace(&path, &data)?;

        Ok(ToolOutput {
            content: format!("Stored key '{}' in namespace '{}'", p.key, p.namespace),
            is_error: false,
            media: None,
        })
    }
}

// --- MemoryListTool ---

pub struct MemoryListTool;

#[derive(Deserialize)]
struct ListParams {
    #[serde(default = "default_namespace")]
    namespace: String,
}

#[async_trait]
impl Tool for MemoryListTool {
    fn name(&self) -> &str {
        "memory_list"
    }

    fn description(&self) -> &str {
        "List all keys stored in a memory namespace."
    }

    fn parameters_schema(&self) -> serde_json::Value {
        serde_json::json!({
            "type": "object",
            "properties": {
                "namespace": {
                    "type": "string",
                    "description": "Memory namespace (default: 'default')"
                }
            }
        })
    }

    async fn execute(
        &self,
        params: serde_json::Value,
        context: &ToolContext,
    ) -> anyhow::Result<ToolOutput> {
        let p: ListParams = serde_json::from_value(params)?;
        debug!(namespace = %p.namespace, "memory_list");

        let dir = memory_dir(context);
        let path = namespace_path(&dir, &p.namespace);
        let data = load_namespace(&path);

        if data.is_empty() {
            return Ok(ToolOutput {
                content: format!("No entries in namespace '{}'", p.namespace),
                is_error: false,
                media: None,
            });
        }

        let mut keys: Vec<&str> = data.keys().map(|k| k.as_str()).collect();
        keys.sort();
        let content = keys.join("\n");

        Ok(ToolOutput {
            content: format!("Keys in '{}' ({}):\n{content}", p.namespace, keys.len()),
            is_error: false,
            media: None,
        })
    }
}

// --- MemorySearchTool ---

pub struct MemorySearchTool;

#[derive(Deserialize)]
struct SearchParams {
    query: String,
    #[serde(default = "default_namespace")]
    namespace: String,
}

#[async_trait]
impl Tool for MemorySearchTool {
    fn name(&self) -> &str {
        "memory_search"
    }

    fn description(&self) -> &str {
        "Search for keys and values containing the query string in a memory namespace."
    }

    fn parameters_schema(&self) -> serde_json::Value {
        serde_json::json!({
            "type": "object",
            "properties": {
                "query": {
                    "type": "string",
                    "description": "Substring to search for in keys and values"
                },
                "namespace": {
                    "type": "string",
                    "description": "Memory namespace (default: 'default')"
                }
            },
            "required": ["query"]
        })
    }

    async fn execute(
        &self,
        params: serde_json::Value,
        context: &ToolContext,
    ) -> anyhow::Result<ToolOutput> {
        let p: SearchParams = serde_json::from_value(params)?;
        debug!(query = %p.query, namespace = %p.namespace, "memory_search");

        let dir = memory_dir(context);
        let path = namespace_path(&dir, &p.namespace);
        let data = load_namespace(&path);

        let query_lower = p.query.to_lowercase();
        let matches: Vec<(&String, &String)> = data
            .iter()
            .filter(|(k, v)| {
                k.to_lowercase().contains(&query_lower) || v.to_lowercase().contains(&query_lower)
            })
            .collect();

        if matches.is_empty() {
            return Ok(ToolOutput {
                content: format!("No matches for '{}' in namespace '{}'", p.query, p.namespace),
                is_error: false,
                media: None,
            });
        }

        let mut output = format!("Found {} matches:\n\n", matches.len());
        for (k, v) in &matches {
            let preview = if v.len() > 100 {
                format!("{}...", &v[..100])
            } else {
                (*v).clone()
            };
            output.push_str(&format!("- **{k}**: {preview}\n"));
        }

        Ok(ToolOutput {
            content: output,
            is_error: false,
            media: None,
        })
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_namespace_path_sanitization() {
        let base = PathBuf::from("/tmp/memory");
        assert_eq!(
            namespace_path(&base, "default"),
            PathBuf::from("/tmp/memory/default.json")
        );
        assert_eq!(
            namespace_path(&base, "../escape"),
            PathBuf::from("/tmp/memory/___escape.json")
        );
        assert_eq!(
            namespace_path(&base, "my-ns_1"),
            PathBuf::from("/tmp/memory/my-ns_1.json")
        );
    }

    #[test]
    fn test_save_and_load_namespace() {
        let dir = tempfile::tempdir().unwrap();
        let path = dir.path().join("test.json");

        let mut data = HashMap::new();
        data.insert("key1".into(), "value1".into());
        data.insert("key2".into(), "value2".into());

        save_namespace(&path, &data).unwrap();
        let loaded = load_namespace(&path);
        assert_eq!(loaded.get("key1").unwrap(), "value1");
        assert_eq!(loaded.get("key2").unwrap(), "value2");
    }

    #[test]
    fn test_load_nonexistent() {
        let data = load_namespace(Path::new("/nonexistent/path.json"));
        assert!(data.is_empty());
    }
}
