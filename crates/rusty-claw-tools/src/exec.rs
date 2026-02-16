//! Shell command execution tool.

use async_trait::async_trait;
use serde_json::json;
use tracing::warn;

use crate::{Tool, ToolContext, ToolOutput};

/// Commands or patterns that are too dangerous to execute.
const DANGEROUS_PATTERNS: &[&str] = &[
    "rm -rf /",
    "rm -rf /*",
    "mkfs",
    "dd if=",
    "shutdown",
    "reboot",
    "halt",
    "poweroff",
    "init 0",
    "init 6",
    ":(){ :|:& };:",
    "> /dev/sda",
    "chmod -R 777 /",
];

pub struct ExecTool;

impl ExecTool {
    fn is_dangerous(command: &str) -> bool {
        let lower = command.to_lowercase();
        DANGEROUS_PATTERNS
            .iter()
            .any(|pat| lower.contains(&pat.to_lowercase()))
    }
}

#[async_trait]
impl Tool for ExecTool {
    fn name(&self) -> &str {
        "exec"
    }

    fn description(&self) -> &str {
        "Execute a shell command in the workspace directory. Returns stdout and stderr."
    }

    fn parameters_schema(&self) -> serde_json::Value {
        json!({
            "type": "object",
            "properties": {
                "command": {
                    "type": "string",
                    "description": "The shell command to execute"
                },
                "timeout_ms": {
                    "type": "integer",
                    "description": "Timeout in milliseconds (default: 30000)"
                }
            },
            "required": ["command"]
        })
    }

    async fn execute(
        &self,
        params: serde_json::Value,
        context: &ToolContext,
    ) -> anyhow::Result<ToolOutput> {
        let command = params
            .get("command")
            .and_then(|v| v.as_str())
            .ok_or_else(|| anyhow::anyhow!("missing 'command' parameter"))?;

        let timeout_ms = params
            .get("timeout_ms")
            .and_then(|v| v.as_u64())
            .unwrap_or(30_000);

        // Check for dangerous commands
        if Self::is_dangerous(command) {
            warn!(command, "Blocked dangerous command");
            return Ok(ToolOutput {
                content: format!("Command blocked: '{command}' matches a dangerous pattern"),
                is_error: true,
                media: None,
            });
        }

        let result = tokio::time::timeout(
            std::time::Duration::from_millis(timeout_ms),
            tokio::process::Command::new("sh")
                .arg("-c")
                .arg(command)
                .current_dir(&context.workspace)
                .output(),
        )
        .await;

        match result {
            Ok(Ok(output)) => {
                let stdout = String::from_utf8_lossy(&output.stdout);
                let stderr = String::from_utf8_lossy(&output.stderr);
                let exit_code = output.status.code().unwrap_or(-1);

                let content = if stderr.is_empty() {
                    format!("Exit code: {exit_code}\n{stdout}")
                } else {
                    format!("Exit code: {exit_code}\nstdout:\n{stdout}\nstderr:\n{stderr}")
                };

                // Truncate very long output
                let content = if content.len() > 100_000 {
                    format!(
                        "{}...\n[output truncated at 100KB]",
                        &content[..100_000]
                    )
                } else {
                    content
                };

                Ok(ToolOutput {
                    content,
                    is_error: !output.status.success(),
                    media: None,
                })
            }
            Ok(Err(e)) => Ok(ToolOutput {
                content: format!("Failed to execute command: {e}"),
                is_error: true,
                media: None,
            }),
            Err(_) => Ok(ToolOutput {
                content: format!("Command timed out after {timeout_ms}ms"),
                is_error: true,
                media: None,
            }),
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::sync::Arc;

    fn test_context() -> ToolContext {
        ToolContext {
            session_key: "test".into(),
            workspace: std::env::temp_dir(),
            config: Arc::new(rusty_claw_core::config::Config::default()),
            restrict_to_workspace: false,
        }
    }

    #[test]
    fn test_dangerous_command_detection() {
        assert!(ExecTool::is_dangerous("rm -rf /"));
        assert!(ExecTool::is_dangerous("sudo rm -rf /home"));
        assert!(ExecTool::is_dangerous("dd if=/dev/zero of=/dev/sda"));
        assert!(!ExecTool::is_dangerous("ls -la"));
        assert!(!ExecTool::is_dangerous("echo hello"));
    }

    #[tokio::test]
    async fn test_exec_echo() {
        let ctx = test_context();
        let result = ExecTool
            .execute(json!({"command": "echo hello"}), &ctx)
            .await
            .unwrap();
        assert!(!result.is_error);
        assert!(result.content.contains("hello"));
    }

    #[tokio::test]
    async fn test_exec_dangerous_blocked() {
        let ctx = test_context();
        let result = ExecTool
            .execute(json!({"command": "rm -rf /"}), &ctx)
            .await
            .unwrap();
        assert!(result.is_error);
        assert!(result.content.contains("blocked"));
    }

    #[tokio::test]
    async fn test_exec_timeout() {
        let ctx = test_context();
        let result = ExecTool
            .execute(
                json!({"command": "sleep 10", "timeout_ms": 100}),
                &ctx,
            )
            .await
            .unwrap();
        assert!(result.is_error);
        assert!(result.content.contains("timed out"));
    }
}
