//! Session model — transcript storage, session keys, and metadata.

use chrono::{DateTime, Utc};
use serde::{Deserialize, Serialize};

use crate::types::{ChatType, ContentBlock, ThinkingLevel};

/// Composite session key encoding the routing context.
#[derive(Debug, Clone, Hash, PartialEq, Eq, Serialize, Deserialize)]
pub struct SessionKey {
    pub channel: String,
    pub account_id: String,
    pub chat_type: ChatType,
    pub peer_id: String,
    pub scope: SessionScope,
}

#[derive(Debug, Clone, Copy, Hash, PartialEq, Eq, Default, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum SessionScope {
    #[default]
    PerSender,
    Global,
    PerPeer,
}

/// Persistent session metadata stored in `sessions.json`.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct SessionMeta {
    pub key: SessionKey,
    pub label: Option<String>,
    pub model: Option<String>,
    #[serde(default)]
    pub thinking_level: ThinkingLevel,
    pub last_channel: Option<String>,
    pub last_updated_at: DateTime<Utc>,
    pub last_reset_at: Option<DateTime<Utc>>,
    pub spawned_by: Option<String>,
    #[serde(default)]
    pub spawn_depth: u32,
}

/// A single entry in the JSONL transcript file.
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(tag = "type")]
pub enum TranscriptEntry {
    #[serde(rename = "user")]
    User {
        content: Vec<ContentBlock>,
        timestamp: DateTime<Utc>,
    },
    #[serde(rename = "assistant")]
    Assistant {
        content: Vec<ContentBlock>,
        #[serde(skip_serializing_if = "Option::is_none")]
        usage: Option<Usage>,
        timestamp: DateTime<Utc>,
    },
    #[serde(rename = "tool_call")]
    ToolCall {
        tool: String,
        params: serde_json::Value,
        timestamp: DateTime<Utc>,
    },
    #[serde(rename = "tool_result")]
    ToolResult {
        tool_use_id: String,
        tool: String,
        content: String,
        is_error: bool,
        timestamp: DateTime<Utc>,
    },
    #[serde(rename = "system")]
    System {
        event: String,
        data: serde_json::Value,
        timestamp: DateTime<Utc>,
    },
}

#[derive(Debug, Clone, Default, Serialize, Deserialize)]
pub struct Usage {
    pub input_tokens: u64,
    pub output_tokens: u64,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub cache_read_tokens: Option<u64>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub cache_write_tokens: Option<u64>,
}

impl SessionKey {
    /// Generate a stable hash string for use as a transcript filename.
    pub fn hash_key(&self) -> String {
        use std::hash::{Hash, Hasher};
        let mut hasher = std::collections::hash_map::DefaultHasher::new();
        self.hash(&mut hasher);
        format!("{:016x}", hasher.finish())
    }
}
