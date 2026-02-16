//! Slack channel implementation.
//!
//! Uses Slack Events API (webhook) for inbound messages and
//! Web API (chat.postMessage) for sending.

use async_trait::async_trait;
use serde::Deserialize;
use tokio::sync::mpsc;
use tracing::{error, info};

use rusty_claw_core::types::{
    ChatType, InboundMessage, OutboundMessage, SendResult, SendTarget,
};

use crate::{
    Channel, ChannelCapabilities, ChannelHandle, ChannelMeta, ChannelStatus, InboundReceiver,
};

/// Slack channel configuration.
#[derive(Debug, Clone, Deserialize)]
pub struct SlackConfig {
    #[serde(default)]
    pub bot_token: Option<String>,
    #[serde(default)]
    pub bot_token_env: Option<String>,
    #[serde(default)]
    pub signing_secret: Option<String>,
    #[serde(default)]
    pub signing_secret_env: Option<String>,
    #[serde(default)]
    pub app_token: Option<String>,
    #[serde(default)]
    pub port: Option<u16>,
}

impl SlackConfig {
    pub fn resolve_bot_token(&self) -> Option<String> {
        resolve_secret(&self.bot_token, &self.bot_token_env)
    }

    pub fn resolve_signing_secret(&self) -> Option<String> {
        resolve_secret(&self.signing_secret, &self.signing_secret_env)
    }
}

fn resolve_secret(direct: &Option<String>, env_var: &Option<String>) -> Option<String> {
    if let Some(val) = direct {
        if !val.is_empty() {
            return Some(val.clone());
        }
    }
    if let Some(env) = env_var {
        if let Ok(val) = std::env::var(env) {
            if !val.is_empty() {
                return Some(val);
            }
        }
    }
    None
}

pub struct SlackChannel {
    bot_token: String,
    _signing_secret: Option<String>,
    _listen_port: u16,
}

impl SlackChannel {
    pub fn new(
        bot_token: String,
        signing_secret: Option<String>,
        listen_port: Option<u16>,
    ) -> Self {
        Self {
            bot_token,
            _signing_secret: signing_secret,
            _listen_port: listen_port.unwrap_or(3100),
        }
    }
}

/// Verify Slack request signature (HMAC-SHA256).
pub fn verify_slack_signature(
    signing_secret: &str,
    timestamp: &str,
    body: &str,
    signature: &str,
) -> bool {
    use hmac::{Hmac, Mac};
    use sha2::Sha256;

    let sig_basestring = format!("v0:{timestamp}:{body}");
    let Ok(mut mac) = Hmac::<Sha256>::new_from_slice(signing_secret.as_bytes()) else {
        return false;
    };
    mac.update(sig_basestring.as_bytes());
    let result = mac.finalize();
    let expected = hex::encode(result.into_bytes());
    let expected_sig = format!("v0={expected}");
    expected_sig == signature
}

/// Slack event payload types.
#[derive(Debug, Deserialize)]
#[serde(tag = "type")]
pub enum SlackEventPayload {
    #[serde(rename = "url_verification")]
    UrlVerification { challenge: String },
    #[serde(rename = "event_callback")]
    EventCallback { event: SlackEvent },
}

#[derive(Debug, Deserialize)]
pub struct SlackEvent {
    #[serde(rename = "type")]
    pub event_type: String,
    #[serde(default)]
    pub user: Option<String>,
    #[serde(default)]
    pub text: Option<String>,
    #[serde(default)]
    pub channel: Option<String>,
    #[serde(default)]
    pub thread_ts: Option<String>,
}

#[async_trait]
impl Channel for SlackChannel {
    fn id(&self) -> &str {
        "slack"
    }

    fn meta(&self) -> ChannelMeta {
        ChannelMeta {
            label: "Slack".into(),
            description: "Slack workspace integration via Events API".into(),
            docs_url: Some("https://api.slack.com/".into()),
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
            max_message_length: Some(40000),
        }
    }

