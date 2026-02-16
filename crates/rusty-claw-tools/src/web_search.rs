//! web_search tool — external search API wrapper.
//!
//! Supports configurable backend (SearXNG, Brave, Google Custom Search).

use async_trait::async_trait;
use serde::{Deserialize, Serialize};
use tracing::debug;

use crate::{Tool, ToolContext, ToolOutput};

pub struct WebSearchTool;

#[derive(Deserialize)]
struct Params {
    query: String,
    #[serde(default = "default_num_results")]
    num_results: usize,
}

fn default_num_results() -> usize {
    5
}

#[derive(Debug, Serialize, Deserialize)]
struct SearchResult {
    title: String,
    url: String,
    snippet: String,
}

/// Parse SearXNG JSON results.
fn parse_searxng_results(body: &serde_json::Value, max: usize) -> Vec<SearchResult> {
    let empty = vec![];
    let results = body["results"].as_array().unwrap_or(&empty);
    results
        .iter()
        .take(max)
        .filter_map(|r| {
            Some(SearchResult {
                title: r["title"].as_str()?.to_string(),
                url: r["url"].as_str()?.to_string(),
                snippet: r["content"].as_str().unwrap_or("").to_string(),
            })
        })
        .collect()
}

/// Parse Brave Search API results.
fn parse_brave_results(body: &serde_json::Value, max: usize) -> Vec<SearchResult> {
    let empty = vec![];
    let results = body["web"]["results"].as_array().unwrap_or(&empty);
    results
        .iter()
        .take(max)
        .filter_map(|r| {
            Some(SearchResult {
                title: r["title"].as_str()?.to_string(),
                url: r["url"].as_str()?.to_string(),
                snippet: r["description"].as_str().unwrap_or("").to_string(),
            })
        })
        .collect()
}

#[async_trait]
impl Tool for WebSearchTool {
    fn name(&self) -> &str {
        "web_search"
    }

    fn description(&self) -> &str {
        "Search the web using a configured search API (SearXNG, Brave, etc.). Returns a list of results with title, URL, and snippet."
    }

    fn parameters_schema(&self) -> serde_json::Value {
        serde_json::json!({
            "type": "object",
            "properties": {
                "query": {
                    "type": "string",
                    "description": "The search query"
                },
                "num_results": {
                    "type": "integer",
                    "description": "Maximum number of results to return (default: 5)"
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
        let p: Params = serde_json::from_value(params)?;
        debug!(query = %p.query, "web_search");

        // Get search API URL from config or environment
        let search_url = context
            .config
            .tools
            .as_ref()
            .and_then(|t| t.search_api_url.clone())
            .or_else(|| std::env::var("SEARCH_API_URL").ok())
            .filter(|s| !s.is_empty());

        let search_api_key = context
            .config
            .tools
            .as_ref()
            .and_then(|t| t.search_api_key.clone())
            .or_else(|| std::env::var("SEARCH_API_KEY").ok())
            .filter(|s| !s.is_empty());

        let Some(base_url) = search_url else {
            return Ok(ToolOutput {
                content: "No search API configured. Set tools.search_api_url in config or SEARCH_API_URL environment variable. Supported: SearXNG (e.g. http://localhost:8888), Brave Search API (https://api.search.brave.com).".to_string(),
                is_error: true,
                media: None,
            });
        };

        let client = reqwest::Client::builder()
            .timeout(std::time::Duration::from_secs(15))
            .build()?;

        // Detect API type from URL
        let is_brave = base_url.contains("brave.com");

        let resp = if is_brave {
            let key = search_api_key.unwrap_or_default();
            client
                .get(format!("{base_url}/res/v1/web/search"))
                .header("X-Subscription-Token", key)
                .query(&[("q", &p.query), ("count", &p.num_results.to_string())])
                .send()
                .await
        } else {
            // SearXNG-compatible
            client
                .get(format!("{base_url}/search"))
                .query(&[
                    ("q", p.query.as_str()),
                    ("format", "json"),
                    ("engines", "google,duckduckgo"),
                ])
                .send()
                .await
        };

        let resp = match resp {
            Ok(r) => r,
            Err(e) => {
                return Ok(ToolOutput {
                    content: format!("Search API error: {e}"),
                    is_error: true,
                    media: None,
                });
            }
        };

        if !resp.status().is_success() {
            return Ok(ToolOutput {
                content: format!("Search API returned HTTP {}", resp.status()),
                is_error: true,
                media: None,
            });
        }

        let body: serde_json::Value = resp.json().await?;

        let results = if is_brave {
            parse_brave_results(&body, p.num_results)
        } else {
            parse_searxng_results(&body, p.num_results)
        };

        if results.is_empty() {
            return Ok(ToolOutput {
                content: "No search results found.".to_string(),
                is_error: false,
                media: None,
            });
        }

        let mut output = String::new();
        for (i, r) in results.iter().enumerate() {
            output.push_str(&format!(
                "{}. **{}**\n   {}\n   {}\n\n",
                i + 1,
                r.title,
                r.url,
                r.snippet
            ));
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
    fn test_parse_searxng_results() {
        let body = serde_json::json!({
            "results": [
                {"title": "Rust Lang", "url": "https://rust-lang.org", "content": "A systems programming language"},
                {"title": "Rust Book", "url": "https://doc.rust-lang.org/book/", "content": "The Rust Programming Language"}
            ]
        });
        let results = parse_searxng_results(&body, 5);
        assert_eq!(results.len(), 2);
        assert_eq!(results[0].title, "Rust Lang");
    }

    #[test]
    fn test_parse_brave_results() {
        let body = serde_json::json!({
            "web": {
                "results": [
                    {"title": "Test", "url": "https://test.com", "description": "A test result"}
                ]
            }
        });
        let results = parse_brave_results(&body, 5);
        assert_eq!(results.len(), 1);
        assert_eq!(results[0].snippet, "A test result");
    }

    #[test]
    fn test_parse_empty_results() {
        let body = serde_json::json!({"results": []});
        let results = parse_searxng_results(&body, 5);
        assert!(results.is_empty());
    }
}
