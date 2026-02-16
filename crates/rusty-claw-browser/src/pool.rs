//! Browser pool — manages Chrome/Chromium instances for automation.

use std::collections::HashMap;
use std::sync::Arc;

use tokio::sync::RwLock;
#[cfg(feature = "browser")]
use tracing::{debug, info, warn};
#[cfg(not(feature = "browser"))]
use tracing::{debug, info};

use rusty_claw_core::config::BrowserConfig;

/// Information about a browser page.
#[derive(Debug, Clone)]
pub struct PageInfo {
    pub title: String,
    pub url: String,
}

// ============================================================
// Real chromiumoxide implementation
// ============================================================
#[cfg(feature = "browser")]
mod real {
    use super::*;
    use chromiumoxide::browser::{Browser, BrowserConfig as CdpConfig};
    use chromiumoxide::page::ScreenshotParams;
    use chromiumoxide::Page;
    use futures::StreamExt;
    use std::time::Duration;

    /// A pool of browser pages backed by real Chrome DevTools Protocol.
    pub struct BrowserPool {
        config: BrowserConfig,
        browser: RwLock<Option<Browser>>,
        pages: Arc<RwLock<HashMap<String, Page>>>,
    }

    impl BrowserPool {
        pub fn new(config: BrowserConfig) -> Self {
            Self {
                config,
                browser: RwLock::new(None),
                pages: Arc::new(RwLock::new(HashMap::new())),
            }
        }

        pub fn max_pages(&self) -> usize {
            self.config.max_pages
        }

        /// Lazily launch Chrome if not already running.
        async fn ensure_browser(&self) -> anyhow::Result<()> {
            let mut browser_guard = self.browser.write().await;
            if browser_guard.is_some() {
                return Ok(());
            }

            info!("Launching headless Chrome via CDP");

            let mut builder = CdpConfig::builder();
            if self.config.headless {
                builder = builder.arg("--headless=new");
            }
            builder = builder
                .arg("--disable-gpu")
                .arg("--no-sandbox")
                .arg("--disable-dev-shm-usage");

            if let Some(ref path) = self.config.chrome_path {
                builder = builder.chrome_executable(path);
            }

            let cdp_config = builder
                .build()
                .map_err(|e| anyhow::anyhow!("Failed to build CDP config: {e}"))?;

            let (browser, mut handler) = Browser::launch(cdp_config)
                .await
                .map_err(|e| anyhow::anyhow!("Failed to launch Chrome: {e}"))?;

            // Spawn the CDP handler
            tokio::spawn(async move {
                while let Some(event) = handler.next().await {
                    if event.is_err() {
                        warn!("CDP handler error");
                        break;
                    }
                }
                debug!("CDP handler finished");
            });

            *browser_guard = Some(browser);
            Ok(())
        }

        fn timeout_duration(&self) -> Duration {
            Duration::from_millis(self.config.timeout_ms)
        }

        pub async fn navigate(&self, session_id: &str, url: &str) -> anyhow::Result<PageInfo> {
            self.ensure_browser().await?;

            let mut pages = self.pages.write().await;

            if pages.len() >= self.config.max_pages && !pages.contains_key(session_id) {
                anyhow::bail!(
                    "Maximum concurrent pages ({}) reached",
                    self.config.max_pages
                );
            }

            info!(session_id, url, "Browser navigate");

            let browser_guard = self.browser.read().await;
            let browser = browser_guard
                .as_ref()
                .ok_or_else(|| anyhow::anyhow!("Browser not launched"))?;

            if let Some(existing) = pages.get(session_id) {
                existing
                    .goto(url)
                    .await
                    .map_err(|e| anyhow::anyhow!("Navigation failed: {e}"))?;

                let title = existing.get_title().await.unwrap_or_default().unwrap_or_default();
                let current_url = existing
                    .url()
                    .await
                    .ok()
                    .flatten()
                    .map(|u| u.to_string())
                    .unwrap_or_else(|| url.to_string());

                Ok(PageInfo {
                    title,
                    url: current_url,
                })
            } else {
                let new_page = tokio::time::timeout(self.timeout_duration(), browser.new_page(url))
                    .await
                    .map_err(|_| anyhow::anyhow!("Page creation timed out"))?
                    .map_err(|e| anyhow::anyhow!("Failed to create page: {e}"))?;

                let title = new_page.get_title().await.unwrap_or_default().unwrap_or_default();
                let current_url = new_page
                    .url()
                    .await
                    .ok()
                    .flatten()
                    .map(|u| u.to_string())
                    .unwrap_or_else(|| url.to_string());

                pages.insert(session_id.to_string(), new_page);

                Ok(PageInfo {
                    title,
                    url: current_url,
                })
            }
        }

