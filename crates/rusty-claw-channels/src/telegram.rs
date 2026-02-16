//! Telegram channel implementation using teloxide.

use std::sync::Arc;

use async_trait::async_trait;
use teloxide::prelude::*;
use teloxide::types::{ChatKind, MediaKind, MessageKind, UpdateKind};
use tokio::sync::{mpsc, oneshot, RwLock};
use tracing::{debug, error, info, warn};

use rusty_claw_core::types::{
    ChatType, InboundMessage, OutboundMessage, Sender, SendResult, SendTarget,
};

use crate::{
    Channel, ChannelCapabilities, ChannelHandle, ChannelMeta, ChannelStatus, InboundReceiver,
};

pub struct TelegramChannel {
    bot_token: String,
    allowed_users: Vec<String>,
    bot_username: Arc<RwLock<Option<String>>>,
}

impl TelegramChannel {
    pub fn new(bot_token: String, allowed_users: Vec<String>) -> Self {
        Self {
            bot_token,
            allowed_users,
            bot_username: Arc::new(RwLock::new(None)),
        }
    }

    /// Check if the message is addressed to our bot in a group.
    fn is_addressed_to_bot(text: &str, bot_username: &Option<String>) -> bool {
        if let Some(username) = bot_username {
            let mention = format!("@{username}");
            text.contains(&mention)
        } else {
            // If we don't know our username, accept all
            true
        }
    }
}

#[async_trait]
impl Channel for TelegramChannel {
    fn id(&self) -> &str {
        "telegram"
    }

    fn meta(&self) -> ChannelMeta {
        ChannelMeta {
            label: "Telegram".into(),
            description: "Telegram Bot API channel".into(),
            docs_url: Some("https://core.telegram.org/bots/api".into()),
            icon: None,
        }
    }

    fn capabilities(&self) -> ChannelCapabilities {
        ChannelCapabilities {
            chat_types: vec![ChatType::Dm, ChatType::Group],
            supports_media: true,
            supports_reactions: false,
            supports_threads: true,
            supports_typing: true,
            supports_read_receipts: false,
            supports_polls: false,
            max_message_length: Some(4096),
        }
    }

    async fn start(
        &self,
        _config: &serde_json::Value,
    ) -> anyhow::Result<(InboundReceiver, ChannelHandle)> {
        let bot = Bot::new(&self.bot_token);

        // Get bot info
        match bot.get_me().await {
            Ok(me) => {
                let username = me.username.clone();
                info!(username = ?username, "Telegram bot connected");
                *self.bot_username.write().await = username;
            }
            Err(e) => {
                error!(%e, "Failed to get bot info");
                anyhow::bail!("Failed to connect to Telegram: {e}");
            }
        }

        let (inbound_tx, inbound_rx) = mpsc::unbounded_channel::<InboundMessage>();
        let (shutdown_tx, mut shutdown_rx) = oneshot::channel::<()>();

        let bot_clone = bot.clone();
        let allowed_users = self.allowed_users.clone();
        let bot_username = self.bot_username.clone();

        // Spawn polling task
        tokio::spawn(async move {
            let mut offset: i32 = 0;

            loop {
                // Check shutdown
                if shutdown_rx.try_recv().is_ok() {
                    info!("Telegram polling shutdown requested");
                    break;
                }

                // Long poll with 30s timeout
                let updates = bot_clone
                    .get_updates()
                    .offset(offset)
                    .timeout(30)
                    .await;

                match updates {
                    Ok(updates) => {
                        for update in updates {
                            offset = update.id.as_offset();

                            if let UpdateKind::Message(message) = &update.kind {
                                // Extract text from the message
                                let text = match &message.kind {
                                    MessageKind::Common(common) => match &common.media_kind {
                                        MediaKind::Text(text_media) => {
                                            Some(text_media.text.clone())
                                        }
                                        _ => None,
                                    },
                                    _ => None,
                                };

                                let text = match text {
                                    Some(t) => t,
                                    None => continue,
                                };

                                let is_private =
                                    matches!(message.chat.kind, ChatKind::Private(_));
                                let username_lock = bot_username.read().await;

                                // In groups, only respond when @mentioned
                                if !is_private
                                    && !Self::is_addressed_to_bot(&text, &username_lock)
                                {
                                    continue;
                                }
                                drop(username_lock);

                                // Check allowed users
                                let sender_id = message
                                    .from
                                    .as_ref()
                                    .map(|u| u.id.0.to_string())
                                    .unwrap_or_default();

                                if !allowed_users.is_empty()
                                    && !allowed_users.contains(&sender_id)
                                {
                                    debug!(
                                        sender = %sender_id,
                                        "Message from non-allowed user, ignoring"
                                    );
                                    continue;
                                }

                                let sender = Sender {
                                    id: sender_id,
                                    display_name: message
                                        .from
                                        .as_ref()
                                        .map(|u| u.full_name()),
                                    username: message
                                        .from
                                        .as_ref()
                                        .and_then(|u| u.username.clone()),
                                };

                                let chat_type = if is_private {
                                    ChatType::Dm
                                } else {
                                    ChatType::Group
                                };

                                let inbound = InboundMessage {
                                    channel: "telegram".into(),
                                    account_id: message.chat.id.0.to_string(),
                                    chat_type,
                                    sender,
                                    text: Some(text),
                                    media: vec![],
                                    reply_to: message
                                        .reply_to_message()
                                        .map(|r| r.id.0.to_string()),
                                    thread_id: message
                                        .thread_id
                                        .map(|t| t.0.to_string()),
                                    timestamp: chrono::Utc::now(),
                                    raw: None,
                                };

                                if inbound_tx.send(inbound).is_err() {
                                    warn!(
                                        "Inbound channel closed, stopping Telegram polling"
                                    );
                                    return;
                                }
                            }
                        }
                    }
                    Err(e) => {
                        error!(%e, "Telegram polling error");
                        tokio::time::sleep(std::time::Duration::from_secs(5)).await;
                    }
                }
            }
        });

        Ok((inbound_rx, ChannelHandle::new(shutdown_tx)))
    }

