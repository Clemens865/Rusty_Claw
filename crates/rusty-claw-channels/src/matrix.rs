//! Matrix channel implementation (HTTP API, no SDK dependency).

use async_trait::async_trait;
use tokio::sync::mpsc;
use tracing::{error, info, warn};

use rusty_claw_core::types::{
    ChatType, InboundMessage, OutboundMessage, SendResult, SendTarget, Sender,
};

use crate::{
    Channel, ChannelCapabilities, ChannelHandle, ChannelMeta, ChannelStatus, InboundReceiver,
};

pub struct MatrixChannel {
    homeserver_url: String,
    access_token: String,
    user_id: Option<String>,
}

impl MatrixChannel {
    pub fn new(homeserver_url: String, access_token: String, user_id: Option<String>) -> Self {
        Self {
            homeserver_url,
            access_token,
            user_id,
        }
    }
}

/// Parse a Matrix sync response for m.room.message events.
pub fn parse_sync_messages(
    sync: &serde_json::Value,
    own_user_id: Option<&str>,
) -> Vec<(String, String, String)> {
    let mut messages = Vec::new();

    if let Some(rooms) = sync.get("rooms").and_then(|r| r.get("join")) {
        if let Some(rooms_map) = rooms.as_object() {
            for (room_id, room_data) in rooms_map {
                if let Some(events) = room_data
                    .get("timeline")
                    .and_then(|t| t.get("events"))
                    .and_then(|e| e.as_array())
                {
                    for event in events {
                        let event_type = event
                            .get("type")
                            .and_then(|v| v.as_str())
                            .unwrap_or("");
                        if event_type != "m.room.message" {
                            continue;
                        }

                        let sender = event
                            .get("sender")
                            .and_then(|v| v.as_str())
                            .unwrap_or("")
                            .to_string();

                        // Skip own messages
                        if own_user_id.is_some_and(|own| own == sender) {
                            continue;
                        }

                        let body = event
                            .get("content")
                            .and_then(|c| c.get("body"))
                            .and_then(|v| v.as_str())
                            .unwrap_or("")
                            .to_string();

                        if !sender.is_empty() && !body.is_empty() {
                            messages.push((sender, body, room_id.clone()));
                        }
                    }
                }
            }
        }
    }

    messages
}

#[async_trait]
impl Channel for MatrixChannel {
    fn id(&self) -> &str {
        "matrix"
    }

    fn meta(&self) -> ChannelMeta {
        ChannelMeta {
            label: "Matrix".into(),
            description: "Matrix protocol via Client-Server API".into(),
            docs_url: Some("https://spec.matrix.org/latest/client-server-api/".into()),
            icon: None,
        }
    }

    fn capabilities(&self) -> ChannelCapabilities {
        ChannelCapabilities {
            chat_types: vec![ChatType::Dm, ChatType::Group],
            supports_media: true,
            supports_reactions: true,
            supports_threads: true,
            supports_typing: true,
            supports_read_receipts: true,
            supports_polls: false,
            max_message_length: None, // No hard limit
        }
    }

