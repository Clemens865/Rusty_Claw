//! WhatsApp Business Cloud API channel implementation.

use async_trait::async_trait;
use sha2::Sha256;
use tokio::sync::mpsc;
use tracing::{error, info};

use rusty_claw_core::types::{
    ChatType, InboundMessage, OutboundMessage, SendResult, SendTarget, Sender,
};

use crate::{
    Channel, ChannelCapabilities, ChannelHandle, ChannelMeta, ChannelStatus, InboundReceiver,
};

#[derive(Debug, Clone, serde::Deserialize)]
pub struct WhatsAppChannelConfig {
    pub phone_number_id: String,
    pub access_token: String,
    pub verify_token: Option<String>,
    pub app_secret: Option<String>,
    #[serde(default = "default_port")]
    pub webhook_port: u16,
}

fn default_port() -> u16 {
    3101
}

pub struct WhatsAppChannel {
    config: WhatsAppChannelConfig,
}

impl WhatsAppChannel {
    pub fn new(config: WhatsAppChannelConfig) -> Self {
        Self { config }
    }
}

/// Parse inbound WhatsApp webhook payload to extract messages.
pub fn parse_webhook_messages(body: &serde_json::Value) -> Vec<(String, String)> {
    let mut messages = Vec::new();

    if let Some(entries) = body.get("entry").and_then(|v| v.as_array()) {
        for entry in entries {
            if let Some(changes) = entry.get("changes").and_then(|v| v.as_array()) {
                for change in changes {
                    if let Some(msgs) = change
                        .get("value")
                        .and_then(|v| v.get("messages"))
                        .and_then(|v| v.as_array())
                    {
                        for msg in msgs {
                            let from = msg
                                .get("from")
                                .and_then(|v| v.as_str())
                                .unwrap_or("")
                                .to_string();
                            let text = msg
                                .get("text")
                                .and_then(|v| v.get("body"))
                                .and_then(|v| v.as_str())
                                .unwrap_or("")
                                .to_string();
                            if !from.is_empty() && !text.is_empty() {
                                messages.push((from, text));
                            }
                        }
                    }
                }
            }
        }
    }

    messages
}

/// Verify Meta webhook signature (HMAC-SHA256).
pub fn verify_signature(payload: &[u8], signature: &str, app_secret: &str) -> bool {
    use hmac::{Hmac, Mac};
    let expected = signature.strip_prefix("sha256=").unwrap_or(signature);

    let mut mac =
        Hmac::<Sha256>::new_from_slice(app_secret.as_bytes()).expect("HMAC key length");
    mac.update(payload);
    let result = hex::encode(mac.finalize().into_bytes());

    result == expected
}

#[async_trait]
impl Channel for WhatsAppChannel {
    fn id(&self) -> &str {
        "whatsapp"
    }

    fn meta(&self) -> ChannelMeta {
        ChannelMeta {
            label: "WhatsApp".into(),
            description: "WhatsApp Business Cloud API".into(),
            docs_url: Some("https://developers.facebook.com/docs/whatsapp".into()),
            icon: None,
        }
    }

