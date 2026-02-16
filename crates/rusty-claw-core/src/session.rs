//! Session model — transcript storage, session keys, and metadata.

use async_trait::async_trait;
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
    /// Currently active skill (if any).
    #[serde(skip_serializing_if = "Option::is_none")]
    pub active_skill: Option<String>,
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

/// Runtime session: metadata + loaded transcript.
#[derive(Debug, Clone)]
pub struct Session {
    pub meta: SessionMeta,
    pub transcript: Vec<TranscriptEntry>,
}

impl Session {
    /// Create a new empty session with the given key.
    pub fn new(key: SessionKey) -> Self {
        let meta = SessionMeta {
            key,
            label: None,
            model: None,
            thinking_level: ThinkingLevel::default(),
            last_channel: None,
            last_updated_at: Utc::now(),
            last_reset_at: None,
            spawned_by: None,
            spawn_depth: 0,
            active_skill: None,
        };
        Self {
            meta,
            transcript: Vec::new(),
        }
    }

    /// Append an entry to this session's transcript.
    pub fn append(&mut self, entry: TranscriptEntry) {
        self.meta.last_updated_at = Utc::now();
        self.transcript.push(entry);
    }
}

/// Async session persistence trait.
#[async_trait]
pub trait SessionStore: Send + Sync {
    /// Load a session by key. Returns None if it doesn't exist.
    async fn load(&self, key: &SessionKey) -> crate::error::Result<Option<Session>>;

    /// Save a full session (metadata + transcript).
    async fn save(&self, session: &Session) -> crate::error::Result<()>;

    /// Append a single transcript entry (append-only for durability).
    async fn append_entry(
        &self,
        key: &SessionKey,
        entry: &TranscriptEntry,
    ) -> crate::error::Result<()>;

    /// List all session metadata.
    async fn list(&self) -> crate::error::Result<Vec<SessionMeta>>;

    /// Delete a session entirely (metadata + transcript).
    async fn delete(&self, key: &SessionKey) -> crate::error::Result<()>;

    /// Reset a session's transcript (keeps metadata, clears transcript).
    async fn reset(&self, key: &SessionKey) -> crate::error::Result<()>;
}
