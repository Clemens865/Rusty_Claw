use chrono::{DateTime, Utc};
use serde::{Deserialize, Serialize};

/// Chat type classification.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum ChatType {
    Dm,
    Group,
    Channel,
    Thread,
}

/// Sender identity from a channel.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Sender {
    pub id: String,
    pub display_name: Option<String>,
    pub username: Option<String>,
}

/// Media attachment from an inbound message.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct MediaAttachment {
    pub url: Option<String>,
    pub data: Option<Vec<u8>>,
    pub mime_type: String,
    pub filename: Option<String>,
    pub size_bytes: Option<u64>,
}

/// Inbound message from any channel.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct InboundMessage {
    pub channel: String,
    pub account_id: String,
    pub chat_type: ChatType,
    pub sender: Sender,
    pub text: Option<String>,
    pub media: Vec<MediaAttachment>,
    pub reply_to: Option<String>,
    pub thread_id: Option<String>,
    pub timestamp: DateTime<Utc>,
    /// Platform-specific raw payload for channel-specific processing.
    pub raw: Option<serde_json::Value>,
}

/// Outbound message to send via a channel.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct OutboundMessage {
    pub text: Option<String>,
    pub media: Vec<MediaAttachment>,
    pub reply_to: Option<String>,
    pub thread_id: Option<String>,
}

/// Target for sending a message.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct SendTarget {
    pub channel: String,
    pub account_id: String,
    pub chat_id: String,
    pub chat_type: ChatType,
}

/// Result of sending a message.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct SendResult {
    pub message_id: Option<String>,
    pub success: bool,
    pub error: Option<String>,
}

/// LLM thinking/reasoning level.
#[derive(Debug, Clone, Copy, Default, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum ThinkingLevel {
    Off,
    Minimal,
    #[default]
    Low,
    Medium,
    High,
    XHigh,
}

/// Content block in a message (text, image, tool_use, tool_result).
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(tag = "type")]
pub enum ContentBlock {
    #[serde(rename = "text")]
    Text { text: String },
    #[serde(rename = "image")]
    Image {
        source: ImageSource,
    },
    #[serde(rename = "tool_use")]
    ToolUse {
        id: String,
        name: String,
        input: serde_json::Value,
    },
    #[serde(rename = "tool_result")]
    ToolResult {
        tool_use_id: String,
        content: String,
        is_error: bool,
    },
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ImageSource {
    #[serde(rename = "type")]
    pub source_type: String,
    pub media_type: String,
    pub data: String,
}

impl InboundMessage {
    /// Create an `InboundMessage` from plain text (for CLI one-shot mode).
    pub fn from_cli_text(text: &str) -> Self {
        Self {
            channel: "cli".into(),
            account_id: "local".into(),
            chat_type: ChatType::Dm,
            sender: Sender {
                id: "local-user".into(),
                display_name: Some("User".into()),
                username: None,
            },
            text: Some(text.to_string()),
            media: Vec::new(),
            reply_to: None,
            thread_id: None,
            timestamp: Utc::now(),
            raw: None,
        }
    }
}