        pub async fn screenshot(&self, session_id: &str) -> anyhow::Result<Vec<u8>> {
            let pages = self.pages.read().await;
            let page = pages
                .get(session_id)
                .ok_or_else(|| anyhow::anyhow!("No page open for session {session_id}"))?;

            let bytes = tokio::time::timeout(
                self.timeout_duration(),
                page.screenshot(ScreenshotParams::builder().build()),
            )
            .await
            .map_err(|_| anyhow::anyhow!("Screenshot timed out"))?
            .map_err(|e| anyhow::anyhow!("Screenshot failed: {e}"))?;

            Ok(bytes)
        }

        pub async fn click(&self, session_id: &str, selector: &str) -> anyhow::Result<()> {
            let pages = self.pages.read().await;
            let page = pages
                .get(session_id)
                .ok_or_else(|| anyhow::anyhow!("No page open for session {session_id}"))?;

            let element = tokio::time::timeout(
                self.timeout_duration(),
                page.find_element(selector),
            )
            .await
            .map_err(|_| anyhow::anyhow!("Find element timed out"))?
            .map_err(|e| anyhow::anyhow!("Element not found '{selector}': {e}"))?;

            element
                .click()
                .await
                .map_err(|e| anyhow::anyhow!("Click failed: {e}"))?;

            Ok(())
        }

        pub async fn extract_text(
            &self,
            session_id: &str,
            selector: Option<&str>,
        ) -> anyhow::Result<String> {
            let pages = self.pages.read().await;
            let page = pages
                .get(session_id)
                .ok_or_else(|| anyhow::anyhow!("No page open for session {session_id}"))?;

            let text = if let Some(sel) = selector {
                let element = tokio::time::timeout(
                    self.timeout_duration(),
                    page.find_element(sel),
                )
                .await
                .map_err(|_| anyhow::anyhow!("Find element timed out"))?
                .map_err(|e| anyhow::anyhow!("Element not found '{sel}': {e}"))?;

                element
                    .inner_text()
                    .await
                    .map_err(|e| anyhow::anyhow!("Text extraction failed: {e}"))?
                    .unwrap_or_default()
            } else {
                let result: String = page
                    .evaluate("document.body.innerText || ''")
                    .await
                    .map_err(|e| anyhow::anyhow!("Page text extraction failed: {e}"))?
                    .into_value()
                    .map_err(|e| anyhow::anyhow!("JS result conversion failed: {e:?}"))?;
                result
            };

            Ok(text)
        }

        pub async fn evaluate_js(
            &self,
            session_id: &str,
            expression: &str,
        ) -> anyhow::Result<serde_json::Value> {
            let pages = self.pages.read().await;
            let page = pages
                .get(session_id)
                .ok_or_else(|| anyhow::anyhow!("No page open for session {session_id}"))?;

            let result = tokio::time::timeout(
                self.timeout_duration(),
                page.evaluate(expression),
            )
            .await
            .map_err(|_| anyhow::anyhow!("JS evaluation timed out"))?
            .map_err(|e| anyhow::anyhow!("JS evaluation failed: {e}"))?;

            let value: serde_json::Value = result
                .into_value()
                .map_err(|e| anyhow::anyhow!("JS result conversion failed: {e:?}"))?;

            Ok(value)
        }

