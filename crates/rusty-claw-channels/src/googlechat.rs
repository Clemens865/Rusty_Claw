//! Google Chat (Workspace) channel implementation.

use async_trait::async_trait;
use tokio::sync::mpsc;
use tracing::{error, info, warn};

use rusty_claw_core::types::{
    ChatType, InboundMessage, OutboundMessage, SendResult, SendTarget, Sender,
};

use crate::{
    Channel, ChannelCapabilities, ChannelHandle, ChannelMeta, ChannelStatus, InboundReceiver,
};

pub struct GoogleChatChannel {
    project_id: String,
    _service_account_json: Option<String>,
    webhook_port: u16,
}

impl GoogleChatChannel {
    pub fn new(project_id: String, service_account_json: Option<String>, webhook_port: u16) -> Self {
        Self {
            project_id,
            _service_account_json: service_account_json,
            webhook_port,
        }
    }
}

/// Parse a Google Chat event into (sender, text, space_name).
pub fn parse_chat_event(
    event: &serde_json::Value,
) -> Option<(String, String, String)> {
    let event_type = event.get("type").and_then(|v| v.as_str()).unwrap_or("");
    if event_type != "MESSAGE" {
        return None;
    }

    let message = event.get("message")?;
    let text = message
        .get("argumentText")
        .or(message.get("text"))
        .and_then(|v| v.as_str())
        .unwrap_or("")
        .trim()
        .to_string();

    let sender = message
        .get("sender")
        .and_then(|s| s.get("name"))
        .and_then(|v| v.as_str())
        .unwrap_or("unknown")
        .to_string();

    let space = event
        .get("space")
        .and_then(|s| s.get("name"))
        .and_then(|v| v.as_str())
        .unwrap_or("")
        .to_string();

    if text.is_empty() {
        return None;
    }

    Some((sender, text, space))
}

#[async_trait]
impl Channel for GoogleChatChannel {
    fn id(&self) -> &str {
        "googlechat"
    }

    fn meta(&self) -> ChannelMeta {
        ChannelMeta {
            label: "Google Chat".into(),
            description: "Google Workspace Chat integration".into(),
            docs_url: Some("https://developers.google.com/workspace/chat".into()),
            icon: None,
        }
    }

    fn capabilities(&self) -> ChannelCapabilities {
        ChannelCapabilities {
            chat_types: vec![ChatType::Dm, ChatType::Group],
            supports_media: false,
            supports_reactions: false,
            supports_threads: true,
            supports_typing: false,
            supports_read_receipts: false,
            supports_polls: false,
            max_message_length: Some(4096),
        }
    }

    async fn start(
        &self,
        _config: &serde_json::Value,
    ) -> anyhow::Result<(InboundReceiver, ChannelHandle)> {
        let (inbound_tx, inbound_rx) = mpsc::unbounded_channel::<InboundMessage>();
        let (shutdown_tx, shutdown_rx) = tokio::sync::oneshot::channel();

        let port = self.webhook_port;

        tokio::spawn(async move {
            info!(port, "Google Chat webhook listener starting");

            let app = axum::Router::new().route(
                "/",
                axum::routing::post(
                    move |body: axum::body::Bytes| {
                        let inbound_tx = inbound_tx.clone();
                        async move {
                            if let Ok(event) =
                                serde_json::from_slice::<serde_json::Value>(&body)
                            {
                                if let Some((sender, text, space)) =
                                    parse_chat_event(&event)
                                {
                                    let thread_id = event
                                        .get("message")
                                        .and_then(|m| m.get("thread"))
                                        .and_then(|t| t.get("name"))
                                        .and_then(|v| v.as_str())
                                        .map(String::from);

                                    let msg = InboundMessage {
                                        channel: "googlechat".into(),
                                        account_id: space,
                                        chat_type: ChatType::Group,
                                        sender: Sender {
                                            id: sender,
                                            display_name: None,
                                            username: None,
                                        },
                                        text: Some(text),
                                        media: vec![],
                                        reply_to: None,
                                        thread_id,
                                        timestamp: chrono::Utc::now(),
                                        raw: None,
                                    };
                                    let _ = inbound_tx.send(msg);
                                }
                            }
                            ""
                        }
                    },
                ),
            );

            let listener =
                match tokio::net::TcpListener::bind(format!("0.0.0.0:{port}")).await {
                    Ok(l) => l,
                    Err(e) => {
                        error!(%e, "Failed to bind Google Chat webhook port");
                        return;
                    }
                };

            tokio::select! {
                _ = axum::serve(listener, app) => {}
                _ = shutdown_rx => {
                    info!("Google Chat channel stopped");
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

        // Google Chat API requires OAuth2 token from service account
        // For now, send via REST assuming token is available
        let client = reqwest::Client::new();
        let url = format!(
            "https://chat.googleapis.com/v1/{}/messages",
            target.chat_id
        );

        let resp = client
            .post(&url)
            .json(&serde_json::json!({ "text": text }))
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
                warn!(%status, "Google Chat send failed");
                Ok(SendResult {
                    message_id: None,
                    success: false,
                    error: Some(format!("Google Chat API error {status}")),
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
            account_id: Some(self.project_id.clone()),
            display_name: Some("Google Chat".into()),
            error: None,
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_event_parsing() {
        let event = serde_json::json!({
            "type": "MESSAGE",
            "message": {
                "text": "Hello from Chat",
                "sender": { "name": "users/123" },
                "thread": { "name": "spaces/abc/threads/xyz" }
            },
            "space": { "name": "spaces/abc" }
        });

        let parsed = parse_chat_event(&event);
        assert!(parsed.is_some());
        let (sender, text, space) = parsed.unwrap();
        assert_eq!(sender, "users/123");
        assert_eq!(text, "Hello from Chat");
        assert_eq!(space, "spaces/abc");
    }

    #[test]
    fn test_config_resolution() {
        let config = rusty_claw_core::config::GoogleChatConfig {
            service_account_json: Some("{}".into()),
            project_id: Some("my-project".into()),
            webhook_port: 3102,
        };
        assert_eq!(config.webhook_port, 3102);
        assert_eq!(config.project_id, Some("my-project".into()));
    }
}