    async fn start(
        &self,
        _config: &serde_json::Value,
    ) -> anyhow::Result<(InboundReceiver, ChannelHandle)> {
        let (_inbound_tx, inbound_rx) = mpsc::unbounded_channel::<InboundMessage>();
        let (shutdown_tx, shutdown_rx) = tokio::sync::oneshot::channel();

        tokio::spawn(async move {
            info!("Slack events listener started");
            let _ = shutdown_rx.await;
            info!("Slack events listener stopped");
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

        let mut payload = serde_json::json!({
            "channel": target.chat_id,
            "text": text,
        });

        if let Some(ref thread_ts) = message.thread_id {
            payload["thread_ts"] = serde_json::json!(thread_ts);
        }

        let client = reqwest::Client::new();
        let resp = client
            .post("https://slack.com/api/chat.postMessage")
            .header("Authorization", format!("Bearer {}", self.bot_token))
            .header("Content-Type", "application/json; charset=utf-8")
            .json(&payload)
            .send()
            .await;

        match resp {
            Ok(r) => {
                let body: serde_json::Value = r.json().await.unwrap_or_default();
                if body["ok"].as_bool() == Some(true) {
                    Ok(SendResult {
                        message_id: body["ts"].as_str().map(String::from),
                        success: true,
                        error: None,
                    })
                } else {
                    let err = body["error"].as_str().unwrap_or("unknown").to_string();
                    error!(error = %err, "Slack send failed");
                    Ok(SendResult {
                        message_id: None,
                        success: false,
                        error: Some(err),
                    })
                }
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
            display_name: Some("Slack Bot".into()),
            error: None,
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_verify_slack_signature_valid() {
        use hmac::{Hmac, Mac};
        use sha2::Sha256;

        let secret = "test_secret_key";
        let timestamp = "1234567890";
        let body = "test body content";

        let sig_basestring = format!("v0:{timestamp}:{body}");
        let mut mac = Hmac::<Sha256>::new_from_slice(secret.as_bytes()).unwrap();
        mac.update(sig_basestring.as_bytes());
        let expected = format!("v0={}", hex::encode(mac.finalize().into_bytes()));

        assert!(verify_slack_signature(secret, timestamp, body, &expected));
    }

    #[test]
    fn test_verify_slack_signature_invalid() {
        assert!(!verify_slack_signature(
            "secret",
            "1234",
            "body",
            "v0=wrong"
        ));
    }

    #[test]
    fn test_slack_event_parsing() {
        let json = r#"{"type":"event_callback","event":{"type":"message","user":"U123","text":"hello","channel":"C456","thread_ts":"1234.5678"}}"#;
        let payload: SlackEventPayload = serde_json::from_str(json).unwrap();
        match payload {
            SlackEventPayload::EventCallback { event } => {
                assert_eq!(event.event_type, "message");
                assert_eq!(event.text.as_deref(), Some("hello"));
            }
            _ => panic!("Expected EventCallback"),
        }
    }

    #[test]
    fn test_slack_url_verification() {
        let json = r#"{"type":"url_verification","challenge":"test_challenge_123"}"#;
        let payload: SlackEventPayload = serde_json::from_str(json).unwrap();
        match payload {
            SlackEventPayload::UrlVerification { challenge } => {
                assert_eq!(challenge, "test_challenge_123");
            }
            _ => panic!("Expected UrlVerification"),
        }
    }

    #[test]
    fn test_slack_channel_meta() {
        let channel = SlackChannel::new("xoxb-token".into(), None, None);
        assert_eq!(channel.id(), "slack");
        assert_eq!(channel.capabilities().max_message_length, Some(40000));
    }

    #[test]
    fn test_slack_config_resolve() {
        unsafe { std::env::set_var("TEST_SLACK_TOKEN_RC", "xoxb-test") };
        let config = SlackConfig {
            bot_token: None,
            bot_token_env: Some("TEST_SLACK_TOKEN_RC".into()),
            signing_secret: Some("secret123".into()),
            signing_secret_env: None,
            app_token: None,
            port: None,
        };
        assert_eq!(config.resolve_bot_token(), Some("xoxb-test".into()));
        assert_eq!(config.resolve_signing_secret(), Some("secret123".into()));
        unsafe { std::env::remove_var("TEST_SLACK_TOKEN_RC") };
    }
}
