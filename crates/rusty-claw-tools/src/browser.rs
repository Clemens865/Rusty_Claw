//! Browser automation tools for the agent.
//!
//! Provides navigate, screenshot, click, extract_text, evaluate_js, and wait_for.
//! These are stub implementations that return descriptive messages when the
//! browser feature is not enabled or Chrome is not available.

use async_trait::async_trait;
use serde_json::json;

use crate::{Tool, ToolContext, ToolOutput};

/// Navigate to a URL and return page info.
pub struct BrowserNavigateTool;

#[async_trait]
impl Tool for BrowserNavigateTool {
    fn name(&self) -> &str {
        "browser_navigate"
    }

    fn description(&self) -> &str {
        "Navigate a browser page to a URL and return the page title and metadata."
    }

    fn parameters_schema(&self) -> serde_json::Value {
        json!({
            "type": "object",
            "properties": {
                "url": {
                    "type": "string",
                    "description": "The URL to navigate to"
                }
            },
            "required": ["url"]
        })
    }

    async fn execute(
        &self,
        params: serde_json::Value,
        _context: &ToolContext,
    ) -> anyhow::Result<ToolOutput> {
        let url = params
            .get("url")
            .and_then(|v| v.as_str())
            .unwrap_or("");

        if url.is_empty() {
            return Ok(ToolOutput {
                content: "Error: url parameter is required".into(),
                is_error: true,
                media: None,
            });
        }

        // Stub: in a real implementation, this would use BrowserPool
        Ok(ToolOutput {
            content: format!("Navigated to: {url}\nTitle: (browser not available — install Chrome and enable browser feature)"),
            is_error: false,
            media: None,
        })
    }
}

/// Take a screenshot of the current page.
pub struct BrowserScreenshotTool;

#[async_trait]
impl Tool for BrowserScreenshotTool {
    fn name(&self) -> &str {
        "browser_screenshot"
    }

    fn description(&self) -> &str {
        "Take a screenshot of the current browser page and return it as an image."
    }

    fn parameters_schema(&self) -> serde_json::Value {
        json!({
            "type": "object",
            "properties": {
                "selector": {
                    "type": "string",
                    "description": "Optional CSS selector to screenshot a specific element"
                }
            }
        })
    }

    async fn execute(
        &self,
        _params: serde_json::Value,
        _context: &ToolContext,
    ) -> anyhow::Result<ToolOutput> {
        Ok(ToolOutput {
            content: "Screenshot not available — browser feature not enabled".into(),
            is_error: true,
            media: None,
        })
    }
}

/// Click an element on the page by CSS selector.
pub struct BrowserClickTool;

#[async_trait]
impl Tool for BrowserClickTool {
    fn name(&self) -> &str {
        "browser_click"
    }

    fn description(&self) -> &str {
        "Click an element on the page identified by a CSS selector."
    }

    fn parameters_schema(&self) -> serde_json::Value {
        json!({
            "type": "object",
            "properties": {
                "selector": {
                    "type": "string",
                    "description": "CSS selector of the element to click"
                }
            },
            "required": ["selector"]
        })
    }

    async fn execute(
        &self,
        params: serde_json::Value,
        _context: &ToolContext,
    ) -> anyhow::Result<ToolOutput> {
        let selector = params
            .get("selector")
            .and_then(|v| v.as_str())
            .unwrap_or("");

        if selector.is_empty() {
            return Ok(ToolOutput {
                content: "Error: selector parameter is required".into(),
                is_error: true,
                media: None,
            });
        }

        Ok(ToolOutput {
            content: format!("Click not available — browser feature not enabled (selector: {selector})"),
            is_error: true,
            media: None,
        })
    }
}

/// Extract text from the page or a specific element.
pub struct BrowserExtractTextTool;

#[async_trait]
impl Tool for BrowserExtractTextTool {
    fn name(&self) -> &str {
        "browser_extract_text"
    }

    fn description(&self) -> &str {
        "Extract text content from the page or a specific element by CSS selector."
    }

    fn parameters_schema(&self) -> serde_json::Value {
        json!({
            "type": "object",
            "properties": {
                "selector": {
                    "type": "string",
                    "description": "Optional CSS selector. If omitted, extracts full page text."
                }
            }
        })
    }

    async fn execute(
        &self,
        _params: serde_json::Value,
        _context: &ToolContext,
    ) -> anyhow::Result<ToolOutput> {
        Ok(ToolOutput {
            content: "Text extraction not available — browser feature not enabled".into(),
            is_error: true,
            media: None,
        })
    }
}

/// Evaluate JavaScript on the current page.
pub struct BrowserEvaluateJsTool;

#[async_trait]
impl Tool for BrowserEvaluateJsTool {
    fn name(&self) -> &str {
        "browser_evaluate_js"
    }

    fn description(&self) -> &str {
        "Evaluate a JavaScript expression in the browser page context and return the result."
    }

    fn parameters_schema(&self) -> serde_json::Value {
        json!({
            "type": "object",
            "properties": {
                "expression": {
                    "type": "string",
                    "description": "JavaScript expression to evaluate"
                }
            },
            "required": ["expression"]
        })
    }

    async fn execute(
        &self,
        params: serde_json::Value,
        _context: &ToolContext,
    ) -> anyhow::Result<ToolOutput> {
        let expression = params
            .get("expression")
            .and_then(|v| v.as_str())
            .unwrap_or("");

        if expression.is_empty() {
            return Ok(ToolOutput {
                content: "Error: expression parameter is required".into(),
                is_error: true,
                media: None,
            });
        }

        Ok(ToolOutput {
            content: "JS evaluation not available — browser feature not enabled".into(),
            is_error: true,
            media: None,
        })
    }
}

/// Wait for an element to appear on the page.
pub struct BrowserWaitForTool;

#[async_trait]
impl Tool for BrowserWaitForTool {
    fn name(&self) -> &str {
        "browser_wait_for"
    }

    fn description(&self) -> &str {
        "Wait for a CSS selector to appear on the page, with a configurable timeout."
    }

    fn parameters_schema(&self) -> serde_json::Value {
        json!({
            "type": "object",
            "properties": {
                "selector": {
                    "type": "string",
                    "description": "CSS selector to wait for"
                },
                "timeout_ms": {
                    "type": "integer",
                    "description": "Timeout in milliseconds (default: 5000)"
                }
            },
            "required": ["selector"]
        })
    }

    async fn execute(
        &self,
        params: serde_json::Value,
        _context: &ToolContext,
    ) -> anyhow::Result<ToolOutput> {
        let selector = params
            .get("selector")
            .and_then(|v| v.as_str())
            .unwrap_or("");

        if selector.is_empty() {
            return Ok(ToolOutput {
                content: "Error: selector parameter is required".into(),
                is_error: true,
                media: None,
            });
        }

        Ok(ToolOutput {
            content: format!("Wait not available — browser feature not enabled (selector: {selector})"),
            is_error: true,
            media: None,
        })
    }
}
