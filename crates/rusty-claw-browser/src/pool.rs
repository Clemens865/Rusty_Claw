//! Browser pool — manages Chrome/Chromium instances for automation.

use std::collections::HashMap;
use std::sync::Arc;

use tokio::sync::RwLock;
use tracing::{debug, info};

use rusty_claw_core::config::BrowserConfig;

/// A pool of browser pages, lazily launched.
pub struct BrowserPool {
    config: BrowserConfig,
    pages: Arc<RwLock<HashMap<String, PageHandle>>>,
}

/// Handle to a browser page (placeholder when chromiumoxide is not available).
pub struct PageHandle {
    pub session_id: String,
    pub url: Option<String>,
    pub title: Option<String>,
}

impl BrowserPool {
    /// Create a new browser pool with the given config.
    pub fn new(config: BrowserConfig) -> Self {
        Self {
            config,
            pages: Arc::new(RwLock::new(HashMap::new())),
        }
    }

    /// Get the maximum number of concurrent pages.
    pub fn max_pages(&self) -> usize {
        self.config.max_pages
    }

    /// Navigate a page to a URL. Creates the page if it doesn't exist.
    pub async fn navigate(&self, session_id: &str, url: &str) -> anyhow::Result<PageInfo> {
        let mut pages = self.pages.write().await;

        if pages.len() >= self.config.max_pages && !pages.contains_key(session_id) {
            anyhow::bail!(
                "Maximum concurrent pages ({}) reached",
                self.config.max_pages
            );
        }

        info!(session_id, url, "Browser navigate");

        let handle = pages.entry(session_id.to_string()).or_insert_with(|| {
            PageHandle {
                session_id: session_id.to_string(),
                url: None,
                title: None,
            }
        });
        handle.url = Some(url.to_string());
        // In a real implementation, this would use chromiumoxide to navigate
        // For now, return a stub result
        Ok(PageInfo {
            title: format!("Page: {url}"),
            url: url.to_string(),
        })
    }

    /// Close a page.
    pub async fn close_page(&self, session_id: &str) {
        let mut pages = self.pages.write().await;
        if pages.remove(session_id).is_some() {
            debug!(session_id, "Browser page closed");
        }
    }

    /// Get the number of active pages.
    pub async fn active_pages(&self) -> usize {
        self.pages.read().await.len()
    }
}

/// Information about a browser page.
pub struct PageInfo {
    pub title: String,
    pub url: String,
}

#[cfg(test)]
mod tests {
    use super::*;

    fn default_config() -> BrowserConfig {
        BrowserConfig {
            chrome_path: None,
            headless: true,
            max_pages: 5,
            timeout_ms: 30_000,
        }
    }

    #[tokio::test]
    async fn test_pool_navigate() {
        let pool = BrowserPool::new(default_config());
        let info = pool.navigate("sess-1", "https://example.com").await.unwrap();
        assert_eq!(info.url, "https://example.com");
        assert_eq!(pool.active_pages().await, 1);
    }

    #[tokio::test]
    async fn test_pool_max_pages() {
        let mut config = default_config();
        config.max_pages = 2;
        let pool = BrowserPool::new(config);

        pool.navigate("s1", "https://a.com").await.unwrap();
        pool.navigate("s2", "https://b.com").await.unwrap();
        assert!(pool.navigate("s3", "https://c.com").await.is_err());
    }

    #[tokio::test]
    async fn test_pool_close_page() {
        let pool = BrowserPool::new(default_config());
        pool.navigate("s1", "https://a.com").await.unwrap();
        assert_eq!(pool.active_pages().await, 1);
        pool.close_page("s1").await;
        assert_eq!(pool.active_pages().await, 0);
    }
}
