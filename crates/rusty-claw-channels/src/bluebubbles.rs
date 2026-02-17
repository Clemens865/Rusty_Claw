//! iMessage channel via BlueBubbles HTTP API.

use async_trait::async_trait;
use tokio::sync::mpsc;
use tracing::{error, info, warn};

use rusty_claw_core::types::{
    ChatType, InboundMessage, OutboundMessage, SendResult, SendTarget, Sender,
};

use crate::{
    Channel, ChannelCapabilities, ChannelHandle, ChannelMeta, ChannelStatus, InboundReceiver,
};

pub struct BlueBubblesChannel {
    api_url: String,
    password: String,
}

impl BlueBubblesChannel {
    pub fn new(api_url: String, password: String) -> Self {
        Self { api_url, password }
    }
}

/// Parse BlueBubbles message list response into (sender, text, chat_guid, is_group).
pub fn parse_messages(
    messages: &[serde_json::Value],
) -> Vec<(String, String, String, bool)> {
    let mut results = Vec::new();

    for msg in messages {
        let is_from_me = msg
            .get("isFromMe")
            .and_then(|v| v.as_bool())
            .unwrap_or(false);
        if is_from_me {
            continue;
        }

        let text = msg
            .get("text")
            .and_then(|v| v.as_str())
            .unwrap_or("")
            .to_string();

        let sender = msg
            .get("handle")
            .and_then(|h| h.get("address"))
            .and_then(|v| v.as_str())
            .unwrap_or("")
            .to_string();

        let chat_guid = msg
            .get("chats")
            .and_then(|c| c.as_array())
            .and_then(|a| a.first())
            .and_then(|c| c.get("guid"))
            .and_then(|v| v.as_str())
            .unwrap_or("")
            .to_string();

        let is_group = msg
            .get("chats")
            .and_then(|c| c.as_array())
            .and_then(|a| a.first())
            .and_then(|c| c.get("participants"))
            .and_then(|p| p.as_array())
            .is_some_and(|p| p.len() > 2);

        if !sender.is_empty() && !text.is_empty() && !chat_guid.is_empty() {
            results.push((sender, text, chat_guid, is_group));
        }
    }

    results
}

#[async_trait]
impl Channel for BlueBubblesChannel {
    fn id(&self) -> &str {
        "bluebubbles"
    }

    fn meta(&self) -> ChannelMeta {
        ChannelMeta {
            label: "iMessage".into(),
            description: "iMessage via BlueBubbles".into(),
            docs_url: Some("https://bluebubbles.app/".into()),
            icon: None,
        }
    }

    fn capabilities(&self) -> ChannelCapabilities {
        ChannelCapabilities {
            chat_types: vec![ChatType::Dm, ChatType::Group],
            supports_media: true,
            supports_reactions: true,
            supports_threads: false,
            supports_typing: false,
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

        let api_url = self.api_url.clone();
        let password = self.password.clone();

        tokio::spawn(async move {
            info!("BlueBubbles (iMessage) channel started, polling every 3s");
            let client = reqwest::Client::new();
            let mut last_timestamp = chrono::Utc::now().timestamp_millis();

            loop {
                tokio::select! {
                    _ = &mut shutdown_rx => {
                        info!("BlueBubbles channel stopped");
                        break;
                    }
                    _ = tokio::time::sleep(std::time::Duration::from_secs(3)) => {
                        let url = format!(
                            "{api_url}/api/v1/message?after={last_timestamp}&password={password}&limit=50&sort=ASC"
                        );

                        match client.get(&url).send().await {
                            Ok(resp) if resp.status().is_success() => {
                                if let Ok(body) = resp.json::<serde_json::Value>().await {
                                    if let Some(msgs) = body.get("data").and_then(|d| d.as_array()) {
                                        let parsed = parse_messages(msgs);
                                        for (sender, text, chat_guid, is_group) in parsed {
                                            let chat_type = if is_group {
                                                ChatType::Group
                                            } else {
                                                ChatType::Dm
                                            };
                                            let msg = InboundMessage {
                                                channel: "bluebubbles".into(),
                                                account_id: chat_guid,
                                                chat_type,
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

                                        // Update last_timestamp from the newest message
                                        if let Some(last) = msgs.last() {
                                            if let Some(ts) = last.get("dateCreated").and_then(|v| v.as_i64()) {
                                                last_timestamp = ts;
                                            }
                                        }
                                    }
                                }
                            }
                            Ok(resp) => {
                                warn!(status = %resp.status(), "BlueBubbles poll non-success");
                            }
                            Err(e) => {
                                warn!(%e, "BlueBubbles poll error");
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

        let client = reqwest::Client::new();
        let resp = client
            .post(format!("{}/api/v1/message/text", self.api_url))
            .json(&serde_json::json!({
                "chatGuid": target.chat_id,
                "message": text,
                "method": "apple-script",
                "tempGuid": uuid::Uuid::new_v4().to_string(),
            }))
            .query(&[("password", &self.password)])
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
                error!(%status, "BlueBubbles send failed");
                Ok(SendResult {
                    message_id: None,
                    success: false,
                    error: Some(format!("BlueBubbles API error {status}")),
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
            account_id: None,
            display_name: Some("iMessage (BlueBubbles)".into()),
            error: None,
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_polling_response_parsing() {
        let messages = vec![serde_json::json!({
            "text": "Hey there!",
            "isFromMe": false,
            "handle": { "address": "+15551234567" },
            "chats": [{ "guid": "iMessage;-;+15551234567", "participants": [] }],
            "dateCreated": 1700000000000_i64
        })];

        let parsed = parse_messages(&messages);
        assert_eq!(parsed.len(), 1);
        assert_eq!(parsed[0].0, "+15551234567");
        assert_eq!(parsed[0].1, "Hey there!");
        assert_eq!(parsed[0].2, "iMessage;-;+15551234567");
        assert!(!parsed[0].3); // not a group
    }

    #[test]
    fn test_send_payload() {
        let channel = BlueBubblesChannel::new(
            "http://localhost:1234".into(),
            "password123".into(),
        );
        assert_eq!(channel.id(), "bluebubbles");
        assert_eq!(channel.capabilities().max_message_length, None);
    }
}
