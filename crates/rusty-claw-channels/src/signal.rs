//! Signal channel implementation via signal-cli REST bridge.

use async_trait::async_trait;
use tokio::sync::mpsc;
use tracing::{error, info, warn};

use rusty_claw_core::types::{
    ChatType, InboundMessage, OutboundMessage, SendResult, SendTarget, Sender,
};

use crate::{
    Channel, ChannelCapabilities, ChannelHandle, ChannelMeta, ChannelStatus, InboundReceiver,
};

pub struct SignalChannel {
    api_url: String,
    phone_number: String,
    poll_interval_ms: u64,
}

impl SignalChannel {
    pub fn new(api_url: String, phone_number: String, poll_interval_ms: u64) -> Self {
        Self {
            api_url,
            phone_number,
            poll_interval_ms,
        }
    }
}

/// Parse signal-cli REST envelope response into (sender, text) pairs.
pub fn parse_envelopes(envelopes: &[serde_json::Value]) -> Vec<(String, String, Option<String>)> {
    let mut messages = Vec::new();

    for envelope in envelopes {
        let source = envelope
            .get("envelope")
            .or(Some(envelope))
            .and_then(|e| e.get("source").or(e.get("sourceNumber")))
            .and_then(|v| v.as_str())
            .unwrap_or("")
            .to_string();

        let data_message = envelope
            .get("envelope")
            .or(Some(envelope))
            .and_then(|e| e.get("dataMessage"));

        if let Some(dm) = data_message {
            let text = dm
                .get("message")
                .and_then(|v| v.as_str())
                .unwrap_or("")
                .to_string();
            let group_id = dm
                .get("groupInfo")
                .and_then(|g| g.get("groupId"))
                .and_then(|v| v.as_str())
                .map(|s| s.to_string());

            if !source.is_empty() && !text.is_empty() {
                messages.push((source, text, group_id));
            }
        }
    }

    messages
}

#[async_trait]
impl Channel for SignalChannel {
    fn id(&self) -> &str {
        "signal"
    }

    fn meta(&self) -> ChannelMeta {
        ChannelMeta {
            label: "Signal".into(),
            description: "Signal messaging via signal-cli REST".into(),
            docs_url: Some("https://github.com/bbernhard/signal-cli-rest-api".into()),
            icon: None,
        }
    }

    fn capabilities(&self) -> ChannelCapabilities {
        ChannelCapabilities {
            chat_types: vec![ChatType::Dm, ChatType::Group],
            supports_media: true,
            supports_reactions: false,
            supports_threads: false,
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
        let (shutdown_tx, mut shutdown_rx) = tokio::sync::oneshot::channel();

        let api_url = self.api_url.clone();
        let phone = self.phone_number.clone();
        let interval = self.poll_interval_ms;

        tokio::spawn(async move {
            info!("Signal channel started, polling every {}ms", interval);
            let client = reqwest::Client::new();

            loop {
                tokio::select! {
                    _ = &mut shutdown_rx => {
                        info!("Signal channel stopped");
                        break;
                    }
                    _ = tokio::time::sleep(std::time::Duration::from_millis(interval)) => {
                        let url = format!("{api_url}/v1/receive/{phone}");
                        match client.get(&url).send().await {
                            Ok(resp) if resp.status().is_success() => {
                                if let Ok(envelopes) = resp.json::<Vec<serde_json::Value>>().await {
                                    let parsed = parse_envelopes(&envelopes);
                                    for (sender, text, group_id) in parsed {
                                        let chat_type = if group_id.is_some() {
                                            ChatType::Group
                                        } else {
                                            ChatType::Dm
                                        };
                                        let chat_id = group_id.unwrap_or_else(|| sender.clone());
                                        let msg = InboundMessage {
                                            channel: "signal".into(),
                                            account_id: chat_id,
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
                            }
                            Ok(resp) => {
                                warn!(status = %resp.status(), "Signal poll non-success");
                            }
                            Err(e) => {
                                warn!(%e, "Signal poll error");
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
        let payload = serde_json::json!({
            "message": text,
            "number": self.phone_number,
            "recipients": [target.chat_id],
        });

        let resp = client
            .post(format!("{}/v2/send", self.api_url))
            .json(&payload)
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
                error!(%status, "Signal send failed");
                Ok(SendResult {
                    message_id: None,
                    success: false,
                    error: Some(format!("Signal API error {status}")),
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
            account_id: Some(self.phone_number.clone()),
            display_name: Some("Signal".into()),
            error: None,
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_config_resolve() {
        unsafe { std::env::set_var("TEST_SIGNAL_PHONE_RC", "+1234567890") };
        let config = rusty_claw_core::config::SignalConfig {
            api_url: "http://localhost:8080".into(),
            phone_number: None,
            phone_number_env: Some("TEST_SIGNAL_PHONE_RC".into()),
            poll_interval_ms: 2000,
        };
        assert_eq!(config.resolve_phone_number(), Some("+1234567890".into()));
        unsafe { std::env::remove_var("TEST_SIGNAL_PHONE_RC") };
    }

    #[test]
    fn test_envelope_parsing() {
        let envelopes = vec![serde_json::json!({
            "envelope": {
                "source": "+15551234567",
                "dataMessage": {
                    "message": "Hello from Signal!",
                    "timestamp": 1234567890
                }
            }
        })];

        let parsed = parse_envelopes(&envelopes);
        assert_eq!(parsed.len(), 1);
        assert_eq!(parsed[0].0, "+15551234567");
        assert_eq!(parsed[0].1, "Hello from Signal!");
        assert!(parsed[0].2.is_none());
    }

    #[test]
    fn test_send_payload() {
        let channel = SignalChannel::new(
            "http://localhost:8080".into(),
            "+1000000000".into(),
            2000,
        );
        assert_eq!(channel.id(), "signal");
        assert_eq!(
            channel.capabilities().max_message_length,
            Some(4096)
        );
    }
}
