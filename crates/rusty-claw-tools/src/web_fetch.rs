//! web_fetch tool — HTTP GET with content extraction.

use async_trait::async_trait;
use serde::Deserialize;
use tracing::debug;

use crate::{Tool, ToolContext, ToolOutput};

pub struct WebFetchTool;

#[derive(Deserialize)]
struct Params {
    url: String,
    #[serde(default = "default_timeout")]
    timeout_ms: u64,
    #[serde(default = "default_max_size")]
    max_size: usize,
    #[serde(default)]
    raw: bool,
}

fn default_timeout() -> u64 {
    30_000
}

fn default_max_size() -> usize {
    1_048_576 // 1 MB
}

/// Strip HTML tags for readability. Simple approach — not a full parser.
fn strip_html_tags(input: &str) -> String {
    let mut result = String::with_capacity(input.len());
    let mut in_tag = false;
    let mut in_script = false;
    let chars: Vec<char> = input.chars().collect();
    let len = chars.len();
    let mut i = 0;

    while i < len {
        if in_script {
            // Look for </script>
            if i + 8 < len {
                let slice: String = chars[i..i + 9].iter().collect();
                if slice.to_lowercase() == "</script>" {
                    in_script = false;
                    i += 9;
                    continue;
                }
            }
            i += 1;
            continue;
        }

        match chars[i] {
            '<' => {
                // Check for <script
                if i + 6 < len {
                    let slice: String = chars[i..i + 7].iter().collect();
                    if slice.to_lowercase() == "<script" {
                        in_script = true;
                        in_tag = true;
                        i += 7;
                        continue;
                    }
                }
                // Check for <style
                if i + 5 < len {
                    let slice: String = chars[i..i + 6].iter().collect();
                    if slice.to_lowercase() == "<style" {
                        in_script = true; // reuse flag to skip content
                        in_tag = true;
                        i += 6;
                        continue;
                    }
                }
                in_tag = true;
            }
            '>' if in_tag => {
                in_tag = false;
                // Add newline after block elements
                result.push(' ');
            }
            c if !in_tag => {
                result.push(c);
            }
            _ => {}
        }
        i += 1;
    }

    // Collapse whitespace
    let mut collapsed = String::with_capacity(result.len());
    let mut last_was_space = false;
    for c in result.chars() {
        if c.is_whitespace() {
            if !last_was_space {
                collapsed.push(if c == '\n' { '\n' } else { ' ' });
            }
            last_was_space = true;
        } else {
            collapsed.push(c);
            last_was_space = false;
        }
    }

    collapsed.trim().to_string()
}

#[async_trait]
impl Tool for WebFetchTool {
    fn name(&self) -> &str {
        "web_fetch"
    }

    fn description(&self) -> &str {
        "Fetch the contents of a URL via HTTP GET. Returns the page text content (HTML tags stripped by default). Use `raw: true` for the raw response body."
    }

    fn parameters_schema(&self) -> serde_json::Value {
        serde_json::json!({
            "type": "object",
            "properties": {
                "url": {
                    "type": "string",
                    "description": "The URL to fetch"
                },
                "timeout_ms": {
                    "type": "integer",
                    "description": "Request timeout in milliseconds (default: 30000)"
                },
                "max_size": {
                    "type": "integer",
                    "description": "Maximum response size in bytes (default: 1048576)"
                },
                "raw": {
                    "type": "boolean",
                    "description": "If true, return raw response body without HTML stripping"
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
        let p: Params = serde_json::from_value(params)?;

        debug!(url = %p.url, "web_fetch");

        let client = reqwest::Client::builder()
            .timeout(std::time::Duration::from_millis(p.timeout_ms))
            .build()?;

        let resp = match client.get(&p.url).send().await {
            Ok(r) => r,
            Err(e) => {
                return Ok(ToolOutput {
                    content: format!("Fetch error: {e}"),
                    is_error: true,
                    media: None,
                });
            }
        };

        let status = resp.status();
        if !status.is_success() {
            return Ok(ToolOutput {
                content: format!("HTTP {status} for {}", p.url),
                is_error: true,
                media: None,
            });
        }

        let bytes = resp.bytes().await?;
        if bytes.len() > p.max_size {
            return Ok(ToolOutput {
                content: format!(
                    "Response too large: {} bytes (max: {})",
                    bytes.len(),
                    p.max_size
                ),
                is_error: true,
                media: None,
            });
        }

        let body = String::from_utf8_lossy(&bytes).to_string();
        let content = if p.raw {
            body
        } else {
            strip_html_tags(&body)
        };

        // Truncate if still too long
        let content = if content.len() > p.max_size {
            format!("{}...\n[truncated at {} bytes]", &content[..p.max_size], p.max_size)
        } else {
            content
        };

        Ok(ToolOutput {
            content,
            is_error: false,
            media: None,
        })
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_strip_html_basic() {
        let html = "<html><body><h1>Hello</h1><p>World</p></body></html>";
        let text = strip_html_tags(html);
        assert!(text.contains("Hello"));
        assert!(text.contains("World"));
        assert!(!text.contains("<h1>"));
    }

    #[test]
    fn test_strip_html_script() {
        let html = "<p>Before</p><script>alert('xss')</script><p>After</p>";
        let text = strip_html_tags(html);
        assert!(text.contains("Before"));
        assert!(text.contains("After"));
        assert!(!text.contains("alert"));
    }

    #[test]
    fn test_strip_html_empty() {
        assert_eq!(strip_html_tags(""), "");
    }

    #[test]
    fn test_strip_html_plain_text() {
        assert_eq!(strip_html_tags("Hello world"), "Hello world");
    }
}
