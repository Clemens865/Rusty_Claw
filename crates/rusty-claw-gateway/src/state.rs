//! Gateway shared state.

use std::collections::HashMap;
use std::sync::atomic::{AtomicU64, Ordering};
use std::sync::Arc;

use tokio::sync::{mpsc, RwLock};

use rusty_claw_channels::ChannelRegistry;
use rusty_claw_core::config::Config;
use rusty_claw_core::pairing::PairingStore;
use rusty_claw_core::session::SessionStore;
use rusty_claw_plugins::HookRegistry;
use rusty_claw_providers::ProviderRegistry;
use rusty_claw_tools::ToolRegistry;

use crate::canvas::CanvasManager;
use crate::rate_limit::RateLimiter;
use crate::skills::SkillRegistry;

/// Shared gateway state accessible from all connections and handlers.
pub struct GatewayState {
    pub config: Arc<Config>,
    pub sessions: Arc<dyn SessionStore>,
    pub channels: Arc<ChannelRegistry>,
    pub tools: Arc<ToolRegistry>,
    pub providers: Arc<ProviderRegistry>,
    pub hooks: Arc<HookRegistry>,
    pub skills: Arc<RwLock<SkillRegistry>>,
    pub canvas: Arc<CanvasManager>,
    pub pairing: Arc<PairingStore>,
    pub rate_limiter: Option<Arc<RateLimiter>>,
    pub connections: RwLock<HashMap<String, ConnectionState>>,
    pub state_version: AtomicU64,
    pub health_version: AtomicU64,
}

/// Per-connection state.
pub struct ConnectionState {
    pub conn_id: String,
    pub event_tx: mpsc::UnboundedSender<String>,
    pub authenticated: bool,
}

impl GatewayState {
    #[allow(clippy::too_many_arguments)]
    pub fn new(
        config: Arc<Config>,
        sessions: Arc<dyn SessionStore>,
        channels: Arc<ChannelRegistry>,
        tools: Arc<ToolRegistry>,
        providers: Arc<ProviderRegistry>,
        hooks: Arc<HookRegistry>,
        skills: SkillRegistry,
        pairing: PairingStore,
    ) -> Self {
        // Set up rate limiter from config
        let rate_limiter = config
            .gateway
            .as_ref()
            .and_then(|g| g.rate_limit.as_ref())
            .map(|rl| Arc::new(RateLimiter::new(rl.max_connections_per_ip)));

        Self {
            config,
            sessions,
            channels,
            tools,
            providers,
            hooks,
            skills: Arc::new(RwLock::new(skills)),
            canvas: Arc::new(CanvasManager::new()),
            pairing: Arc::new(pairing),
            rate_limiter,
            connections: RwLock::new(HashMap::new()),
            state_version: AtomicU64::new(1),
            health_version: AtomicU64::new(1),
        }
    }

    pub fn bump_state_version(&self) -> u64 {
        self.state_version.fetch_add(1, Ordering::SeqCst) + 1
    }

    pub fn bump_health_version(&self) -> u64 {
        self.health_version.fetch_add(1, Ordering::SeqCst) + 1
    }
}
