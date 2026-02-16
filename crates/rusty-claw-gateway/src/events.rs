//! Event broadcasting to all connected WebSocket clients.

use std::sync::atomic::Ordering;
use std::sync::Arc;

use rusty_claw_core::protocol::{GatewayFrame, StateVersion};
use tracing::debug;

use crate::state::GatewayState;

/// Broadcast an event to all connected clients.
pub async fn broadcast_event(state: &Arc<GatewayState>, event: &str, payload: Option<serde_json::Value>) {
    let frame = GatewayFrame::Event {
        event: event.to_string(),
        payload,
        seq: None,
        state_version: Some(StateVersion {
            presence: state.state_version.load(Ordering::SeqCst),
            health: state.health_version.load(Ordering::SeqCst),
        }),
    };

    let msg = match serde_json::to_string(&frame) {
        Ok(m) => m,
        Err(e) => {
            tracing::error!(%e, "Failed to serialize event");
            return;
        }
    };

    let connections = state.connections.read().await;
    let mut sent = 0;
    for conn in connections.values() {
        if conn.event_tx.send(msg.clone()).is_ok() {
            sent += 1;
        }
    }
    debug!(event, sent, "Broadcast event");
}
