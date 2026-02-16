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
    let bind_addr = state
        .config
        .gateway
        .as_ref()
        .and_then(|g| g.bind.clone())
        .unwrap_or_else(|| "0.0.0.0".to_string());

    // /ws and /health are registered first so they take priority over the UI catch-all
    let mut app = Router::new()
        .route("/ws", get(ws_handler))
        .route("/health", get(health_handler))
        .route("/canvas/{session_id}", get(canvas_ws_handler))
        .with_state(state.clone());

    if ui_enabled {
        app = app.merge(rusty_claw_web::ui_router());
        info!("Control UI available at http://{bind_addr}:{port}/");
    }

    let addr = format!("{bind_addr}:{port}");

    // Check for TLS config
    #[cfg(feature = "tls")]
    if let Some(tls_config) = state.config.gateway.as_ref().and_then(|g| g.tls.as_ref()) {
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
    .with_graceful_shutdown(shutdown_signal())
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

    axum::Json(json!({
        "status": "ok",
        "version": version,
        "connections": connections,
    }))
}

async fn shutdown_signal() {
    tokio::signal::ctrl_c()
        .await
        .expect("Failed to install CTRL+C handler");
    info!("Shutdown signal received");
}
