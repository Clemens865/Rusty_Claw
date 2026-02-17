//! Axum-based WebSocket server with rate limiting and optional TLS.

use std::net::SocketAddr;
use std::sync::Arc;

use axum::{
    extract::{ConnectInfo, State, WebSocketUpgrade},
    response::IntoResponse,
    routing::get,
    Router,
};
use serde_json::json;
use tracing::info;

use crate::canvas::canvas_ws_handler;
use crate::connection::handle_ws_connection;
use crate::state::GatewayState;

/// Start the gateway WebSocket server.
///
/// When `ui_enabled` is true, the embedded Control UI is served at `/`.
pub async fn start_gateway(
    state: Arc<GatewayState>,
    port: u16,
    ui_enabled: bool,
) -> anyhow::Result<()> {
    let config = state.read_config().await;
    let bind_addr = config
        .gateway
        .as_ref()
        .and_then(|g| g.bind.clone())
        .unwrap_or_else(|| "0.0.0.0".to_string());

    // /ws and /health are registered first so they take priority over the UI catch-all
    let app = Router::new()
        .route("/ws", get(ws_handler))
        .route("/health", get(health_handler))
        .route("/canvas/{session_id}", get(canvas_ws_handler));

    #[cfg(feature = "metrics")]
    let app = app.route("/metrics", get(metrics_handler));

    let mut app = app.with_state(state.clone());

    if ui_enabled {
        app = app.merge(rusty_claw_web::ui_router());
        info!("Control UI available at http://{bind_addr}:{port}/");
    }

    let addr = format!("{bind_addr}:{port}");

    // Check for TLS config
    #[cfg(feature = "tls")]
    if let Some(tls_config) = config.gateway.as_ref().and_then(|g| g.tls.as_ref()) {
        use axum_server::tls_rustls::RustlsConfig;

        let tls = RustlsConfig::from_pem_file(&tls_config.cert_path, &tls_config.key_path)
            .await?;

        info!("Gateway listening on {addr} (TLS enabled)");
        let socket_addr: SocketAddr = addr.parse()?;
        axum_server::bind_rustls(socket_addr, tls)
            .serve(app.into_make_service_with_connect_info::<SocketAddr>())
            .await?;

        return Ok(());
    }

    let listener = tokio::net::TcpListener::bind(&addr).await?;
    info!("Gateway listening on {addr}");

    axum::serve(
        listener,
        app.into_make_service_with_connect_info::<SocketAddr>(),
    )
    .with_graceful_shutdown(shutdown_signal(state.clone()))
    .await?;

    Ok(())
}

async fn ws_handler(
    ws: WebSocketUpgrade,
    ConnectInfo(addr): ConnectInfo<SocketAddr>,
    State(state): State<Arc<GatewayState>>,
) -> impl IntoResponse {
    // Rate limiting check
    if let Some(limiter) = &state.rate_limiter {
        if !limiter.check(addr.ip()) {
            return axum::http::StatusCode::TOO_MANY_REQUESTS.into_response();
        }
    }

    ws.on_upgrade(move |socket| handle_ws_connection(state, socket))
        .into_response()
}

async fn health_handler(State(state): State<Arc<GatewayState>>) -> impl IntoResponse {
    let version = env!("CARGO_PKG_VERSION");
    let connections = state.connections.read().await.len();
    let uptime_seconds = state.startup_time.elapsed().as_secs();
    let active_agents = state.active_agents.read().await.len();

    // Provider reachability (lightweight: just check if they exist)
    let providers: Vec<serde_json::Value> = state
        .providers
        .list_ids()
        .iter()
        .map(|id| {
            json!({
                "id": id,
                "reachable": state.providers.get(id).is_some(),
            })
        })
        .collect();

    // Channel status
    let mut channels = Vec::new();
    for ch_id in state.channels.list() {
        if let Some(ch) = state.channels.get(ch_id) {
            let status = ch.status().await;
            channels.push(json!({
                "id": ch_id,
                "connected": status.connected,
            }));
        }
    }

    axum::Json(json!({
        "status": "ok",
        "version": version,
        "connections": connections,
        "uptime_seconds": uptime_seconds,
        "active_agents": active_agents,
        "providers": providers,
        "channels": channels,
    }))
}