    async fn send(
        &self,
        target: &SendTarget,
        message: OutboundMessage,
    ) -> anyhow::Result<SendResult> {
        let bot = Bot::new(&self.bot_token);
        let chat_id = ChatId(target.chat_id.parse::<i64>().unwrap_or(0));

        if let Some(text) = &message.text {
            // Send typing indicator first
            let _ = bot
                .send_chat_action(chat_id, teloxide::types::ChatAction::Typing)
                .await;

            // Split long messages (Telegram max is 4096)
            let chunks = split_message(text, 4096);
            let mut last_msg_id = None;

            for chunk in chunks {
                let req = bot.send_message(chat_id, &chunk);

                match req.await {
                    Ok(sent) => {
                        last_msg_id = Some(sent.id.0.to_string());
                    }
                    Err(e) => {
                        return Ok(SendResult {
                            message_id: None,
                            success: false,
                            error: Some(format!("Send error: {e}")),
                        });
                    }
                }
            }

            Ok(SendResult {
                message_id: last_msg_id,
                success: true,
                error: None,
            })
        } else {
            Ok(SendResult {
                message_id: None,
                success: false,
                error: Some("No text to send".into()),
            })
        }
    }

    async fn status(&self) -> ChannelStatus {
        let username = self.bot_username.read().await.clone();
        ChannelStatus {
            connected: username.is_some(),
            account_id: None,
            display_name: username,
            error: None,
        }
    }
}

/// Split a message into chunks that fit within Telegram's limit.
fn split_message(text: &str, max_len: usize) -> Vec<String> {
    if text.len() <= max_len {
        return vec![text.to_string()];
    }

    let mut chunks = Vec::new();
    let mut remaining = text;

    while !remaining.is_empty() {
        if remaining.len() <= max_len {
            chunks.push(remaining.to_string());
            break;
        }

        // Try to split at a newline
        let split_at = remaining[..max_len].rfind('\n').unwrap_or(max_len);

        chunks.push(remaining[..split_at].to_string());
        remaining = remaining[split_at..].trim_start_matches('\n');
    }

    chunks
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_split_message_short() {
        let chunks = split_message("hello", 4096);
        assert_eq!(chunks.len(), 1);
        assert_eq!(chunks[0], "hello");
    }

    #[test]
    fn test_split_message_long() {
        let text = "a".repeat(5000);
        let chunks = split_message(&text, 4096);
        assert_eq!(chunks.len(), 2);
        assert!(chunks[0].len() <= 4096);
    }

    #[test]
    fn test_is_addressed_to_bot() {
        assert!(TelegramChannel::is_addressed_to_bot(
            "hello @mybot how are you",
            &Some("mybot".into())
        ));
        assert!(!TelegramChannel::is_addressed_to_bot(
            "hello world",
            &Some("mybot".into())
        ));
        // No username known = accept all
        assert!(TelegramChannel::is_addressed_to_bot("hello", &None));
    }
}