        pub async fn wait_for(
            &self,
            session_id: &str,
            selector: &str,
            timeout_ms: Option<u64>,
        ) -> anyhow::Result<bool> {
            let pages = self.pages.read().await;
            let page = pages
                .get(session_id)
                .ok_or_else(|| anyhow::anyhow!("No page open for session {session_id}"))?;

            let timeout =
                std::time::Duration::from_millis(timeout_ms.unwrap_or(self.config.timeout_ms));

            match tokio::time::timeout(timeout, page.find_element(selector)).await {
                Ok(Ok(_)) => Ok(true),
                Ok(Err(_)) | Err(_) => Ok(false),
            }
        }

        pub async fn close_page(&self, session_id: &str) {
            let mut pages = self.pages.write().await;
            if let Some(page) = pages.remove(session_id) {
                let _ = page.close().await;
                debug!(session_id, "Browser page closed");
            }
        }

        pub async fn active_pages(&self) -> usize {
            self.pages.read().await.len()
        }
    }
}

// ============================================================
// Stub implementation (no browser feature)
// ============================================================
#[cfg(not(feature = "browser"))]
mod stub {
    use super::*;

    /// Stub browser pool that returns errors for operations needing real Chrome.
    pub struct BrowserPool {
        config: BrowserConfig,
        pages: Arc<RwLock<HashMap<String, ()>>>,
    }

    impl BrowserPool {
        pub fn new(config: BrowserConfig) -> Self {
            Self {
                config,
                pages: Arc::new(RwLock::new(HashMap::new())),
            }
        }

        pub fn max_pages(&self) -> usize {
            self.config.max_pages
        }

        pub async fn navigate(&self, session_id: &str, url: &str) -> anyhow::Result<PageInfo> {
            let mut pages = self.pages.write().await;

            if pages.len() >= self.config.max_pages && !pages.contains_key(session_id) {
                anyhow::bail!(
                    "Maximum concurrent pages ({}) reached",
                    self.config.max_pages
                );
            }

            info!(session_id, url, "Browser navigate (stub)");
            pages.entry(session_id.to_string()).or_insert(());

            Ok(PageInfo {
                title: format!("Page: {url}"),
                url: url.to_string(),
            })
        }

        pub async fn screenshot(&self, _session_id: &str) -> anyhow::Result<Vec<u8>> {
            anyhow::bail!("Browser feature not enabled — rebuild with --features browser")
        }

        pub async fn click(&self, _session_id: &str, _selector: &str) -> anyhow::Result<()> {
            anyhow::bail!("Browser feature not enabled — rebuild with --features browser")
        }

        pub async fn extract_text(
            &self,
            _session_id: &str,
            _selector: Option<&str>,
        ) -> anyhow::Result<String> {
            anyhow::bail!("Browser feature not enabled — rebuild with --features browser")
        }

        pub async fn evaluate_js(
            &self,
            _session_id: &str,
            _expression: &str,
        ) -> anyhow::Result<serde_json::Value> {
            anyhow::bail!("Browser feature not enabled — rebuild with --features browser")
        }

        pub async fn wait_for(
            &self,
            _session_id: &str,
            _selector: &str,
            _timeout_ms: Option<u64>,
        ) -> anyhow::Result<bool> {
            anyhow::bail!("Browser feature not enabled — rebuild with --features browser")
        }

        pub async fn close_page(&self, session_id: &str) {
            let mut pages = self.pages.write().await;
            if pages.remove(session_id).is_some() {
                debug!(session_id, "Browser page closed (stub)");
            }
        }

        pub async fn active_pages(&self) -> usize {
            self.pages.read().await.len()
        }
    }
}

#[cfg(feature = "browser")]
pub use real::BrowserPool;

#[cfg(not(feature = "browser"))]
pub use stub::BrowserPool;

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
        assert!(!info.url.is_empty());
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
