//! Channel abstraction and built-in channel implementations.
//!
//! Every messaging platform (Telegram, Discord, Slack, etc.) implements the
//! [`Channel`] trait. Channels are feature-gated to minimize binary size.

use async_trait::async_trait;
use serde::{Deserialize, Serialize};
use tokio::sync::mpsc;

use rusty_claw_core::types::{ChatType, InboundMessage, OutboundMessage, SendResult, SendTarget};

#[cfg(feature = "telegram")]
pub mod telegram;

#[cfg(feature = "discord")]
pub mod discord;

#[cfg(feature = "slack")]
pub mod slack;

#[cfg(feature = "webchat")]
pub mod webchat;

#[cfg(feature = "whatsapp")]
pub mod whatsapp;

#[cfg(feature = "signal")]
pub mod signal;

#[cfg(feature = "googlechat")]
pub mod googlechat;

#[cfg(feature = "msteams")]
pub mod msteams;

#[cfg(feature = "matrix")]
pub mod matrix;

#[cfg(feature = "bluebubbles")]
pub mod bluebubbles;

/// Channel metadata for UI display and discovery.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ChannelMeta {
    pub label: String,
    pub description: String,
    pub docs_url: Option<String>,
    pub icon: Option<String>,
}

/// What a channel supports.
#[derive(Debug, Clone, Default, Serialize, Deserialize)]
pub struct ChannelCapabilities {
    pub chat_types: Vec<ChatType>,
    pub supports_media: bool,
    pub supports_reactions: bool,
    pub supports_threads: bool,
    pub supports_typing: bool,
    pub supports_read_receipts: bool,
    pub supports_polls: bool,
    pub max_message_length: Option<usize>,
}

/// Channel health status.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ChannelStatus {
    pub connected: bool,
    pub account_id: Option<String>,
    pub display_name: Option<String>,
    pub error: Option<String>,
}

/// Handle to stop a running channel.
pub struct ChannelHandle {
    shutdown_tx: tokio::sync::oneshot::Sender<()>,
}

impl ChannelHandle {
    pub fn new(shutdown_tx: tokio::sync::oneshot::Sender<()>) -> Self {
        Self { shutdown_tx }
    }

    pub fn shutdown(self) {
        let _ = self.shutdown_tx.send(());
    }
}

/// Receiver for inbound messages from a channel.
pub type InboundReceiver = mpsc::UnboundedReceiver<InboundMessage>;

/// Sender for inbound messages (used by channel implementations).
pub type InboundSender = mpsc::UnboundedSender<InboundMessage>;

/// The core channel trait.
#[async_trait]
pub trait Channel: Send + Sync + 'static {
    /// Unique channel identifier (e.g., "telegram", "discord").
    fn id(&self) -> &str;

    /// Channel metadata for UI display.
    fn meta(&self) -> ChannelMeta;

    /// What this channel supports.
    fn capabilities(&self) -> ChannelCapabilities;

    /// Start monitoring for inbound messages.
    /// Returns a receiver for inbound messages and a handle to stop monitoring.
    async fn start(
        &self,
        config: &serde_json::Value,
    ) -> anyhow::Result<(InboundReceiver, ChannelHandle)>;

    /// Send a message to a target on this channel.
    async fn send(
        &self,
        target: &SendTarget,
        message: OutboundMessage,
    ) -> anyhow::Result<SendResult>;

    /// Get current channel status/health.
    async fn status(&self) -> ChannelStatus;
}

/// Registry of available channels.
#[derive(Default)]
pub struct ChannelRegistry {
    channels: Vec<Box<dyn Channel>>,
}

impl ChannelRegistry {
    pub fn new() -> Self {
        Self::default()
    }

    pub fn register(&mut self, channel: Box<dyn Channel>) {
        self.channels.push(channel);
    }

    pub fn get(&self, id: &str) -> Option<&dyn Channel> {
        self.channels.iter().find(|c| c.id() == id).map(|c| c.as_ref())
    }

    pub fn list(&self) -> Vec<&str> {
        self.channels.iter().map(|c| c.id()).collect()
    }
}
