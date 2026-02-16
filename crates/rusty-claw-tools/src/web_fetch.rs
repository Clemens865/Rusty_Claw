//! web_fetch tool — HTTP GET with content extraction and SSRF protection.

use std::net::IpAddr;

use async_trait::async_trait;
use serde::Deserialize;
use tracing::{debug, warn};

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

/// Validate a URL against SSRF attacks.
/// Blocks private IPs, localhost, cloud metadata endpoints, and non-HTTP schemes.
async fn validate_url(url: &str) -> Result<(), String> {
    // Parse the URL
    let parsed = url::Url::parse(url).map_err(|e| format!("Invalid URL: {e}"))?;

    // Only allow http:// and https://
    match parsed.scheme() {
        "http" | "https" => {}
        scheme => return Err(format!("Blocked scheme: {scheme}:// (only http/https allowed)")),
    }

    // Extract hostname
    let host = parsed
        .host_str()
        .ok_or("URL has no host")?;

    // Block localhost explicitly
    if host == "localhost" || host == "127.0.0.1" || host == "::1" || host == "[::1]" || host == "0.0.0.0" {
        return Err(format!("Blocked: requests to {host} are not allowed"));
    }

    // Block cloud metadata endpoints
    if host == "169.254.169.254" || host == "metadata.google.internal" {
        return Err(format!("Blocked: cloud metadata endpoint {host}"));
    }

    // Resolve hostname and check for private IPs
    let port = parsed.port().unwrap_or(if parsed.scheme() == "https" { 443 } else { 80 });
    let addr_str = format!("{host}:{port}");

    match tokio::net::lookup_host(&addr_str).await {
        Ok(addrs) => {
            for addr in addrs {
                if is_private_ip(addr.ip()) {
                    return Err(format!(
                        "Blocked: {host} resolves to private IP {}",
                        addr.ip()
                    ));
                }
            }
        }
        Err(e) => {
            // DNS resolution failure — allow the request to proceed,
            // reqwest will handle the error
            debug!(host, %e, "DNS lookup failed during SSRF check, allowing request");
        }
    }

    Ok(())
}

/// Check if an IP address is private/reserved.
fn is_private_ip(ip: IpAddr) -> bool {
    match ip {
        IpAddr::V4(v4) => {
            v4.is_loopback()              // 127.0.0.0/8
                || v4.is_private()         // 10.0.0.0/8, 172.16.0.0/12, 192.168.0.0/16
                || v4.is_link_local()      // 169.254.0.0/16
                || v4.is_unspecified()     // 0.0.0.0
                || v4.octets()[0] == 100 && v4.octets()[1] >= 64 && v4.octets()[1] <= 127 // 100.64.0.0/10 (CGNAT)
        }
        IpAddr::V6(v6) => {
            v6.is_loopback()               // ::1
                || v6.is_unspecified()      // ::
                || {
                    let segments = v6.segments();
                    // fc00::/7 (unique local)
                    (segments[0] & 0xfe00) == 0xfc00
                    // fe80::/10 (link-local)
                    || (segments[0] & 0xffc0) == 0xfe80
                }
        }
    }
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

        // SSRF protection: validate URL before making request
        if let Err(reason) = validate_url(&p.url).await {
            warn!(url = %p.url, %reason, "SSRF protection blocked request");
            return Ok(ToolOutput {
                content: format!("Request blocked: {reason}"),
                is_error: true,
                media: None,
            });
        }

        let client = reqwest::Client::builder()
            .timeout(std::time::Duration::from_millis(p.timeout_ms))
            .redirect(reqwest::redirect::Policy::limited(5))
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

    #[test]
    fn test_is_private_ipv4() {
        use std::net::Ipv4Addr;
        assert!(is_private_ip(IpAddr::V4(Ipv4Addr::new(127, 0, 0, 1))));
        assert!(is_private_ip(IpAddr::V4(Ipv4Addr::new(10, 0, 0, 1))));
        assert!(is_private_ip(IpAddr::V4(Ipv4Addr::new(172, 16, 0, 1))));
        assert!(is_private_ip(IpAddr::V4(Ipv4Addr::new(192, 168, 1, 1))));
        assert!(is_private_ip(IpAddr::V4(Ipv4Addr::new(169, 254, 169, 254))));
        assert!(!is_private_ip(IpAddr::V4(Ipv4Addr::new(8, 8, 8, 8))));
        assert!(!is_private_ip(IpAddr::V4(Ipv4Addr::new(1, 1, 1, 1))));
    }

    #[test]
    fn test_is_private_ipv6() {
        use std::net::Ipv6Addr;
        assert!(is_private_ip(IpAddr::V6(Ipv6Addr::LOCALHOST)));
        assert!(is_private_ip(IpAddr::V6(Ipv6Addr::UNSPECIFIED)));
    }

    #[tokio::test]
    async fn test_ssrf_block_localhost() {
        assert!(validate_url("http://localhost/secret").await.is_err());
        assert!(validate_url("http://127.0.0.1/secret").await.is_err());
        assert!(validate_url("http://[::1]/secret").await.is_err());
    }

    #[tokio::test]
    async fn test_ssrf_block_scheme() {
        assert!(validate_url("file:///etc/passwd").await.is_err());
        assert!(validate_url("ftp://example.com").await.is_err());
        assert!(validate_url("gopher://example.com").await.is_err());
    }

    #[tokio::test]
    async fn test_ssrf_block_metadata() {
        assert!(validate_url("http://169.254.169.254/latest/meta-data/").await.is_err());
    }

    #[tokio::test]
    async fn test_ssrf_allow_public() {
        // Public URLs should pass validation
        assert!(validate_url("https://example.com").await.is_ok());
        assert!(validate_url("http://example.com/page").await.is_ok());
    }
}
