//! WebSocket connection lifecycle — handshake, read/write loops.

use std::sync::Arc;

use axum::extract::ws::{Message, WebSocket};
use futures::{SinkExt, StreamExt};
use sha2::{Digest, Sha256};
use tokio::sync::mpsc;
use tracing::{debug, error, info, warn};
use uuid::Uuid;

use rusty_claw_core::config::Config;
use rusty_claw_core::protocol::{
    ConnectParams, Features, GatewayFrame, HelloOk, Policy, ServerInfo, Snapshot, StateVersion,
    PROTOCOL_VERSION,
};

use crate::methods::dispatch_method;
use crate::state::{ConnectionState, GatewayState};

/// Determine the auth mode from config.
fn auth_mode(config: &Config) -> &str {
    config
        .gateway
        .as_ref()
        .and_then(|g| g.auth.as_ref())
        .map(|a| a.effective_mode())
        .unwrap_or("none")
}

/// Constant-time string comparison to prevent timing attacks.
fn constant_time_eq(a: &str, b: &str) -> bool {
    if a.len() != b.len() {
        return false;
    }
    a.bytes()
        .zip(b.bytes())
        .fold(0u8, |acc, (x, y)| acc | (x ^ y))
        == 0
}

/// Authenticate a client connection using ConnectParams.
/// Returns Ok(()) on success, Err(message) on failure.
fn authenticate(config: &Config, params: &ConnectParams) -> Result<(), String> {
    let mode = auth_mode(config);
    let auth_config = config
        .gateway
        .as_ref()
        .and_then(|g| g.auth.as_ref());

    match mode {
        "none" => Ok(()),
        "token" => {
            let expected = auth_config
                .and_then(|a| a.resolve_token())
                .ok_or_else(|| "Server token not configured".to_string())?;

            match &params.auth {
                Some(rusty_claw_core::protocol::AuthParams::Token { token }) => {
                    if constant_time_eq(token, &expected) {
                        Ok(())
                    } else {
                        Err("Invalid token".to_string())
                    }
                }
                _ => Err("Token authentication required".to_string()),
            }
        }
        "password" => {
            let expected = auth_config
                .and_then(|a| a.resolve_password())
                .ok_or_else(|| "Server password not configured".to_string())?;

            match &params.auth {
                Some(rusty_claw_core::protocol::AuthParams::Password { password }) => {
                    // Compare SHA-256 hashes
                    let expected_hash = format!("{:x}", Sha256::digest(expected.as_bytes()));
                    let provided_hash = format!("{:x}", Sha256::digest(password.as_bytes()));
                    if constant_time_eq(&provided_hash, &expected_hash) {
                        Ok(())
                    } else {
                        Err("Invalid password".to_string())
                    }
                }
                _ => Err("Password authentication required".to_string()),
            }
        }
        other => Err(format!("Unknown auth mode: {other}")),
    }
}