#[cfg(feature = "metrics")]
async fn metrics_handler(State(state): State<Arc<GatewayState>>) -> impl IntoResponse {
    match &state.prometheus_handle {
        Some(handle) => handle.render().into_response(),
        None => "# Metrics not initialized\n".into_response(),
    }
}

async fn shutdown_signal(state: Arc<GatewayState>) {
    let ctrl_c = tokio::signal::ctrl_c();

    #[cfg(unix)]
    {
        let mut sigterm =
            tokio::signal::unix::signal(tokio::signal::unix::SignalKind::terminate())
                .expect("Failed to install SIGTERM handler");
        tokio::select! {
            _ = ctrl_c => {},
            _ = sigterm.recv() => {},
        }
    }

    #[cfg(not(unix))]
    {
        ctrl_c.await.expect("Failed to install CTRL+C handler");
    }

    info!("Shutdown signal received, draining connections...");
    graceful_drain(&state).await;
}

/// Cancel active agents and wait for connections to drain (up to 30s).
async fn graceful_drain(state: &Arc<GatewayState>) {
    // Cancel all active agents
    {
        let agents = state.active_agents.read().await;
        let count = agents.len();
        for token in agents.values() {
            token.cancel();
        }
        if count > 0 {
            info!(count, "Cancelled active agents");
        }
    }

    // Wait for connections to drain (poll every 500ms, max 30s)
    let deadline = tokio::time::Instant::now() + std::time::Duration::from_secs(30);
    loop {
        let conn_count = state.connections.read().await.len();
        if conn_count == 0 {
            break;
        }
        if tokio::time::Instant::now() >= deadline {
            info!(remaining = conn_count, "Drain timeout, forcing shutdown");
            break;
        }
        tokio::time::sleep(std::time::Duration::from_millis(500)).await;
    }

    let uptime = state.startup_time.elapsed();
    info!(uptime_secs = uptime.as_secs(), "Gateway shutting down");
}

#[cfg(test)]
mod tests {
    use super::*;

    /// Helper to build a minimal GatewayState for testing.
    async fn test_state() -> Arc<GatewayState> {
        let config = rusty_claw_core::config::Config::default();
        let config_rw = Arc::new(tokio::sync::RwLock::new(config));

        let sessions: Arc<dyn rusty_claw_core::session::SessionStore> = Arc::new(
            rusty_claw_core::session_store::JsonlSessionStore::new(
                std::env::temp_dir().join(format!(
                    "rusty-claw-test-drain-{}",
                    std::process::id()
                )),
            ),
        );

        let channels = Arc::new(rusty_claw_channels::ChannelRegistry::new());
        let mut tools = rusty_claw_tools::ToolRegistry::new();
        rusty_claw_tools::register_builtin_tools(&mut tools);
        let tools = Arc::new(tools);

        let providers = Arc::new(rusty_claw_providers::ProviderRegistry::new(
            "none".into(),
        ));

        let hooks = Arc::new(rusty_claw_plugins::HookRegistry::new());
        let skills = crate::skills::SkillRegistry::new();
        let pairing = rusty_claw_core::pairing::PairingStore::new(
            std::env::temp_dir().join(format!(
                "rusty-claw-pairing-drain-{}",
                std::process::id()
            )),
        );

        Arc::new(GatewayState::new(
            config_rw,
            None,
            sessions,
            channels,
            tools,
            providers,
            hooks,
            skills,
            pairing,
            None,
            None,
        ))
    }

    #[tokio::test]
    async fn test_graceful_drain_cancels_agents() {
        let state = test_state().await;

        // Register an active agent with a CancellationToken
        let token = tokio_util::sync::CancellationToken::new();
        let token_clone = token.clone();
        {
            let mut agents = state.active_agents.write().await;
            agents.insert("test-agent-1".into(), token_clone);
        }

        // The token should not be cancelled yet
        assert!(!token.is_cancelled());

        // Call graceful_drain — it should cancel the agent token
        graceful_drain(&state).await;

        // Verify the token has been cancelled
        assert!(token.is_cancelled());
    }

    #[tokio::test]
    async fn test_graceful_drain_timeout() {
        let state = test_state().await;
        // No active agents, no connections — drain should complete quickly

        let start = std::time::Instant::now();
        graceful_drain(&state).await;
        let elapsed = start.elapsed();

        // With no agents and no connections, drain should complete well under 30s
        assert!(
            elapsed.as_secs() < 5,
            "Drain took too long with no agents: {elapsed:?}"
        );
    }
}
