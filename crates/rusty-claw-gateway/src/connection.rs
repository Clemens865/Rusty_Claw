//! WebSocket connection lifecycle — handshake, read/write loops.

use std::sync::Arc;

use axum::extract::ws::{Message, WebSocket};
use futures::{SinkExt, StreamExt};
use tokio::sync::mpsc;
use tracing::{debug, error, info, warn};
use uuid::Uuid;

use rusty_claw_core::protocol::{
    Features, GatewayFrame, HelloOk, Policy, ServerInfo, Snapshot, StateVersion, PROTOCOL_VERSION,
};

use crate::methods::dispatch_method;
use crate::state::{ConnectionState, GatewayState};

/// Handle a new WebSocket connection.
pub async fn handle_ws_connection(state: Arc<GatewayState>, ws: WebSocket) {
    let conn_id = Uuid::new_v4().to_string();
    info!(conn_id = %conn_id, "New WebSocket connection");

    let (mut ws_tx, mut ws_rx) = ws.split();

    // Create event channel for this connection
    let (event_tx, mut event_rx) = mpsc::unbounded_channel::<String>();

    // Register connection
    {
        let mut connections = state.connections.write().await;
        connections.insert(
            conn_id.clone(),
            ConnectionState {
                conn_id: conn_id.clone(),
                event_tx: event_tx.clone(),
                authenticated: true, // For now, auto-authenticate
            },
        );
    }

    // Send HelloOk
    let hello = HelloOk {
        protocol: PROTOCOL_VERSION,
        server: ServerInfo {
            version: env!("CARGO_PKG_VERSION").to_string(),
            commit: None,
            conn_id: conn_id.clone(),
        },
        features: Features {
            methods: vec![
                "sessions.list".into(),
                "sessions.preview".into(),
                "sessions.delete".into(),
                "sessions.reset".into(),
                "sessions.patch".into(),
                "agent".into(),
                "wake".into(),
                "models.list".into(),
                "channels.status".into(),
                "config.get".into(),
                "config.set".into(),
            ],
            events: vec![
                "agent.event".into(),
                "session.updated".into(),
            ],
        },
        snapshot: Snapshot {
            state_version: StateVersion {
                presence: state
                    .state_version
                    .load(std::sync::atomic::Ordering::SeqCst),
                health: state
                    .health_version
                    .load(std::sync::atomic::Ordering::SeqCst),
            },
            auth_mode: "none".into(),
        },
        policy: Policy {
            max_payload: 1_048_576, // 1MB
            max_buffered_bytes: 10_485_760,
            tick_interval_ms: 30_000,
        },
    };

    let hello_frame = GatewayFrame::Event {
        event: "hello".into(),
        payload: serde_json::to_value(&hello).ok(),
        seq: Some(0),
        state_version: None,
    };

    if let Ok(msg) = serde_json::to_string(&hello_frame) {
        if ws_tx.send(Message::Text(msg.into())).await.is_err() {
            cleanup_connection(&state, &conn_id).await;
            return;
        }
    }

    // Spawn event sender task
    let send_task = tokio::spawn(async move {
        while let Some(msg) = event_rx.recv().await {
            if ws_tx.send(Message::Text(msg.into())).await.is_err() {
                break;
            }
        }
    });

    // Main read loop
    while let Some(msg_result) = ws_rx.next().await {
        match msg_result {
            Ok(Message::Text(text)) => {
                let text = text.to_string();
                match serde_json::from_str::<GatewayFrame>(&text) {
                    Ok(GatewayFrame::Request { id, method, params }) => {
                        let response = dispatch_method(&state, &id, &method, params).await;
                        if let Ok(response_json) = serde_json::to_string(&response) {
                            let _ = event_tx.send(response_json);
                        }
                    }
                    Ok(_) => {
                        debug!("Received non-request frame, ignoring");
                    }
                    Err(e) => {
                        warn!(%e, "Invalid frame received");
                        let error_frame = GatewayFrame::Response {
                            id: "unknown".into(),
                            ok: false,
                            payload: None,
                            error: Some(rusty_claw_core::protocol::ErrorShape {
                                code: "parse_error".into(),
                                message: format!("Invalid frame: {e}"),
                                details: None,
                            }),
                        };
                        if let Ok(msg) = serde_json::to_string(&error_frame) {
                            let _ = event_tx.send(msg);
                        }
                    }
                }
            }
            Ok(Message::Ping(_)) => {
                // Axum handles ping/pong automatically
            }
            Ok(Message::Close(_)) => {
                debug!(conn_id = %conn_id, "Client requested close");
                break;
            }
            Err(e) => {
                error!(conn_id = %conn_id, %e, "WebSocket error");
                break;
            }
            _ => {}
        }
    }

    // Cleanup
    send_task.abort();
    cleanup_connection(&state, &conn_id).await;
    info!(conn_id = %conn_id, "WebSocket connection closed");
}

async fn cleanup_connection(state: &Arc<GatewayState>, conn_id: &str) {
    let mut connections = state.connections.write().await;
    connections.remove(conn_id);
}