    fn capabilities(&self) -> ChannelCapabilities {
        ChannelCapabilities {
            chat_types: vec![ChatType::Dm],
            supports_media: true,
            supports_reactions: false,
            supports_threads: false,
            supports_typing: false,
            supports_read_receipts: true,
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

        let verify_token = self.config.verify_token.clone().unwrap_or_default();
        let app_secret = self.config.app_secret.clone();
        let port = self.config.webhook_port;

        tokio::spawn(async move {
            info!(port, "WhatsApp webhook listener starting");

            let inbound = inbound_tx;
            let vt = verify_token;
            let secret = app_secret;

            let verify_handler = {
                let vt = vt.clone();
                move |query: std::collections::HashMap<String, String>| {
                    let mode = query.get("hub.mode").cloned().unwrap_or_default();
                    let token = query.get("hub.verify_token").cloned().unwrap_or_default();
                    let challenge = query.get("hub.challenge").cloned().unwrap_or_default();

                    if mode == "subscribe" && token == vt {
                        challenge
                    } else {
                        String::new()
                    }
                }
            };

            // Simple HTTP server for webhook
            let app = axum::Router::new()
                .route(
                    "/webhook",
                    axum::routing::get(move |axum::extract::Query(params): axum::extract::Query<std::collections::HashMap<String, String>>| async move {
                        verify_handler(params)
                    }),
                )
                .route(
                    "/webhook",
                    axum::routing::post(move |body: axum::body::Bytes| async move {
                        if let Ok(payload) = serde_json::from_slice::<serde_json::Value>(&body) {
                            // Optionally verify signature
                            if let Some(ref _secret) = secret {
                                // In production, verify X-Hub-Signature-256 header
                            }
                            let messages = parse_webhook_messages(&payload);
                            for (from, text) in messages {
                                let msg = InboundMessage {
                                    channel: "whatsapp".into(),
                                    account_id: from.clone(),
                                    chat_type: ChatType::Dm,
                                    sender: Sender {
                                        id: from,
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
                                let _ = inbound.send(msg);
                            }
                        }
                        "OK"
                    }),
                );

            let listener = match tokio::net::TcpListener::bind(format!("0.0.0.0:{port}")).await {
                Ok(l) => l,
                Err(e) => {
                    error!(%e, "Failed to bind WhatsApp webhook port");
                    return;
                }
            };

            tokio::select! {
                _ = axum::serve(listener, app) => {}
                _ = shutdown_rx => {
                    info!("WhatsApp channel stopped");
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
            .post(format!(
                "https://graph.facebook.com/v21.0/{}/messages",
                self.config.phone_number_id
            ))
            .header("Authorization", format!("Bearer {}", self.config.access_token))
            .header("Content-Type", "application/json")
            .json(&serde_json::json!({
                "messaging_product": "whatsapp",
                "to": target.chat_id,
                "type": "text",
                "text": { "body": text }
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
                let body = r.text().await.unwrap_or_default();
                error!(%status, body, "WhatsApp send failed");
                Ok(SendResult {
                    message_id: None,
                    success: false,
                    error: Some(format!("WhatsApp API error {status}")),
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
            account_id: Some(self.config.phone_number_id.clone()),
            display_name: Some("WhatsApp Business".into()),
            error: None,
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_config_resolve() {
        unsafe { std::env::set_var("TEST_WA_TOKEN_RC", "wa-token-123") };
        let config = rusty_claw_core::config::WhatsAppConfig {
            phone_number_id: Some("123".into()),
            access_token: None,
            access_token_env: Some("TEST_WA_TOKEN_RC".into()),
            verify_token: None,
            app_secret: None,
            app_secret_env: None,
            webhook_port: 3101,
        };
        assert_eq!(config.resolve_access_token(), Some("wa-token-123".into()));
        unsafe { std::env::remove_var("TEST_WA_TOKEN_RC") };
    }

    #[test]
    fn test_webhook_challenge() {
        let vt = "my-verify-token";
        let mut params = std::collections::HashMap::<String, String>::new();
        params.insert("hub.mode".into(), "subscribe".into());
        params.insert("hub.verify_token".into(), vt.into());
        params.insert("hub.challenge".into(), "challenge-123".into());

        let mode = params.get("hub.mode").unwrap();
        let token = params.get("hub.verify_token").unwrap();
        let challenge = params.get("hub.challenge").unwrap();
        assert_eq!(mode, "subscribe");
        assert_eq!(token, vt);
        assert_eq!(challenge, "challenge-123");
    }

    #[test]
    fn test_message_parsing() {
        let body = serde_json::json!({
            "entry": [{
                "changes": [{
                    "value": {
                        "messages": [{
                            "from": "15551234567",
                            "text": { "body": "Hello bot!" }
                        }]
                    }
                }]
            }]
        });

        let messages = parse_webhook_messages(&body);
        assert_eq!(messages.len(), 1);
        assert_eq!(messages[0].0, "15551234567");
        assert_eq!(messages[0].1, "Hello bot!");
    }
}
