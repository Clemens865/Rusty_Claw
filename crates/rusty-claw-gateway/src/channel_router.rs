//! Routes inbound channel messages to agent runs.

use std::sync::Arc;

use tracing::{error, info};

use rusty_claw_core::session::{Session, SessionKey, SessionScope};
use rusty_claw_core::types::InboundMessage;
use rusty_claw_agent::AgentEvent;
use rusty_claw_channels::InboundReceiver;

use crate::state::GatewayState;

/// Start routing messages from a channel's inbound receiver to agent runs.
pub fn start_channel_router(
    state: Arc<GatewayState>,
    channel_id: String,
    mut rx: InboundReceiver,
) -> tokio::task::JoinHandle<()> {
    tokio::spawn(async move {
        info!(channel = %channel_id, "Channel router started");

        while let Some(message) = rx.recv().await {
            let state = state.clone();
            let channel_id = channel_id.clone();

            tokio::spawn(async move {
                if let Err(e) = handle_inbound_message(&state, &channel_id, message).await {
                    error!(channel = %channel_id, %e, "Failed to handle inbound message");
                }
            });
        }

        info!(channel = %channel_id, "Channel router stopped");
    })
}

async fn handle_inbound_message(
    state: &Arc<GatewayState>,
    channel_id: &str,
    message: InboundMessage,
) -> anyhow::Result<()> {
    // Build session key from the message
    let key = SessionKey {
        channel: channel_id.to_string(),
        account_id: message.account_id.clone(),
        chat_type: message.chat_type,
        peer_id: message.sender.id.clone(),
        scope: SessionScope::PerSender,
    };

    // Load or create session
    let mut session = match state.sessions.load(&key).await? {
        Some(s) => s,
        None => Session::new(key.clone()),
    };

    // Set up event channel
    let (event_tx, mut event_rx) = tokio::sync::mpsc::unbounded_channel::<AgentEvent>();

    // Collect final text for response
    let response_text = Arc::new(tokio::sync::Mutex::new(String::new()));
    let response_text_clone = response_text.clone();

    // Forward events as broadcasts
    let state_clone = state.clone();
    let event_task = tokio::spawn(async move {
        while let Some(event) = event_rx.recv().await {
            // Collect final text
            if let AgentEvent::BlockReply { ref text, is_final: true } = event {
                let mut rt = response_text_clone.lock().await;
                *rt = text.clone();
            }

            if let Ok(payload) = serde_json::to_value(&event) {
                crate::events::broadcast_event(&state_clone, "agent.event", Some(payload)).await;
            }
        }
    });

    // Resolve provider
    let (provider, credentials) = match state.providers.default() {
        Some(pc) => pc,
        None => {
            anyhow::bail!("No default provider configured");
        }
    };

    // Run agent
    info!(channel = channel_id, sender = %message.sender.id, "Running agent for channel message");

    // Read config snapshot
    let config = Arc::new(state.read_config().await);

    let result = rusty_claw_agent::run_agent(
        &mut session,
        message.clone(),
        &config,
        &state.tools,
        provider,
        credentials,
        event_tx,
        &state.hooks,
    )
    .await;

    // Wait for event forwarding to complete
    let _ = event_task.await;

    // Save session
    state.sessions.save(&session).await?;

    // Send response back through the channel
    let reply_text = response_text.lock().await.clone();
    if !reply_text.is_empty() {
        let target = rusty_claw_core::types::SendTarget {
            channel: channel_id.to_string(),
            account_id: message.account_id.clone(),
            chat_id: message.sender.id.clone(),
            chat_type: message.chat_type,
        };

        let outbound = rusty_claw_core::types::OutboundMessage {
            text: Some(reply_text),
            media: vec![],
            reply_to: None,
            thread_id: message.thread_id.clone(),
        };

        if let Some(channel) = state.channels.get(channel_id) {
            match channel.send(&target, outbound).await {
                Ok(_) => info!(channel = channel_id, "Response sent"),
                Err(e) => error!(channel = channel_id, %e, "Failed to send response"),
            }
        }
    }

    if let Err(e) = result {
        error!(channel = channel_id, %e, "Agent run failed");
    }

    Ok(())
}