/// Handle a new WebSocket connection.
pub async fn handle_ws_connection(state: Arc<GatewayState>, ws: WebSocket) {
    let conn_id = Uuid::new_v4().to_string();
    info!(conn_id = %conn_id, "New WebSocket connection");

    let (mut ws_tx, mut ws_rx) = ws.split();

    // Create event channel for this connection
    let (event_tx, mut event_rx) = mpsc::unbounded_channel::<String>();

    // Read config snapshot for auth
    let config = state.read_config().await;
    let mode = auth_mode(&config).to_string();
    let needs_auth = mode != "none";

    // Register connection (not yet authenticated if auth required)
    {
        let mut connections = state.connections.write().await;
        connections.insert(
            conn_id.clone(),
            ConnectionState {
                conn_id: conn_id.clone(),
                event_tx: event_tx.clone(),
                authenticated: !needs_auth,
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
                "agent.abort".into(),
                "agent.status".into(),
                "wake".into(),
                "models.list".into(),
                "channels.status".into(),
                "channels.login".into(),
                "channels.logout".into(),
                "config.get".into(),
                "config.set".into(),
                "cron.list".into(),
                "cron.add".into(),
                "cron.remove".into(),
                "skills.list".into(),
                "skills.get".into(),
                "talk.config".into(),
                "node.pair.request".into(),
                "node.pair.approve".into(),
                "node.invoke".into(),
                "node.event".into(),
            ],
            events: vec![
                "agent.event".into(),
                "session.updated".into(),
                "canvas.operation".into(),
                "config.changed".into(),
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
            auth_mode: mode.clone(),
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

    // If auth required, wait for ConnectParams as first message
    if needs_auth {
        let auth_result = tokio::time::timeout(
            std::time::Duration::from_secs(10),
            wait_for_auth(&config, &mut ws_rx, &conn_id),
        )
        .await;

        match auth_result {
            Ok(Ok(())) => {
                // Mark as authenticated
                let mut connections = state.connections.write().await;
                if let Some(conn) = connections.get_mut(&conn_id) {
                    conn.authenticated = true;
                }
                // Send auth success event
                let ok_event = GatewayFrame::Event {
                    event: "auth.ok".into(),
                    payload: None,
                    seq: Some(1),
                    state_version: None,
                };
                if let Ok(msg) = serde_json::to_string(&ok_event) {
                    if ws_tx.send(Message::Text(msg.into())).await.is_err() {
                        cleanup_connection(&state, &conn_id).await;
                        return;
                    }
                }
                info!(conn_id = %conn_id, "Client authenticated");
            }
            Ok(Err(reason)) => {
                warn!(conn_id = %conn_id, %reason, "Authentication failed");
                let err_frame = GatewayFrame::Event {
                    event: "auth.error".into(),
                    payload: Some(serde_json::json!({"message": reason})),
                    seq: None,
                    state_version: None,
                };
                if let Ok(msg) = serde_json::to_string(&err_frame) {
                    let _ = ws_tx.send(Message::Text(msg.into())).await;
                }
                let _ = ws_tx.send(Message::Close(None)).await;
                cleanup_connection(&state, &conn_id).await;
                return;
            }
            Err(_) => {
                warn!(conn_id = %conn_id, "Authentication timeout");
                let _ = ws_tx.send(Message::Close(None)).await;
                cleanup_connection(&state, &conn_id).await;
                return;
            }
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

/// Wait for the client's ConnectParams message and authenticate.
async fn wait_for_auth(
    config: &Config,
    ws_rx: &mut futures::stream::SplitStream<WebSocket>,
    conn_id: &str,
) -> Result<(), String> {
    while let Some(msg_result) = ws_rx.next().await {
        match msg_result {
            Ok(Message::Text(text)) => {
                let text = text.to_string();
                // Try to parse as a ConnectParams (wrapped in a request or raw)
                if let Ok(GatewayFrame::Request { params: Some(params), .. }) = serde_json::from_str::<GatewayFrame>(&text) {
                    if let Ok(connect) = serde_json::from_value::<ConnectParams>(params) {
                        return authenticate(config, &connect);
                    }
                }
                // Also try direct ConnectParams parse
                if let Ok(connect) = serde_json::from_str::<ConnectParams>(&text) {
                    return authenticate(config, &connect);
                }
                debug!(conn_id = %conn_id, "Received non-auth message during handshake");
                return Err("Expected ConnectParams for authentication".to_string());
            }
            Ok(Message::Close(_)) => return Err("Connection closed during auth".to_string()),
            Err(e) => return Err(format!("WebSocket error during auth: {e}")),
            _ => continue,
        }
    }
    Err("Connection dropped during auth".to_string())
}

async fn cleanup_connection(state: &Arc<GatewayState>, conn_id: &str) {
    let mut connections = state.connections.write().await;
    connections.remove(conn_id);
}

#[cfg(test)]
mod tests {
    use super::*;
    use rusty_claw_core::config::{GatewayAuthConfig, GatewayConfig};
    use rusty_claw_core::protocol::{AuthParams, ClientInfo, ConnectParams};

    fn make_config_with_auth(mode: &str, token: Option<&str>, password: Option<&str>) -> Config {
        Config {
            gateway: Some(GatewayConfig {
                port: 18789,
                bind: None,
                auth: Some(GatewayAuthConfig {
                    mode: Some(mode.to_string()),
                    token: token.map(|s| s.to_string()),
                    token_env: None,
                    password: password.map(|s| s.to_string()),
                    password_env: None,
                }),
                tls: None,
                rate_limit: None,
                tailscale: None,
            }),
            ..Default::default()
        }
    }

    fn make_connect_params(auth: Option<AuthParams>) -> ConnectParams {
        ConnectParams {
            min_protocol: 3,
            max_protocol: 3,
            client: ClientInfo {
                id: "test".into(),
                display_name: None,
                version: None,
                platform: None,
                device_family: None,
                mode: None,
            },
            caps: vec![],
            role: None,
            auth,
            device: None,
        }
    }

    #[test]
    fn test_auth_mode_none() {
        let config = make_config_with_auth("none", None, None);
        let params = make_connect_params(None);
        assert!(authenticate(&config, &params).is_ok());
    }

    #[test]
    fn test_auth_token_valid() {
        let config = make_config_with_auth("token", Some("secret-token"), None);
        let params = make_connect_params(Some(AuthParams::Token {
            token: "secret-token".into(),
        }));
        assert!(authenticate(&config, &params).is_ok());
    }

    #[test]
    fn test_auth_token_invalid() {
        let config = make_config_with_auth("token", Some("secret-token"), None);
        let params = make_connect_params(Some(AuthParams::Token {
            token: "wrong-token".into(),
        }));
        assert!(authenticate(&config, &params).is_err());
    }

    #[test]
    fn test_auth_token_missing() {
        let config = make_config_with_auth("token", Some("secret-token"), None);
        let params = make_connect_params(None);
        assert!(authenticate(&config, &params).is_err());
    }

    #[test]
    fn test_auth_password_valid() {
        let config = make_config_with_auth("password", None, Some("my-password"));
        let params = make_connect_params(Some(AuthParams::Password {
            password: "my-password".into(),
        }));
        assert!(authenticate(&config, &params).is_ok());
    }

    #[test]
    fn test_auth_password_invalid() {
        let config = make_config_with_auth("password", None, Some("my-password"));
        let params = make_connect_params(Some(AuthParams::Password {
            password: "wrong".into(),
        }));
        assert!(authenticate(&config, &params).is_err());
    }

    #[test]
    fn test_constant_time_eq() {
        assert!(constant_time_eq("hello", "hello"));
        assert!(!constant_time_eq("hello", "world"));
        assert!(!constant_time_eq("hello", "hell"));
    }
}
