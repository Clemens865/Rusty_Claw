//! File editing tool — exact string replacement.

use async_trait::async_trait;
use serde_json::json;

use crate::path_guard::validate_path;
use crate::{Tool, ToolContext, ToolOutput};

pub struct EditFileTool;

#[async_trait]
impl Tool for EditFileTool {
    fn name(&self) -> &str {
        "edit_file"
    }

    fn description(&self) -> &str {
        "Edit a file by replacing an exact text match with new text. The old_text must appear exactly once in the file."
    }

    fn parameters_schema(&self) -> serde_json::Value {
        json!({
            "type": "object",
            "properties": {
                "path": {
                    "type": "string",
                    "description": "Path to the file (relative to workspace or absolute)"
                },
                "old_text": {
                    "type": "string",
                    "description": "Exact text to find and replace (must match exactly once)"
                },
                "new_text": {
                    "type": "string",
                    "description": "Text to replace the old_text with"
                }
            },
            "required": ["path", "old_text", "new_text"]
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

        let old_text = params
            .get("old_text")
            .and_then(|v| v.as_str())
            .ok_or_else(|| anyhow::anyhow!("missing 'old_text' parameter"))?;

        let new_text = params
            .get("new_text")
            .and_then(|v| v.as_str())
            .ok_or_else(|| anyhow::anyhow!("missing 'new_text' parameter"))?;

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

        let match_count = content.matches(old_text).count();

        if match_count == 0 {
            return Ok(ToolOutput {
                content: "No match found for old_text in the file".into(),
                is_error: true,
                media: None,
            });
        }

        if match_count > 1 {
            return Ok(ToolOutput {
                content: format!(
                    "old_text matches {match_count} times — must match exactly once. \
                     Provide more surrounding context to make the match unique."
                ),
                is_error: true,
                media: None,
            });
        }

        let new_content = content.replacen(old_text, new_text, 1);

        // Atomic write
        let tmp_path = path.with_extension("edit.tmp");
        tokio::fs::write(&tmp_path, new_content.as_bytes()).await?;
        tokio::fs::rename(&tmp_path, &path).await?;

        Ok(ToolOutput {
            content: format!("Edited {}", path.display()),
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
    async fn test_edit_file() {
        let dir = tempfile::tempdir().unwrap();
        let workspace = dir.path();
        std::fs::write(workspace.join("code.rs"), "fn main() {\n    println!(\"old\");\n}").unwrap();

        let ctx = ToolContext {
            session_key: "test".into(),
            workspace: workspace.to_path_buf(),
            config: Arc::new(rusty_claw_core::config::Config::default()),
            restrict_to_workspace: true,
        };

        let result = EditFileTool
            .execute(
                json!({
                    "path": "code.rs",
                    "old_text": "println!(\"old\")",
                    "new_text": "println!(\"new\")"
                }),
                &ctx,
            )
            .await
            .unwrap();
        assert!(!result.is_error);

        let content = std::fs::read_to_string(workspace.join("code.rs")).unwrap();
        assert!(content.contains("println!(\"new\")"));
        assert!(!content.contains("println!(\"old\")"));
    }

    #[tokio::test]
    async fn test_edit_no_match() {
        let dir = tempfile::tempdir().unwrap();
        let workspace = dir.path();
        std::fs::write(workspace.join("test.txt"), "hello world").unwrap();

        let ctx = ToolContext {
            session_key: "test".into(),
            workspace: workspace.to_path_buf(),
            config: Arc::new(rusty_claw_core::config::Config::default()),
            restrict_to_workspace: true,
        };

        let result = EditFileTool
            .execute(
                json!({"path": "test.txt", "old_text": "xyz", "new_text": "abc"}),
                &ctx,
            )
            .await
            .unwrap();
        assert!(result.is_error);
        assert!(result.content.contains("No match"));
    }

    #[tokio::test]
    async fn test_edit_multiple_matches() {
        let dir = tempfile::tempdir().unwrap();
        let workspace = dir.path();
        std::fs::write(workspace.join("dup.txt"), "aaa\naaa\naaa").unwrap();

        let ctx = ToolContext {
            session_key: "test".into(),
            workspace: workspace.to_path_buf(),
            config: Arc::new(rusty_claw_core::config::Config::default()),
            restrict_to_workspace: true,
        };

        let result = EditFileTool
            .execute(
                json!({"path": "dup.txt", "old_text": "aaa", "new_text": "bbb"}),
                &ctx,
            )
            .await
            .unwrap();
        assert!(result.is_error);
        assert!(result.content.contains("3 times"));
    }
}
