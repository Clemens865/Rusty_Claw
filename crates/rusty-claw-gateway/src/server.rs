//! Axum-based WebSocket server.

use std::sync::Arc;

use axum::{
    extract::{State, WebSocketUpgrade},
    response::IntoResponse,
    routing::get,
    Router,
};
use serde_json::json;
use tracing::info;

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
        .with_state(state);

    if ui_enabled {
        app = app.merge(rusty_claw_web::ui_router());
        info!("Control UI available at http://{bind_addr}:{port}/");
    }

    let addr = format!("{bind_addr}:{port}");
    let listener = tokio::net::TcpListener::bind(&addr).await?;
    info!("Gateway listening on {addr}");

    axum::serve(listener, app)
        .with_graceful_shutdown(shutdown_signal())
        .await?;

    Ok(())
}

async fn ws_handler(
    ws: WebSocketUpgrade,
    State(state): State<Arc<GatewayState>>,
) -> impl IntoResponse {
    ws.on_upgrade(move |socket| handle_ws_connection(state, socket))
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
