//! File reading tool.

use async_trait::async_trait;
use serde_json::json;

use crate::path_guard::validate_path;
use crate::{Tool, ToolContext, ToolOutput};

pub struct ReadFileTool;

#[async_trait]
impl Tool for ReadFileTool {
    fn name(&self) -> &str {
        "read_file"
    }

    fn description(&self) -> &str {
        "Read the contents of a file, optionally with line offset and limit. Returns content with line numbers."
    }

    fn parameters_schema(&self) -> serde_json::Value {
        json!({
            "type": "object",
            "properties": {
                "path": {
                    "type": "string",
                    "description": "Path to the file (relative to workspace or absolute)"
                },
                "offset": {
                    "type": "integer",
                    "description": "Line number to start from (0-indexed)"
                },
                "limit": {
                    "type": "integer",
                    "description": "Maximum number of lines to return"
                }
            },
            "required": ["path"]
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
            .ok_or_else(|| anyhow::anyhow!("missing 'path' parameter"))?;

        let offset = params.get("offset").and_then(|v| v.as_u64()).unwrap_or(0) as usize;
        let limit = params.get("limit").and_then(|v| v.as_u64()).map(|v| v as usize);

        let path = match validate_path(raw_path, &context.workspace, context.restrict_to_workspace)
        {
            Ok(p) => p,
            Err(e) => {
                return Ok(ToolOutput {
                    content: format!("Path error: {e}"),
                    is_error: true,
                    media: None,
                });
            }
        };

        if !path.exists() {
            return Ok(ToolOutput {
                content: format!("File not found: {}", path.display()),
                is_error: true,
                media: None,
            });
        }

        if !path.is_file() {
            return Ok(ToolOutput {
                content: format!("Not a file: {}", path.display()),
                is_error: true,
                media: None,
            });
        }

        let content = match tokio::fs::read_to_string(&path).await {
            Ok(c) => c,
            Err(e) => {
                return Ok(ToolOutput {
                    content: format!("Read error: {e}"),
                    is_error: true,
                    media: None,
                });
            }
        };

        let lines: Vec<&str> = content.lines().collect();
        let total = lines.len();

        let end = match limit {
            Some(lim) => (offset + lim).min(total),
            None => total,
        };

        let selected = &lines[offset.min(total)..end];

        let mut result = String::new();
        for (i, line) in selected.iter().enumerate() {
            let line_num = offset + i + 1;
            result.push_str(&format!("{line_num:>6}\t{line}\n"));
        }

        if result.is_empty() {
            result = "(empty file or offset beyond end)".into();
        }

        Ok(ToolOutput {
            content: result,
            is_error: false,
            media: None,
        })
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::sync::Arc;

    #[tokio::test]
    async fn test_read_file() {
        let dir = tempfile::tempdir().unwrap();
        let workspace = dir.path();
        std::fs::write(workspace.join("hello.txt"), "line1\nline2\nline3").unwrap();

        let ctx = ToolContext {
            session_key: "test".into(),
            workspace: workspace.to_path_buf(),
            config: Arc::new(rusty_claw_core::config::Config::default()),
            restrict_to_workspace: true,
        };

        let result = ReadFileTool
            .execute(json!({"path": "hello.txt"}), &ctx)
            .await
            .unwrap();
        assert!(!result.is_error);
        assert!(result.content.contains("line1"));
        assert!(result.content.contains("line3"));
    }

    #[tokio::test]
    async fn test_read_file_with_offset() {
        let dir = tempfile::tempdir().unwrap();
        let workspace = dir.path();
        std::fs::write(workspace.join("data.txt"), "a\nb\nc\nd\ne").unwrap();

        let ctx = ToolContext {
            session_key: "test".into(),
            workspace: workspace.to_path_buf(),
            config: Arc::new(rusty_claw_core::config::Config::default()),
            restrict_to_workspace: true,
        };

        let result = ReadFileTool
            .execute(json!({"path": "data.txt", "offset": 2, "limit": 2}), &ctx)
            .await
            .unwrap();
        assert!(!result.is_error);
        assert!(result.content.contains("c"));
        assert!(result.content.contains("d"));
        assert!(!result.content.contains("\ta\n"));
    }

    #[tokio::test]
    async fn test_read_file_not_found() {
        let dir = tempfile::tempdir().unwrap();
        let ctx = ToolContext {
            session_key: "test".into(),
            workspace: dir.path().to_path_buf(),
            config: Arc::new(rusty_claw_core::config::Config::default()),
            restrict_to_workspace: true,
        };

        let result = ReadFileTool
            .execute(json!({"path": "nope.txt"}), &ctx)
            .await
            .unwrap();
        assert!(result.is_error);
        assert!(result.content.contains("not found"));
    }
}
