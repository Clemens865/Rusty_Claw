//! File listing tool with glob pattern support.

use async_trait::async_trait;
use serde_json::json;

use crate::path_guard::validate_path;
use crate::{Tool, ToolContext, ToolOutput};

pub struct FileListTool;

#[async_trait]
impl Tool for FileListTool {
    fn name(&self) -> &str {
        "file_list"
    }

    fn description(&self) -> &str {
        "List files and directories in a path, optionally filtered by glob pattern."
    }

    fn parameters_schema(&self) -> serde_json::Value {
        json!({
            "type": "object",
            "properties": {
                "path": {
                    "type": "string",
                    "description": "Directory to list (relative to workspace or absolute). Default: \".\""
                },
                "pattern": {
                    "type": "string",
                    "description": "Glob pattern to filter entries (e.g. \"*.rs\", \"src/**/*.ts\")"
                },
                "recursive": {
                    "type": "boolean",
                    "description": "Whether to recurse into subdirectories. Default: false"
                },
                "limit": {
                    "type": "integer",
                    "description": "Maximum number of entries to return. Default: 200"
                }
            }
        })
    }

    async fn execute(
        &self,
        params: serde_json::Value,
        context: &ToolContext,
    ) -> anyhow::Result<ToolOutput> {
        let raw_path = params
            .get("path")
            .and_then(|v| v.as_str())
            .unwrap_or(".");
        let pattern = params.get("pattern").and_then(|v| v.as_str());
        let recursive = params
            .get("recursive")
            .and_then(|v| v.as_bool())
            .unwrap_or(false);
        let limit = params
            .get("limit")
            .and_then(|v| v.as_u64())
            .unwrap_or(200) as usize;

        let dir_path =
            match validate_path(raw_path, &context.workspace, context.restrict_to_workspace) {
                Ok(p) => p,
                Err(e) => {
                    return Ok(ToolOutput {
                        content: format!("Path error: {e}"),
                        is_error: true,
                        media: None,
                    });
                }
            };

        if !dir_path.exists() {
            return Ok(ToolOutput {
                content: format!("Directory not found: {}", dir_path.display()),
                is_error: true,
                media: None,
            });
        }

        if !dir_path.is_dir() {
            return Ok(ToolOutput {
                content: format!("Not a directory: {}", dir_path.display()),
                is_error: true,
                media: None,
            });
        }

        // If a glob pattern is provided, use it
        if let Some(pat) = pattern {
            let glob_pattern = if recursive {
                format!("{}/**/{pat}", dir_path.display())
            } else {
                format!("{}/{pat}", dir_path.display())
            };

            let mut entries = Vec::new();
            match glob::glob(&glob_pattern) {
                Ok(paths) => {
                    for path in paths.take(limit).flatten() {
                        let relative = path
                            .strip_prefix(&dir_path)
                            .unwrap_or(&path);
                        if path.is_dir() {
                            entries.push(format!("[dir]  {}/", relative.display()));
                        } else {
                            let size = std::fs::metadata(&path)
                                .map(|m| m.len())
                                .unwrap_or(0);
                            entries.push(format!(
                                "[file] {} ({size} bytes)",
                                relative.display(),
                            ));
                        }
                    }
                }
                Err(e) => {
                    return Ok(ToolOutput {
                        content: format!("Invalid glob pattern: {e}"),
                        is_error: true,
                        media: None,
                    });
                }
            }

            if entries.is_empty() {
                return Ok(ToolOutput {
                    content: "No matching entries found.".into(),
                    is_error: false,
                    media: None,
                });
            }

            return Ok(ToolOutput {
                content: entries.join("\n"),
                is_error: false,
                media: None,
            });
        }

        // No pattern — simple directory listing
        let mut entries = Vec::new();
        let read_result = if recursive {
            collect_recursive(&dir_path, &dir_path, &mut entries, limit)
        } else {
            collect_flat(&dir_path, &mut entries, limit)
        };

        if let Err(e) = read_result {
            return Ok(ToolOutput {
                content: format!("Read error: {e}"),
                is_error: true,
                media: None,
            });
        }

        if entries.is_empty() {
            return Ok(ToolOutput {
                content: "(empty directory)".into(),
                is_error: false,
                media: None,
            });
        }

        Ok(ToolOutput {
            content: entries.join("\n"),
            is_error: false,
            media: None,
        })
    }
}

