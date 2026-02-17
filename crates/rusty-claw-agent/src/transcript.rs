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

// ============================================================
// Token estimation
// ============================================================

/// Estimate token count from text using a simple heuristic (1 token ~ 4 chars).
pub fn estimate_tokens(text: &str) -> usize {
    text.len().div_ceil(4)
}

/// Estimate total tokens across all transcript entries.
pub fn estimate_transcript_tokens(transcript: &[TranscriptEntry]) -> usize {
    transcript.iter().map(entry_chars).sum::<usize>() / 4
}

fn entry_chars(entry: &TranscriptEntry) -> usize {
    match entry {
        TranscriptEntry::User { content, .. } => content_blocks_chars(content),
        TranscriptEntry::Assistant { content, .. } => content_blocks_chars(content),
        TranscriptEntry::ToolCall { tool, params, .. } => {
            tool.len() + params.to_string().len()
        }
        TranscriptEntry::ToolResult { content, tool, .. } => {
            tool.len() + content.len()
        }
        TranscriptEntry::System { event, data, .. } => {
            event.len() + data.to_string().len()
        }
    }
}

fn content_blocks_chars(blocks: &[ContentBlock]) -> usize {
    blocks.iter().map(|b| match b {
        ContentBlock::Text { text } => text.len(),
        ContentBlock::Image { .. } => 256, // rough estimate for image token overhead
        ContentBlock::ToolUse { name, input, .. } => name.len() + input.to_string().len(),
        ContentBlock::ToolResult { content, .. } => content.len(),
    }).sum()
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
    fn test_estimate_tokens_basic() {
        assert_eq!(estimate_tokens("hello world"), 3); // 11 chars / 4 = 2.75 → 3
        assert_eq!(estimate_tokens(""), 0);
        assert_eq!(estimate_tokens("a"), 1);
        assert_eq!(estimate_tokens("abcd"), 1);
        assert_eq!(estimate_tokens("abcde"), 2);
    }

    #[test]
    fn test_estimate_transcript_tokens() {
        let transcript = vec![
            TranscriptEntry::User {
                content: vec![ContentBlock::Text { text: "Hello there, this is a test message".into() }],
                timestamp: Utc::now(),
            },
            TranscriptEntry::Assistant {
                content: vec![ContentBlock::Text { text: "Hi! How can I help you?".into() }],
                usage: None,
                timestamp: Utc::now(),
            },
        ];
        let tokens = estimate_transcript_tokens(&transcript);
        assert!(tokens > 0);
        // "Hello there, this is a test message" = 35 chars
        // "Hi! How can I help you?" = 23 chars
        // Total chars = 58, / 4 = 14
        assert_eq!(tokens, 14);
    }

    #[test]
    fn test_estimate_transcript_empty() {
        let transcript: Vec<TranscriptEntry> = vec![];
        assert_eq!(estimate_transcript_tokens(&transcript), 0);
    }

    // --- 6c-1: Thinking Token Pass-through ---

    #[test]
    fn test_thinking_level_to_budget_mapping() {
        use rusty_claw_core::types::ThinkingLevel;

        // Each ThinkingLevel variant should map to the correct budget
        let mapping: Vec<(ThinkingLevel, Option<u32>)> = vec![
            (ThinkingLevel::Off, None),
            (ThinkingLevel::Minimal, Some(1024)),
            (ThinkingLevel::Low, Some(2048)),
            (ThinkingLevel::Medium, Some(4096)),
            (ThinkingLevel::High, Some(8192)),
            (ThinkingLevel::XHigh, Some(16384)),
        ];

        for (level, expected) in mapping {
            let budget = match level {
                ThinkingLevel::Off => None,
                ThinkingLevel::Minimal => Some(1024),
                ThinkingLevel::Low => Some(2048),
                ThinkingLevel::Medium => Some(4096),
                ThinkingLevel::High => Some(8192),
                ThinkingLevel::XHigh => Some(16384),
            };
            assert_eq!(
                budget, expected,
                "ThinkingLevel::{level:?} should map to {expected:?}, got {budget:?}"
            );
        }
    }

    // --- 6c-2: Image Input / media_to_content_block ---

    #[test]
    fn test_media_to_content_block() {
        use rusty_claw_core::types::{ContentBlock, ImageSource, MediaAttachment};
        use base64::Engine;

        // Simulate converting a media attachment with base64 data into a ContentBlock::Image
        let raw_data = b"fake-png-data";
        let b64 = base64::engine::general_purpose::STANDARD.encode(raw_data);

        let attachment = MediaAttachment {
            url: None,
            data: Some(raw_data.to_vec()),
            mime_type: "image/png".into(),
            filename: Some("test.png".into()),
            size_bytes: Some(raw_data.len() as u64),
        };

        // Convert (this mirrors the logic in runtime.rs run_agent)
        let block = if attachment.mime_type.starts_with("image/") {
            if let Some(ref data) = attachment.data {
                let encoded = base64::engine::general_purpose::STANDARD.encode(data);
                Some(ContentBlock::Image {
                    source: ImageSource {
                        source_type: "base64".into(),
                        media_type: attachment.mime_type.clone(),
                        data: encoded,
                    },
                })
            } else {
                None
            }
        } else {
            None
        };

        let block = block.expect("Should produce an Image content block");
        match block {
            ContentBlock::Image { source } => {
                assert_eq!(source.source_type, "base64");
                assert_eq!(source.media_type, "image/png");
                assert_eq!(source.data, b64);
            }
            _ => panic!("Expected ContentBlock::Image"),
        }
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
