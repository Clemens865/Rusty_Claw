//! Gateway shared state.

use std::collections::HashMap;
use std::sync::atomic::{AtomicU64, Ordering};
use std::sync::Arc;
use std::time::Instant;

use tokio::sync::{mpsc, RwLock};
use tokio_util::sync::CancellationToken;

use rusty_claw_browser::BrowserPool;
use rusty_claw_channels::ChannelRegistry;
use rusty_claw_core::config::Config;
use rusty_claw_core::pairing::PairingStore;
use rusty_claw_core::session::SessionStore;
use rusty_claw_plugins::HookRegistry;
use rusty_claw_providers::ProviderRegistry;
use rusty_claw_tools::ToolRegistry;

use crate::canvas::CanvasManager;
use crate::cron::CronScheduler;
use crate::rate_limit::RateLimiter;
use crate::skills::SkillRegistry;

/// Shared gateway state accessible from all connections and handlers.
pub struct GatewayState {
    pub config: Arc<tokio::sync::RwLock<Config>>,
    pub config_path: Option<std::path::PathBuf>,
    pub sessions: Arc<dyn SessionStore>,
    pub channels: Arc<ChannelRegistry>,
    pub tools: Arc<ToolRegistry>,
    pub providers: Arc<ProviderRegistry>,
    pub hooks: Arc<HookRegistry>,
    pub skills: Arc<RwLock<SkillRegistry>>,
    pub canvas: Arc<CanvasManager>,
    pub pairing: Arc<PairingStore>,
    pub browser: Option<Arc<BrowserPool>>,
    pub cron: Option<Arc<CronScheduler>>,
    pub rate_limiter: Option<Arc<RateLimiter>>,
    pub active_agents: RwLock<HashMap<String, CancellationToken>>,
    pub connections: RwLock<HashMap<String, ConnectionState>>,
    pub state_version: AtomicU64,
    pub health_version: AtomicU64,
    pub startup_time: Instant,
    #[cfg(feature = "metrics")]
    pub prometheus_handle: Option<metrics_exporter_prometheus::PrometheusHandle>,
}

/// Per-connection state.
pub struct ConnectionState {
    pub conn_id: String,
    pub event_tx: mpsc::UnboundedSender<String>,
    pub authenticated: bool,
    /// Voice session handle (if active).
    pub voice_session: Option<rusty_claw_media::voice_session::VoiceSessionHandle>,
    /// Sender for binary audio frames back to the client.
    pub binary_event_tx: Option<mpsc::UnboundedSender<Vec<u8>>>,
}

impl GatewayState {
    #[allow(clippy::too_many_arguments)]
    pub fn new(
        config: Arc<tokio::sync::RwLock<Config>>,
        config_path: Option<std::path::PathBuf>,
        sessions: Arc<dyn SessionStore>,
        channels: Arc<ChannelRegistry>,
        tools: Arc<ToolRegistry>,
        providers: Arc<ProviderRegistry>,
        hooks: Arc<HookRegistry>,
        skills: SkillRegistry,
        pairing: PairingStore,
        browser: Option<Arc<BrowserPool>>,
        cron: Option<Arc<CronScheduler>>,
    ) -> Self {
        // Set up rate limiter from config (read once at startup)
        let rate_limiter = {
            let config_guard = config.try_read();
            config_guard
                .ok()
                .and_then(|c| c.gateway.as_ref().and_then(|g| g.rate_limit.as_ref()).cloned())
                .map(|rl| Arc::new(RateLimiter::new(rl.max_connections_per_ip)))
        };

        Self {
            config,
            config_path,
            sessions,
            channels,
            tools,
            providers,
            hooks,
            skills: Arc::new(RwLock::new(skills)),
            canvas: Arc::new(CanvasManager::new()),
            pairing: Arc::new(pairing),
            browser,
            cron,
            rate_limiter,
            active_agents: RwLock::new(HashMap::new()),
            connections: RwLock::new(HashMap::new()),
            state_version: AtomicU64::new(1),
            health_version: AtomicU64::new(1),
            startup_time: Instant::now(),
            #[cfg(feature = "metrics")]
            prometheus_handle: None,
        }
    }

    pub fn bump_state_version(&self) -> u64 {
        self.state_version.fetch_add(1, Ordering::SeqCst) + 1
    }

    pub fn bump_health_version(&self) -> u64 {
        self.health_version.fetch_add(1, Ordering::SeqCst) + 1
    }

    /// Read a snapshot of the current config. Prefer this over holding the lock.
    pub async fn read_config(&self) -> Config {
        self.config.read().await.clone()
    }
}
