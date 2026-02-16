//! Convert session transcript to Anthropic message format.

use rusty_claw_core::session::TranscriptEntry;
use rusty_claw_core::types::ContentBlock;
use serde_json::json;

/// Convert a transcript to the Anthropic Messages API format.
pub fn transcript_to_messages(transcript: &[TranscriptEntry]) -> Vec<serde_json::Value> {
    let mut messages: Vec<serde_json::Value> = Vec::new();

    for entry in transcript {
        match entry {
            TranscriptEntry::User { content, .. } => {
                let blocks: Vec<serde_json::Value> = content
                    .iter()
                    .map(content_block_to_json)
                    .collect();
                messages.push(json!({
                    "role": "user",
                    "content": blocks,
                }));
            }
            TranscriptEntry::Assistant { content, .. } => {
                let blocks: Vec<serde_json::Value> = content
                    .iter()
                    .map(content_block_to_json)
                    .collect();
                if !blocks.is_empty() {
                    messages.push(json!({
                        "role": "assistant",
                        "content": blocks,
                    }));
                }
            }
            TranscriptEntry::ToolResult {
                tool_use_id,
                content,
                is_error,
                ..
            } => {
                messages.push(json!({
                    "role": "user",
                    "content": [{
                        "type": "tool_result",
                        "tool_use_id": tool_use_id,
                        "content": content,
                        "is_error": is_error,
                    }],
                }));
            }
            TranscriptEntry::ToolCall { .. } | TranscriptEntry::System { .. } => {
                // ToolCall entries are already embedded in Assistant content blocks.
                // System events don't go to the LLM.
            }
        }
    }

    messages
}

fn content_block_to_json(block: &ContentBlock) -> serde_json::Value {
    match block {
        ContentBlock::Text { text } => json!({
            "type": "text",
            "text": text,
        }),
        ContentBlock::Image { source } => json!({
            "type": "image",
            "source": {
                "type": source.source_type,
                "media_type": source.media_type,
                "data": source.data,
            },
        }),
        ContentBlock::ToolUse { id, name, input } => json!({
            "type": "tool_use",
            "id": id,
            "name": name,
            "input": input,
        }),
        ContentBlock::ToolResult {
            tool_use_id,
            content,
            is_error,
        } => json!({
            "type": "tool_result",
            "tool_use_id": tool_use_id,
            "content": content,
            "is_error": is_error,
        }),
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use chrono::Utc;

    #[test]
    fn test_user_message_conversion() {
        let transcript = vec![TranscriptEntry::User {
            content: vec![ContentBlock::Text {
                text: "Hello".into(),
            }],
            timestamp: Utc::now(),
        }];

        let messages = transcript_to_messages(&transcript);
        assert_eq!(messages.len(), 1);
        assert_eq!(messages[0]["role"], "user");
        assert_eq!(messages[0]["content"][0]["type"], "text");
        assert_eq!(messages[0]["content"][0]["text"], "Hello");
    }

    #[test]
    fn test_tool_use_roundtrip() {
        let transcript = vec![
            TranscriptEntry::User {
                content: vec![ContentBlock::Text {
                    text: "Run ls".into(),
                }],
                timestamp: Utc::now(),
            },
            TranscriptEntry::Assistant {
                content: vec![ContentBlock::ToolUse {
                    id: "toolu_1".into(),
                    name: "exec".into(),
                    input: json!({"command": "ls"}),
                }],
                usage: None,
                timestamp: Utc::now(),
            },
            TranscriptEntry::ToolCall {
                tool: "exec".into(),
                params: json!({"command": "ls"}),
                timestamp: Utc::now(),
            },
            TranscriptEntry::ToolResult {
                tool_use_id: "toolu_1".into(),
                tool: "exec".into(),
                content: "file1.txt\nfile2.txt".into(),
                is_error: false,
                timestamp: Utc::now(),
            },
        ];

        let messages = transcript_to_messages(&transcript);
        // User, Assistant (with tool_use), Tool result
        assert_eq!(messages.len(), 3);
        assert_eq!(messages[0]["role"], "user");
        assert_eq!(messages[1]["role"], "assistant");
        assert_eq!(messages[1]["content"][0]["type"], "tool_use");
        assert_eq!(messages[2]["role"], "user");
        assert_eq!(messages[2]["content"][0]["type"], "tool_result");
    }
}