fn collect_flat(
    dir: &std::path::Path,
    entries: &mut Vec<String>,
    limit: usize,
) -> std::io::Result<()> {
    let mut read_dir: Vec<_> = std::fs::read_dir(dir)?
        .filter_map(|e| e.ok())
        .collect();
    read_dir.sort_by_key(|e| e.file_name());

    for entry in read_dir {
        if entries.len() >= limit {
            entries.push(format!("... (truncated at {limit} entries)"));
            break;
        }
        let path = entry.path();
        let name = entry.file_name();
        if path.is_dir() {
            entries.push(format!("[dir]  {}/", name.to_string_lossy()));
        } else {
            let size = entry.metadata().map(|m| m.len()).unwrap_or(0);
            entries.push(format!(
                "[file] {} ({} bytes)",
                name.to_string_lossy(),
                size
            ));
        }
    }
    Ok(())
}

fn collect_recursive(
    root: &std::path::Path,
    dir: &std::path::Path,
    entries: &mut Vec<String>,
    limit: usize,
) -> std::io::Result<()> {
    let mut read_dir: Vec<_> = std::fs::read_dir(dir)?
        .filter_map(|e| e.ok())
        .collect();
    read_dir.sort_by_key(|e| e.file_name());

    for entry in read_dir {
        if entries.len() >= limit {
            entries.push(format!("... (truncated at {limit} entries)"));
            return Ok(());
        }
        let path = entry.path();
        let relative = path.strip_prefix(root).unwrap_or(&path);
        if path.is_dir() {
            entries.push(format!("[dir]  {}/", relative.display()));
            collect_recursive(root, &path, entries, limit)?;
        } else {
            let size = entry.metadata().map(|m| m.len()).unwrap_or(0);
            entries.push(format!(
                "[file] {} ({} bytes)",
                relative.display(),
                size
            ));
        }
    }
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::sync::Arc;

    fn test_context(workspace: &std::path::Path) -> ToolContext {
        ToolContext {
            session_key: "test".into(),
            workspace: workspace.to_path_buf(),
            config: Arc::new(rusty_claw_core::config::Config::default()),
            restrict_to_workspace: true,
            sandbox_mode: rusty_claw_core::config::SandboxMode::default(),
            browser_pool: None,
        }
    }

    #[tokio::test]
    async fn test_basic_listing() {
        let dir = tempfile::tempdir().unwrap();
        let ws = dir.path();
        std::fs::write(ws.join("readme.md"), "hello").unwrap();
        std::fs::create_dir(ws.join("src")).unwrap();

        let ctx = test_context(ws);
        let result = FileListTool
            .execute(json!({"path": "."}), &ctx)
            .await
            .unwrap();
        assert!(!result.is_error);
        assert!(result.content.contains("readme.md"));
        assert!(result.content.contains("[dir]"));
        assert!(result.content.contains("src/"));
    }

    #[tokio::test]
    async fn test_pattern_filter() {
        let dir = tempfile::tempdir().unwrap();
        let ws = dir.path();
        std::fs::write(ws.join("lib.rs"), "fn main() {}").unwrap();
        std::fs::write(ws.join("readme.md"), "hello").unwrap();

        let ctx = test_context(ws);
        let result = FileListTool
            .execute(json!({"path": ".", "pattern": "*.rs"}), &ctx)
            .await
            .unwrap();
        assert!(!result.is_error);
        assert!(result.content.contains("lib.rs"));
        assert!(!result.content.contains("readme.md"));
    }

    #[tokio::test]
    async fn test_recursive_listing() {
        let dir = tempfile::tempdir().unwrap();
        let ws = dir.path();
        std::fs::create_dir_all(ws.join("src/nested")).unwrap();
        std::fs::write(ws.join("src/nested/deep.txt"), "deep").unwrap();
        std::fs::write(ws.join("top.txt"), "top").unwrap();

        let ctx = test_context(ws);
        let result = FileListTool
            .execute(json!({"path": ".", "recursive": true}), &ctx)
            .await
            .unwrap();
        assert!(!result.is_error);
        assert!(result.content.contains("deep.txt"));
        assert!(result.content.contains("top.txt"));
    }

    #[tokio::test]
    async fn test_workspace_restriction() {
        let dir = tempfile::tempdir().unwrap();
        let ctx = test_context(dir.path());
        let result = FileListTool
            .execute(json!({"path": "/etc"}), &ctx)
            .await
            .unwrap();
        assert!(result.is_error);
        assert!(result.content.contains("outside"));
    }
}
