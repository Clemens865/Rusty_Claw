//! Microsoft Teams (Bot Framework) channel implementation.

use async_trait::async_trait;
use tokio::sync::mpsc;
use tracing::{error, info};

use rusty_claw_core::types::{
    ChatType, InboundMessage, OutboundMessage, SendResult, SendTarget, Sender,
};

use crate::{
    Channel, ChannelCapabilities, ChannelHandle, ChannelMeta, ChannelStatus, InboundReceiver,
};

pub struct MsTeamsChannel {
    app_id: String,
    app_password: String,
    webhook_port: u16,
}

impl MsTeamsChannel {
    pub fn new(app_id: String, app_password: String, webhook_port: u16) -> Self {
        Self {
            app_id,
            app_password,
            webhook_port,
        }
    }
}

/// Format OAuth2 token request body.
pub fn format_oauth_body(app_id: &str, app_password: &str) -> String {
    format!(
        "grant_type=client_credentials&client_id={}&client_secret={}&scope=https%3A%2F%2Fapi.botframework.com%2F.default",
        urlencoding::encode(app_id),
        urlencoding::encode(app_password)
    )
}

/// Parse a Bot Framework Activity into (sender, text, conversation_id, service_url).
pub fn parse_activity(
    activity: &serde_json::Value,
) -> Option<(String, String, String, String)> {
    let activity_type = activity
        .get("type")
        .and_then(|v| v.as_str())
        .unwrap_or("");

    if activity_type != "message" {
        return None;
    }

    let text = activity
        .get("text")
        .and_then(|v| v.as_str())
        .unwrap_or("")
        .to_string();

    let sender = activity
        .get("from")
        .and_then(|f| f.get("id"))
        .and_then(|v| v.as_str())
        .unwrap_or("unknown")
        .to_string();

    let conversation_id = activity
        .get("conversation")
        .and_then(|c| c.get("id"))
        .and_then(|v| v.as_str())
        .unwrap_or("")
        .to_string();

    let service_url = activity
        .get("serviceUrl")
        .and_then(|v| v.as_str())
        .unwrap_or("")
        .to_string();

    if text.is_empty() || conversation_id.is_empty() {
        return None;
    }

    Some((sender, text, conversation_id, service_url))
}

#[async_trait]
impl Channel for MsTeamsChannel {
    fn id(&self) -> &str {
        "msteams"
    }

    fn meta(&self) -> ChannelMeta {
        ChannelMeta {
            label: "Microsoft Teams".into(),
            description: "Microsoft Teams via Bot Framework".into(),
            docs_url: Some(
                "https://learn.microsoft.com/en-us/microsoftteams/platform/bots".into(),
            ),
            icon: None,
        }
    }

    fn capabilities(&self) -> ChannelCapabilities {
        ChannelCapabilities {
            chat_types: vec![ChatType::Dm, ChatType::Group],
            supports_media: false,
            supports_reactions: false,
            supports_threads: true,
            supports_typing: true,
            supports_read_receipts: false,
            supports_polls: false,
            max_message_length: Some(28000),
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
            info!(port, "MS Teams webhook listener starting");

            let app = axum::Router::new().route(
                "/api/messages",
                axum::routing::post(move |body: axum::body::Bytes| {
                    let inbound_tx = inbound_tx.clone();
                    async move {
                        if let Ok(activity) =
                            serde_json::from_slice::<serde_json::Value>(&body)
                        {
                            if let Some((sender, text, conversation_id, _service_url)) =
                                parse_activity(&activity)
                            {
                                let chat_type = activity
                                    .get("conversation")
                                    .and_then(|c| c.get("conversationType"))
                                    .and_then(|v| v.as_str())
                                    .map(|t| {
                                        if t == "personal" {
                                            ChatType::Dm
                                        } else {
                                            ChatType::Group
                                        }
                                    })
                                    .unwrap_or(ChatType::Dm);

                                let msg = InboundMessage {
                                    channel: "msteams".into(),
                                    account_id: conversation_id,
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
                        }
                        axum::http::StatusCode::OK
                    }
                }),
            );

            let listener =
                match tokio::net::TcpListener::bind(format!("0.0.0.0:{port}")).await {
                    Ok(l) => l,
                    Err(e) => {
                        error!(%e, "Failed to bind MS Teams webhook port");
                        return;
                    }
                };

            tokio::select! {
                _ = axum::serve(listener, app) => {}
                _ = shutdown_rx => {
                    info!("MS Teams channel stopped");
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

        // Get OAuth token first
        let client = reqwest::Client::new();
        let token_resp = client
            .post("https://login.microsoftonline.com/botframework.com/oauth2/v2.0/token")
            .header("Content-Type", "application/x-www-form-urlencoded")
            .body(format_oauth_body(&self.app_id, &self.app_password))
            .send()
            .await;

        let token = match token_resp {
            Ok(r) if r.status().is_success() => {
                let body: serde_json::Value = r.json().await.unwrap_or_default();
                body.get("access_token")
                    .and_then(|v| v.as_str())
                    .unwrap_or("")
                    .to_string()
            }
            Ok(r) => {
                let status = r.status();
                return Ok(SendResult {
                    message_id: None,
                    success: false,
                    error: Some(format!("OAuth error {status}")),
                });
            }
            Err(e) => {
                return Ok(SendResult {
                    message_id: None,
                    success: false,
                    error: Some(format!("OAuth request failed: {e}")),
                });
            }
        };

        // Send message via Bot Framework REST API
        let service_url = "https://smba.trafficmanager.net/teams";

        let url = format!(
            "{}/v3/conversations/{}/activities",
            service_url, target.chat_id
        );

        let resp = client
            .post(&url)
            .header("Authorization", format!("Bearer {token}"))
            .json(&serde_json::json!({
                "type": "message",
                "text": text,
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
                error!(%status, "Teams send failed");
                Ok(SendResult {
                    message_id: None,
                    success: false,
                    error: Some(format!("Teams API error {status}")),
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
            account_id: Some(self.app_id.clone()),
            display_name: Some("Microsoft Teams".into()),
            error: None,
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_oauth_formatting() {
        let body = format_oauth_body("my-app-id", "my-secret");
        assert!(body.contains("client_id=my-app-id"));
        assert!(body.contains("client_secret=my-secret"));
        assert!(body.contains("grant_type=client_credentials"));
    }

    #[test]
    fn test_activity_parsing() {
        let activity = serde_json::json!({
            "type": "message",
            "text": "Hello Teams!",
            "from": { "id": "user-123", "name": "Test User" },
            "conversation": { "id": "conv-456", "conversationType": "personal" },
            "serviceUrl": "https://smba.trafficmanager.net/teams"
        });

        let parsed = parse_activity(&activity);
        assert!(parsed.is_some());
        let (sender, text, conv_id, service_url) = parsed.unwrap();
        assert_eq!(sender, "user-123");
        assert_eq!(text, "Hello Teams!");
        assert_eq!(conv_id, "conv-456");
        assert_eq!(service_url, "https://smba.trafficmanager.net/teams");
    }

    #[test]
    fn test_send_payload() {
        let channel = MsTeamsChannel::new(
            "app-id".into(),
            "password".into(),
            3103,
        );
        assert_eq!(channel.id(), "msteams");
        assert_eq!(channel.capabilities().max_message_length, Some(28000));
    }
}
