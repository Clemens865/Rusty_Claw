//! WebChat channel — embedded chat via WebSocket on the gateway.
//!
//! The WebChat channel allows direct browser-based chat without any
//! external service. Clients connect to the `/webchat` WebSocket endpoint.

use async_trait::async_trait;
use chrono::Utc;
use serde::{Deserialize, Serialize};
use tokio::sync::mpsc;

use rusty_claw_core::types::{ChatType, InboundMessage, OutboundMessage, SendResult, SendTarget, Sender};

use crate::{
    Channel, ChannelCapabilities, ChannelHandle, ChannelMeta, ChannelStatus, InboundReceiver,
    InboundSender,
};

/// Inbound JSON message from a webchat client.
#[derive(Debug, Deserialize)]
pub struct WebChatInbound {
    pub text: String,
    #[serde(default)]
    pub client_id: Option<String>,
}

/// Outbound JSON message to a webchat client.
#[derive(Debug, Serialize)]
pub struct WebChatOutbound {
    pub text: String,
    #[serde(rename = "type")]
    pub msg_type: String, // "reply" or "partial"
}

pub struct WebChatChannel {
    /// Sender half that the gateway's webchat WS handler uses to inject messages.
    _inbound_tx: InboundSender,
}

impl WebChatChannel {
    /// Create a new WebChat channel, returning it and the sender
    /// that the gateway's WS handler should use.
    pub fn new() -> (Self, InboundSender) {
        let (inbound_tx, _) = mpsc::unbounded_channel::<InboundMessage>();
        (Self { _inbound_tx: inbound_tx.clone() }, inbound_tx)
    }

    /// Create an InboundMessage from a webchat JSON payload.
    pub fn parse_inbound(msg: &WebChatInbound, client_id: &str) -> InboundMessage {
        InboundMessage {
            channel: "webchat".into(),
            account_id: client_id.to_string(),
            chat_type: ChatType::Dm,
            sender: Sender {
                id: client_id.to_string(),
                display_name: Some("WebChat User".into()),
                username: None,
            },
            text: Some(msg.text.clone()),
            media: vec![],
            reply_to: None,
            thread_id: None,
            timestamp: Utc::now(),
            raw: None,
        }
    }
}

#[async_trait]
impl Channel for WebChatChannel {
    fn id(&self) -> &str {
        "webchat"
    }

    fn meta(&self) -> ChannelMeta {
        ChannelMeta {
            label: "WebChat".into(),
            description: "Browser-based chat via the gateway WebSocket".into(),
            docs_url: None,
            icon: None,
        }
    }

    fn capabilities(&self) -> ChannelCapabilities {
        ChannelCapabilities {
            chat_types: vec![ChatType::Dm],
            supports_media: false,
            supports_reactions: false,
            supports_threads: false,
            supports_typing: true,
            supports_read_receipts: false,
            supports_polls: false,
            max_message_length: None,
        }
    }

    async fn start(
        &self,
        _config: &serde_json::Value,
    ) -> anyhow::Result<(InboundReceiver, ChannelHandle)> {
        // WebChat is managed by the gateway — start() creates the channels
        // but the actual WS handler pushes messages via the inbound_tx.
        let (tx, rx) = mpsc::unbounded_channel();
        let (shutdown_tx, _shutdown_rx) = tokio::sync::oneshot::channel();

        // Note: In practice, the gateway's webchat handler sends messages
        // through the inbound_tx created in new(). This start() is a no-op
        // but follows the Channel trait contract.
        let _ = tx; // Gateway handler uses the InboundSender from new()

        Ok((rx, ChannelHandle::new(shutdown_tx)))
    }

    async fn send(&self, _target: &SendTarget, _message: OutboundMessage) -> anyhow::Result<SendResult> {
        // Outbound messages for webchat are sent directly through the WebSocket
        // connection managed by the gateway. This is a placeholder.
        Ok(SendResult {
            message_id: None,
            success: true,
            error: None,
        })
    }

    async fn status(&self) -> ChannelStatus {
        ChannelStatus {
            connected: true,
            account_id: None,
            display_name: Some("WebChat".into()),
            error: None,
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_webchat_inbound_parse() {
        let json = r#"{"text": "Hello", "client_id": "abc123"}"#;
        let inbound: WebChatInbound = serde_json::from_str(json).unwrap();
        assert_eq!(inbound.text, "Hello");
        assert_eq!(inbound.client_id.as_deref(), Some("abc123"));
    }

    #[test]
    fn test_webchat_outbound_serialize() {
        let outbound = WebChatOutbound {
            text: "Hi there".into(),
            msg_type: "reply".into(),
        };
        let json = serde_json::to_string(&outbound).unwrap();
        assert!(json.contains("\"type\":\"reply\""));
    }

    #[test]
    fn test_webchat_channel_meta() {
        let (channel, _tx) = WebChatChannel::new();
        assert_eq!(channel.id(), "webchat");
        assert_eq!(channel.meta().label, "WebChat");
    }

    #[test]
    fn test_parse_inbound_message() {
        let msg = WebChatInbound {
            text: "test message".into(),
            client_id: Some("client1".into()),
        };
        let inbound = WebChatChannel::parse_inbound(&msg, "client1");
        assert_eq!(inbound.channel, "webchat");
        assert_eq!(inbound.text.as_deref(), Some("test message"));
        assert_eq!(inbound.sender.id, "client1");
    }
}
