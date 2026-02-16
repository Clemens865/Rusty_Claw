//! File writing tool with atomic writes.

use async_trait::async_trait;
use serde_json::json;

use crate::path_guard::validate_write_path;
use crate::{Tool, ToolContext, ToolOutput};

pub struct WriteFileTool;

#[async_trait]
impl Tool for WriteFileTool {
    fn name(&self) -> &str {
        "write_file"
    }

    fn description(&self) -> &str {
        "Write content to a file. Creates the file if it doesn't exist. Uses atomic writes (temp file + rename) for safety."
    }

    fn parameters_schema(&self) -> serde_json::Value {
        json!({
            "type": "object",
            "properties": {
                "path": {
                    "type": "string",
                    "description": "Path to the file (relative to workspace or absolute)"
                },
                "content": {
                    "type": "string",
                    "description": "Content to write"
                },
                "create_dirs": {
                    "type": "boolean",
                    "description": "Create parent directories if they don't exist (default: false)"
                }
            },
            "required": ["path", "content"]
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

        let content = params
            .get("content")
            .and_then(|v| v.as_str())
            .ok_or_else(|| anyhow::anyhow!("missing 'content' parameter"))?;

        let create_dirs = params
            .get("create_dirs")
            .and_then(|v| v.as_bool())
            .unwrap_or(false);

        let path = match validate_write_path(
            raw_path,
            &context.workspace,
            context.restrict_to_workspace,
            create_dirs,
        ) {
            Ok(p) => p,
            Err(e) => {
                return Ok(ToolOutput {
                    content: format!("Path error: {e}"),
                    is_error: true,
                    media: None,
                });
            }
        };

        // Atomic write: write to temp file then rename
        let tmp_path = path.with_extension("tmp");
        if let Err(e) = tokio::fs::write(&tmp_path, content.as_bytes()).await {
            return Ok(ToolOutput {
                content: format!("Write error: {e}"),
                is_error: true,
                media: None,
            });
        }

        if let Err(e) = tokio::fs::rename(&tmp_path, &path).await {
            // Clean up temp file on rename failure
            let _ = tokio::fs::remove_file(&tmp_path).await;
            return Ok(ToolOutput {
                content: format!("Rename error: {e}"),
                is_error: true,
                media: None,
            });
        }

        let bytes = content.len();
        Ok(ToolOutput {
            content: format!("Wrote {bytes} bytes to {}", path.display()),
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
    async fn test_write_file() {
        let dir = tempfile::tempdir().unwrap();
        let workspace = dir.path();

        let ctx = ToolContext {
            session_key: "test".into(),
            workspace: workspace.to_path_buf(),
            config: Arc::new(rusty_claw_core::config::Config::default()),
            restrict_to_workspace: true,
            sandbox_mode: rusty_claw_core::config::SandboxMode::default(),
            browser_pool: None,
        };

        let result = WriteFileTool
            .execute(
                json!({"path": "out.txt", "content": "hello world"}),
                &ctx,
            )
            .await
            .unwrap();
        assert!(!result.is_error);

        let written = std::fs::read_to_string(workspace.join("out.txt")).unwrap();
        assert_eq!(written, "hello world");
    }

    #[tokio::test]
    async fn test_write_file_create_dirs() {
        let dir = tempfile::tempdir().unwrap();
        let workspace = dir.path();

        let ctx = ToolContext {
            session_key: "test".into(),
            workspace: workspace.to_path_buf(),
            config: Arc::new(rusty_claw_core::config::Config::default()),
            restrict_to_workspace: true,
            sandbox_mode: rusty_claw_core::config::SandboxMode::default(),
            browser_pool: None,
        };

        let result = WriteFileTool
            .execute(
                json!({"path": "sub/dir/file.txt", "content": "nested", "create_dirs": true}),
                &ctx,
            )
            .await
            .unwrap();
        assert!(!result.is_error);

        let written = std::fs::read_to_string(workspace.join("sub/dir/file.txt")).unwrap();
        assert_eq!(written, "nested");
    }
}
