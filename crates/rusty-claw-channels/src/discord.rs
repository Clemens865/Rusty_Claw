//! Discord channel implementation.
//!
//! Supports DMs, guilds, and threads via the Discord HTTP API.
//! The bot token comes from config.

use async_trait::async_trait;
use tokio::sync::mpsc;
use tracing::{error, info};

use rusty_claw_core::types::{
    ChatType, InboundMessage, OutboundMessage, SendResult, SendTarget,
};

use crate::{
    Channel, ChannelCapabilities, ChannelHandle, ChannelMeta, ChannelStatus, InboundReceiver,
};

/// Discord channel configuration (typed).
#[derive(Debug, Clone, serde::Deserialize)]
pub struct DiscordConfig {
    #[serde(default)]
    pub bot_token: Option<String>,
    #[serde(default)]
    pub bot_token_env: Option<String>,
    #[serde(default)]
    pub allowed_guilds: Vec<String>,
    #[serde(default)]
    pub allowed_users: Vec<String>,
}

impl DiscordConfig {
    pub fn resolve_bot_token(&self) -> Option<String> {
        if let Some(ref token) = self.bot_token {
            if !token.is_empty() {
                return Some(token.clone());
            }
        }
        if let Some(ref env_var) = self.bot_token_env {
            if let Ok(val) = std::env::var(env_var) {
                if !val.is_empty() {
                    return Some(val);
                }
            }
        }
        None
    }
}

pub struct DiscordChannel {
    bot_token: String,
    _allowed_guilds: Vec<String>,
    _allowed_users: Vec<String>,
}

impl DiscordChannel {
    pub fn new(
        bot_token: String,
        allowed_guilds: Vec<String>,
        allowed_users: Vec<String>,
    ) -> Self {
        Self {
            bot_token,
            _allowed_guilds: allowed_guilds,
            _allowed_users: allowed_users,
        }
    }
}

/// Split a message at Discord's 2000-char limit.
pub fn split_discord_message(text: &str) -> Vec<String> {
    const MAX_LEN: usize = 2000;
    if text.len() <= MAX_LEN {
        return vec![text.to_string()];
    }

    let mut chunks = Vec::new();
    let mut remaining = text;
    while !remaining.is_empty() {
        if remaining.len() <= MAX_LEN {
            chunks.push(remaining.to_string());
            break;
        }
        let split_at = remaining[..MAX_LEN]
            .rfind('\n')
            .unwrap_or(MAX_LEN);
        chunks.push(remaining[..split_at].to_string());
        remaining = remaining[split_at..].trim_start_matches('\n');
    }
    chunks
}

#[async_trait]
impl Channel for DiscordChannel {
    fn id(&self) -> &str {
        "discord"
    }

    fn meta(&self) -> ChannelMeta {
        ChannelMeta {
            label: "Discord".into(),
            description: "Discord bot integration".into(),
            docs_url: Some("https://discord.com/developers/docs".into()),
            icon: None,
        }
    }

    fn capabilities(&self) -> ChannelCapabilities {
        ChannelCapabilities {
            chat_types: vec![ChatType::Dm, ChatType::Group, ChatType::Thread],
            supports_media: true,
            supports_reactions: true,
            supports_threads: true,
            supports_typing: true,
            supports_read_receipts: false,
            supports_polls: false,
            max_message_length: Some(2000),
        }
    }

    async fn start(
        &self,
        _config: &serde_json::Value,
    ) -> anyhow::Result<(InboundReceiver, ChannelHandle)> {
        let (_inbound_tx, inbound_rx) = mpsc::unbounded_channel::<InboundMessage>();
        let (shutdown_tx, shutdown_rx) = tokio::sync::oneshot::channel();

        tokio::spawn(async move {
            info!("Discord channel started");
            let _ = shutdown_rx.await;
            info!("Discord channel stopped");
        });

        Ok((inbound_rx, ChannelHandle::new(shutdown_tx)))
    }

    async fn send(
        &self,
        target: &SendTarget,
        message: OutboundMessage,
    ) -> anyhow::Result<SendResult> {
        let text = message.text.unwrap_or_default();
        if text.is_empty() {
            return Ok(SendResult {
                message_id: None,
                success: true,
                error: None,
            });
        }

        let chunks = split_discord_message(&text);
        let client = reqwest::Client::new();

        for chunk in &chunks {
            let resp = client
                .post(format!(
                    "https://discord.com/api/v10/channels/{}/messages",
                    target.chat_id
                ))
                .header("Authorization", format!("Bot {}", self.bot_token))
                .header("Content-Type", "application/json")
                .json(&serde_json::json!({ "content": chunk }))
                .send()
                .await;

            match resp {
                Ok(r) if r.status().is_success() => {}
                Ok(r) => {
                    let status = r.status();
                    let body = r.text().await.unwrap_or_default();
                    error!(%status, body, "Discord send failed");
                    return Ok(SendResult {
                        message_id: None,
                        success: false,
                        error: Some(format!("Discord API error {status}")),
                    });
                }
                Err(e) => {
                    return Ok(SendResult {
                        message_id: None,
                        success: false,
                        error: Some(e.to_string()),
                    });
                }
            }
        }

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
            display_name: Some("Discord Bot".into()),
            error: None,
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_split_discord_message_short() {
        let chunks = split_discord_message("Hello");
        assert_eq!(chunks.len(), 1);
        assert_eq!(chunks[0], "Hello");
    }

    #[test]
    fn test_split_discord_message_long() {
        let text = "a".repeat(3000);
        let chunks = split_discord_message(&text);
        assert!(chunks.len() >= 2);
        for chunk in &chunks {
            assert!(chunk.len() <= 2000);
        }
    }

    #[test]
    fn test_discord_config_resolve_token() {
        unsafe { std::env::set_var("TEST_DISCORD_TOKEN_RC", "disc-token-123") };
        let config = DiscordConfig {
            bot_token: None,
            bot_token_env: Some("TEST_DISCORD_TOKEN_RC".into()),
            allowed_guilds: vec![],
            allowed_users: vec![],
        };
        assert_eq!(config.resolve_bot_token(), Some("disc-token-123".into()));
        unsafe { std::env::remove_var("TEST_DISCORD_TOKEN_RC") };
    }

    #[test]
    fn test_discord_channel_meta() {
        let channel = DiscordChannel::new("token".into(), vec![], vec![]);
        assert_eq!(channel.id(), "discord");
        assert_eq!(channel.capabilities().max_message_length, Some(2000));
    }
}