    async fn start(
        &self,
        _config: &serde_json::Value,
    ) -> anyhow::Result<(InboundReceiver, ChannelHandle)> {
        let (inbound_tx, inbound_rx) = mpsc::unbounded_channel::<InboundMessage>();
        let (shutdown_tx, mut shutdown_rx) = tokio::sync::oneshot::channel();

        let homeserver = self.homeserver_url.clone();
        let token = self.access_token.clone();
        let user_id = self.user_id.clone();

        tokio::spawn(async move {
            info!("Matrix channel started, syncing");
            let client = reqwest::Client::new();
            let mut since: Option<String> = None;

            loop {
                let mut url = format!("{homeserver}/_matrix/client/v3/sync?timeout=30000");
                if let Some(ref s) = since {
                    url.push_str(&format!("&since={s}"));
                }

                tokio::select! {
                    _ = &mut shutdown_rx => {
                        info!("Matrix channel stopped");
                        break;
                    }
                    result = client.get(&url).header("Authorization", format!("Bearer {token}")).send() => {
                        match result {
                            Ok(resp) if resp.status().is_success() => {
                                if let Ok(sync_data) = resp.json::<serde_json::Value>().await {
                                    if let Some(next_batch) = sync_data.get("next_batch").and_then(|v| v.as_str()) {
                                        since = Some(next_batch.to_string());
                                    }

                                    let messages = parse_sync_messages(&sync_data, user_id.as_deref());
                                    for (sender, text, room_id) in messages {
                                        let msg = InboundMessage {
                                            channel: "matrix".into(),
                                            account_id: room_id,
                                            chat_type: ChatType::Group,
                                            sender: Sender {
                                                id: sender,
                                                display_name: None,
                                                username: None,
                                            },
                                            text: Some(text),
                                            media: vec![],
                                            reply_to: None,
                                            thread_id: None,
                                            timestamp: chrono::Utc::now(),
                                            raw: None,
                                        };
                                        let _ = inbound_tx.send(msg);
                                    }
                                }
                            }
                            Ok(resp) => {
                                warn!(status = %resp.status(), "Matrix sync non-success");
                                tokio::time::sleep(std::time::Duration::from_secs(5)).await;
                            }
                            Err(e) => {
                                warn!(%e, "Matrix sync error");
                                tokio::time::sleep(std::time::Duration::from_secs(5)).await;
                            }
                        }
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
        let text = message.text.unwrap_or_default();
        if text.is_empty() {
            return Ok(SendResult {
                message_id: None,
                success: true,
                error: None,
            });
        }

        let txn_id = uuid::Uuid::new_v4().to_string();
        let client = reqwest::Client::new();
        let url = format!(
            "{}/_matrix/client/v3/rooms/{}/send/m.room.message/{}",
            self.homeserver_url,
            target.chat_id,
            txn_id,
        );

        let resp = client
            .put(&url)
            .header("Authorization", format!("Bearer {}", self.access_token))
            .json(&serde_json::json!({
                "msgtype": "m.text",
                "body": text,
            }))
            .send()
            .await;

        match resp {
            Ok(r) if r.status().is_success() => Ok(SendResult {
                message_id: None,
                success: true,
                error: None,
            }),
            Ok(r) => {
                let status = r.status();
                error!(%status, "Matrix send failed");
                Ok(SendResult {
                    message_id: None,
                    success: false,
                    error: Some(format!("Matrix API error {status}")),
                })
            }
            Err(e) => Ok(SendResult {
                message_id: None,
                success: false,
                error: Some(e.to_string()),
            }),
        }
    }

    async fn status(&self) -> ChannelStatus {
        ChannelStatus {
            connected: true,
            account_id: self.user_id.clone(),
            display_name: Some("Matrix".into()),
            error: None,
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_config_resolve() {
        unsafe { std::env::set_var("TEST_MATRIX_TOKEN_RC", "mx-token-123") };
        let config = rusty_claw_core::config::MatrixConfig {
            homeserver_url: Some("https://matrix.org".into()),
            username: Some("bot".into()),
            password: None,
            password_env: None,
            access_token: None,
            access_token_env: Some("TEST_MATRIX_TOKEN_RC".into()),
        };
        assert_eq!(config.resolve_access_token(), Some("mx-token-123".into()));
        unsafe { std::env::remove_var("TEST_MATRIX_TOKEN_RC") };
    }

    #[test]
    fn test_sync_message_parsing() {
        let sync = serde_json::json!({
            "next_batch": "batch_123",
            "rooms": {
                "join": {
                    "!room:matrix.org": {
                        "timeline": {
                            "events": [{
                                "type": "m.room.message",
                                "sender": "@user:matrix.org",
                                "content": {
                                    "msgtype": "m.text",
                                    "body": "Hello Matrix!"
                                }
                            }]
                        }
                    }
                }
            }
        });

        let messages = parse_sync_messages(&sync, Some("@bot:matrix.org"));
        assert_eq!(messages.len(), 1);
        assert_eq!(messages[0].0, "@user:matrix.org");
        assert_eq!(messages[0].1, "Hello Matrix!");
        assert_eq!(messages[0].2, "!room:matrix.org");
    }
}
