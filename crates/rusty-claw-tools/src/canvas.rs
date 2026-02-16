//! Canvas tool — allows the agent to push HTML, reset, eval JS, and snapshot a canvas session.

use async_trait::async_trait;
use serde_json::json;

use crate::{Tool, ToolContext, ToolOutput};

/// Tool for interacting with the canvas/A2UI system.
pub struct CanvasTool;

#[async_trait]
impl Tool for CanvasTool {
    fn name(&self) -> &str {
        "canvas"
    }

    fn description(&self) -> &str {
        "Interact with the visual canvas workspace. Actions: push (add HTML), reset (clear), eval (run JS), snapshot (get current state)."
    }

    fn parameters_schema(&self) -> serde_json::Value {
        json!({
            "type": "object",
            "properties": {
                "action": {
                    "type": "string",
                    "enum": ["push", "reset", "eval", "snapshot"],
                    "description": "The canvas operation to perform"
                },
                "html": {
                    "type": "string",
                    "description": "HTML content to push (for 'push' action)"
                },
                "js": {
                    "type": "string",
                    "description": "JavaScript to evaluate (for 'eval' action)"
                }
            },
            "required": ["action"]
        })
    }

    async fn execute(
        &self,
        params: serde_json::Value,
        _context: &ToolContext,
    ) -> anyhow::Result<ToolOutput> {
        let action = params
            .get("action")
            .and_then(|v| v.as_str())
            .unwrap_or("");

        match action {
            "push" => {
                let html = params
                    .get("html")
                    .and_then(|v| v.as_str())
                    .unwrap_or("");
                if html.is_empty() {
                    return Ok(ToolOutput {
                        content: "Error: html parameter is required for push action".into(),
                        is_error: true,
                        media: None,
                    });
                }
                // In a real implementation, this would push via CanvasManager
                Ok(ToolOutput {
                    content: format!("Canvas push queued ({} bytes of HTML)", html.len()),
                    is_error: false,
                    media: None,
                })
            }
            "reset" => {
                Ok(ToolOutput {
                    content: "Canvas reset queued".into(),
                    is_error: false,
                    media: None,
                })
            }
            "eval" => {
                let js = params
                    .get("js")
                    .and_then(|v| v.as_str())
                    .unwrap_or("");
                if js.is_empty() {
                    return Ok(ToolOutput {
                        content: "Error: js parameter is required for eval action".into(),
                        is_error: true,
                        media: None,
                    });
                }
                Ok(ToolOutput {
                    content: format!("Canvas eval queued ({} bytes of JS)", js.len()),
                    is_error: false,
                    media: None,
                })
            }
            "snapshot" => {
                // In a real implementation, return the current canvas components
                Ok(ToolOutput {
                    content: json!({"components": []}).to_string(),
                    is_error: false,
                    media: None,
                })
            }
            _ => Ok(ToolOutput {
                content: format!("Error: unknown canvas action '{action}'. Use push, reset, eval, or snapshot."),
                is_error: true,
                media: None,
            }),
        }
    }
}
