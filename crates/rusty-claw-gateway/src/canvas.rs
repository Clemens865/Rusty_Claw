//! Canvas/A2UI WebSocket handler.
//!
//! Provides `/canvas/{session_id}` endpoint for real-time agent-to-UI communication.

use std::collections::HashMap;
use std::sync::Arc;

use axum::{
    extract::{Path, State, WebSocketUpgrade},
    response::IntoResponse,
};
use axum::extract::ws::{Message, WebSocket};
use futures::{SinkExt, StreamExt};
use tokio::sync::{mpsc, RwLock};
use tracing::{debug, info};

use rusty_claw_canvas::{CanvasOperation, CanvasEvent};
use crate::state::GatewayState;

/// State for all active canvas sessions.
pub struct CanvasManager {
    sessions: RwLock<HashMap<String, CanvasSessionState>>,
}

struct CanvasSessionState {
    /// Accumulated HTML components
    components: Vec<String>,
    /// Connected client senders
    clients: Vec<mpsc::UnboundedSender<String>>,
}

impl Default for CanvasManager {
    fn default() -> Self {
        Self::new()
    }
}

impl CanvasManager {
    pub fn new() -> Self {
        Self {
            sessions: RwLock::new(HashMap::new()),
        }
    }

    /// Push an operation from the agent into a canvas session.
    pub async fn push_operation(&self, session_id: &str, op: CanvasOperation) {
        let mut sessions = self.sessions.write().await;
        let session = sessions.entry(session_id.to_string()).or_insert_with(|| {
            CanvasSessionState {
                components: Vec::new(),
                clients: Vec::new(),
            }
        });

        let event = match &op {
            CanvasOperation::Push { html } => {
                session.components.push(html.clone());
                CanvasEvent::ComponentAdded {
                    index: session.components.len() - 1,
                    html: html.clone(),
                }
            }
            CanvasOperation::Reset => {
                session.components.clear();
                CanvasEvent::Reset
            }
            CanvasOperation::Eval { js } => {
                CanvasEvent::Eval { js: js.clone() }
            }
            CanvasOperation::Snapshot => {
                CanvasEvent::Snapshot {
                    components: session.components.clone(),
                }
            }
        };

        // Broadcast to all connected clients
        if let Ok(msg) = serde_json::to_string(&event) {
            session.clients.retain(|tx| tx.send(msg.clone()).is_ok());
        }
    }

    /// Get current snapshot of a session.
    pub async fn snapshot(&self, session_id: &str) -> Vec<String> {
        let sessions = self.sessions.read().await;
        sessions
            .get(session_id)
            .map(|s| s.components.clone())
            .unwrap_or_default()
    }
}

/// WebSocket upgrade handler for canvas connections.
pub async fn canvas_ws_handler(
    ws: WebSocketUpgrade,
    Path(session_id): Path<String>,
    State(state): State<Arc<GatewayState>>,
) -> impl IntoResponse {
    ws.on_upgrade(move |socket| handle_canvas_connection(state, session_id, socket))
}

async fn handle_canvas_connection(
    state: Arc<GatewayState>,
    session_id: String,
    ws: WebSocket,
) {
    info!(session_id = %session_id, "Canvas client connected");

    let (mut ws_tx, mut ws_rx) = ws.split();
    let (tx, mut rx) = mpsc::unbounded_channel::<String>();

    // Register client
    {
        let mut sessions = state.canvas.sessions.write().await;
        let session = sessions.entry(session_id.clone()).or_insert_with(|| {
            CanvasSessionState {
                components: Vec::new(),
                clients: Vec::new(),
            }
        });
        session.clients.push(tx);

        // Send current snapshot
        let snapshot = CanvasEvent::Snapshot {
            components: session.components.clone(),
        };
        if let Ok(msg) = serde_json::to_string(&snapshot) {
            let _ = ws_tx.send(Message::Text(msg.into())).await;
        }
    }

    // Forward events to WebSocket
    let send_task = tokio::spawn(async move {
        while let Some(msg) = rx.recv().await {
            if ws_tx.send(Message::Text(msg.into())).await.is_err() {
                break;
            }
        }
    });

    // Read client messages (for future bidirectional use)
    while let Some(msg) = ws_rx.next().await {
        match msg {
            Ok(Message::Close(_)) => break,
            Err(_) => break,
            _ => {}
        }
    }

    send_task.abort();
    debug!(session_id = %session_id, "Canvas client disconnected");
}
