//! Session management tools — list, inspect sessions.

use async_trait::async_trait;
use serde::Deserialize;
use tracing::debug;

use crate::{Tool, ToolContext, ToolOutput};

// --- SessionsListTool ---

pub struct SessionsListTool;

#[async_trait]
impl Tool for SessionsListTool {
    fn name(&self) -> &str {
        "sessions_list"
    }

    fn description(&self) -> &str {
        "List all active sessions with their channel, last activity time, and message count."
    }

    fn parameters_schema(&self) -> serde_json::Value {
        serde_json::json!({
            "type": "object",
            "properties": {}
        })
    }

    async fn execute(
        &self,
        _params: serde_json::Value,
        _context: &ToolContext,
    ) -> anyhow::Result<ToolOutput> {
        debug!("sessions_list");

        let store = rusty_claw_core::session_store::JsonlSessionStore::new(
            rusty_claw_core::session_store::JsonlSessionStore::default_path(),
        );

        use rusty_claw_core::session::SessionStore;
        let sessions = store.list().await?;

        if sessions.is_empty() {
            return Ok(ToolOutput {
                content: "No active sessions.".into(),
                is_error: false,
                media: None,
            });
        }

        let mut output = format!("Active sessions ({}):\n\n", sessions.len());
        for s in &sessions {
            output.push_str(&format!(
                "- **{}** | channel: {} | peer: {} | last: {}\n",
                s.key.hash_key(),
                s.key.channel,
                s.key.peer_id,
                s.last_updated_at.format("%Y-%m-%d %H:%M"),
            ));
        }

        Ok(ToolOutput {
            content: output,
            is_error: false,
            media: None,
        })
    }
}

// --- SessionsSendTool ---

pub struct SessionsSendTool;

#[derive(Deserialize)]
struct SendParams {
    session_hash: String,
    message: String,
}

#[async_trait]
impl Tool for SessionsSendTool {
    fn name(&self) -> &str {
        "sessions_send"
    }

    fn description(&self) -> &str {
        "Send a message to another session (cross-session messaging). The message will appear as a system note in the target session's transcript."
    }

    fn parameters_schema(&self) -> serde_json::Value {
        serde_json::json!({
            "type": "object",
            "properties": {
                "session_hash": {
                    "type": "string",
                    "description": "The hash key of the target session (from sessions_list)"
                },
                "message": {
                    "type": "string",
                    "description": "The message to send to the session"
                }
            },
            "required": ["session_hash", "message"]
        })
    }

    async fn execute(
        &self,
        params: serde_json::Value,
        _context: &ToolContext,
    ) -> anyhow::Result<ToolOutput> {
        let p: SendParams = serde_json::from_value(params)?;
        debug!(session = %p.session_hash, "sessions_send");

        let store = rusty_claw_core::session_store::JsonlSessionStore::new(
            rusty_claw_core::session_store::JsonlSessionStore::default_path(),
        );

        use rusty_claw_core::session::SessionStore;
        let sessions = store.list().await?;

        let session_meta = sessions
            .iter()
            .find(|s| s.key.hash_key() == p.session_hash);

        match session_meta {
            Some(meta) => {
                // Append a system note to the session
                let entry = rusty_claw_core::session::TranscriptEntry::System {
                    event: "cross_session_message".into(),
                    data: serde_json::json!({ "message": p.message }),
                    timestamp: chrono::Utc::now(),
                };
                store.append_entry(&meta.key, &entry).await?;

                Ok(ToolOutput {
                    content: format!("Message sent to session {}", p.session_hash),
                    is_error: false,
                    media: None,
                })
            }
            None => Ok(ToolOutput {
                content: format!("Session '{}' not found", p.session_hash),
                is_error: true,
                media: None,
            }),
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_sessions_list_schema() {
        let tool = SessionsListTool;
        assert_eq!(tool.name(), "sessions_list");
        let schema = tool.parameters_schema();
        assert_eq!(schema["type"], "object");
    }

    #[test]
    fn test_sessions_send_schema() {
        let tool = SessionsSendTool;
        assert_eq!(tool.name(), "sessions_send");
        let schema = tool.parameters_schema();
        assert!(schema["required"].as_array().unwrap().len() == 2);
    }
}
