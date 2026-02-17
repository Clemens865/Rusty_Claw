//! Transcript compaction — summarize old entries to stay within context limits.

use std::sync::Arc;

use chrono::Utc;
use serde_json::json;
use tokio_stream::StreamExt;
use tracing::{debug, info, warn};

use rusty_claw_core::config::Config;
use rusty_claw_core::session::{Session, TranscriptEntry};
use rusty_claw_core::types::ContentBlock;
use rusty_claw_plugins::{HookContext, HookEvent, HookRegistry};
use rusty_claw_providers::{CompletionRequest, Credentials, LlmProvider};

use crate::transcript::estimate_transcript_tokens;

/// Compact the transcript if it exceeds the configured token limit.
///
/// Returns `Ok(true)` if compaction was performed, `Ok(false)` if not needed.
pub async fn compact_transcript(
    session: &mut Session,
    config: &Arc<Config>,
    provider: &dyn LlmProvider,
    credentials: &Credentials,
    hooks: &Arc<HookRegistry>,
) -> anyhow::Result<bool> {
    let max_tokens = config.max_context_tokens();
    let keep_recent = config.compact_keep_recent();

    let current_tokens = estimate_transcript_tokens(&session.transcript);
    debug!(current_tokens, max_tokens, "Checking if compaction needed");

    if current_tokens <= max_tokens {
        return Ok(false);
    }

    info!(
        current_tokens,
        max_tokens, "Transcript exceeds limit, compacting"
    );

    // Fire BeforeCompaction hook
    let hook_ctx = HookContext {
        session_key: session.meta.key.hash_key(),
        timestamp: Utc::now(),
        metadata: std::collections::HashMap::new(),
    };
    let _ = hooks
        .fire(
            HookEvent::BeforeCompaction,
            hook_ctx.clone(),
            json!({
                "current_tokens": current_tokens,
                "max_tokens": max_tokens,
            }),
        )
        .await;

    // Split: old entries to summarize vs recent entries to keep
    let total = session.transcript.len();
    let split_at = total.saturating_sub(keep_recent);

    if split_at == 0 {
        debug!("Not enough entries to compact, keeping all");
        return Ok(false);
    }

    let old_entries = &session.transcript[..split_at];
    let recent_entries = session.transcript[split_at..].to_vec();

    // Build summarization prompt
    let summary_text = format_entries_for_summary(old_entries);
    let summarize_prompt = format!(
        "Summarize the following conversation transcript concisely. \
         Preserve key facts, decisions, tool results, and context needed \
         to continue the conversation. Be brief but complete.\n\n{summary_text}"
    );

    // Call the LLM to summarize
    let messages = provider.format_messages(&[TranscriptEntry::User {
        content: vec![ContentBlock::Text {
            text: summarize_prompt,
        }],
        timestamp: Utc::now(),
    }]);

    let request = CompletionRequest {
        model: config.default_model(),
        messages,
        max_tokens: 1024,
        temperature: Some(0.3),
        tools: None,
        system: Some("You are a transcript summarizer. Produce a concise summary.".into()),
        thinking_budget_tokens: None,
    };

    let stream = provider.stream(&request, credentials).await?;
    let mut stream = std::pin::pin!(stream);
    let mut summary = String::new();

    while let Some(chunk_result) = stream.next().await {
        match chunk_result {
            Ok(chunk) => {
                if let Some(ref delta) = chunk.delta {
                    summary.push_str(delta);
                }
            }
            Err(e) => {
                warn!(%e, "Error during compaction summarization");
                break;
            }
        }
    }

    if summary.is_empty() {
        warn!("Compaction produced empty summary, keeping transcript as-is");
        return Ok(false);
    }

    // Replace transcript: compaction system event + recent entries
    let compaction_entry = TranscriptEntry::System {
        event: "compaction".into(),
        data: json!({
            "summary": summary,
            "compacted_entries": split_at,
            "original_tokens": current_tokens,
        }),
        timestamp: Utc::now(),
    };

    session.transcript = Vec::with_capacity(1 + recent_entries.len());
    session.transcript.push(compaction_entry);
    session.transcript.extend(recent_entries);

    let new_tokens = estimate_transcript_tokens(&session.transcript);
    info!(
        old_tokens = current_tokens,
        new_tokens, "Compaction complete"
    );

    // Fire AfterCompaction hook
    let _ = hooks
        .fire(
            HookEvent::AfterCompaction,
            hook_ctx,
            json!({
                "old_tokens": current_tokens,
                "new_tokens": new_tokens,
                "compacted_entries": split_at,
            }),
        )
        .await;

    Ok(true)
}

/// Format transcript entries into readable text for the summarizer.
pub fn format_entries_for_summary(entries: &[TranscriptEntry]) -> String {
    let mut parts = Vec::new();

    for entry in entries {
        match entry {
            TranscriptEntry::User { content, .. } => {
                let text = extract_text(content);
                if !text.is_empty() {
                    parts.push(format!("User: {text}"));
                }
            }
            TranscriptEntry::Assistant { content, .. } => {
                let text = extract_text(content);
                if !text.is_empty() {
                    parts.push(format!("Assistant: {text}"));
                }
            }
            TranscriptEntry::ToolCall { tool, params, .. } => {
                parts.push(format!("Tool call: {tool}({params})"));
            }
            TranscriptEntry::ToolResult {
                tool,
                content,
                is_error,
                ..
            } => {
                let status = if *is_error { "error" } else { "ok" };
                // Truncate long tool results
                let preview = if content.len() > 500 {
                    format!("{}...", &content[..500])
                } else {
                    content.clone()
                };
                parts.push(format!("Tool result ({tool}, {status}): {preview}"));
            }
            TranscriptEntry::System { event, data, .. } => {
                parts.push(format!("System event: {event} — {data}"));
            }
        }
    }

    parts.join("\n")
}

fn extract_text(blocks: &[ContentBlock]) -> String {
    blocks
        .iter()
        .filter_map(|b| match b {
            ContentBlock::Text { text } => Some(text.as_str()),
            _ => None,
        })
        .collect::<Vec<_>>()
        .join(" ")
}

#[cfg(test)]
mod tests {
    use super::*;
    use chrono::Utc;

    #[test]
    fn test_format_entries_for_summary() {
        let entries = vec![
            TranscriptEntry::User {
                content: vec![ContentBlock::Text {
                    text: "What is Rust?".into(),
                }],
                timestamp: Utc::now(),
            },
            TranscriptEntry::Assistant {
                content: vec![ContentBlock::Text {
                    text: "Rust is a systems programming language.".into(),
                }],
                usage: None,
                timestamp: Utc::now(),
            },
        ];
        let summary = format_entries_for_summary(&entries);
        assert!(summary.contains("User: What is Rust?"));
        assert!(summary.contains("Assistant: Rust is a systems programming language."));
    }

    #[test]
    fn test_preserves_recent_entries() {
        // Verify the split logic: if we have 5 entries and keep_recent=3,
        // only the first 2 should be summarized
        let entries: Vec<TranscriptEntry> = (0..5)
            .map(|i| TranscriptEntry::User {
                content: vec![ContentBlock::Text {
                    text: format!("Message {i}"),
                }],
                timestamp: Utc::now(),
            })
            .collect();

        let total = entries.len();
        let keep_recent = 3;
        let split_at = total.saturating_sub(keep_recent);
        assert_eq!(split_at, 2);

        let old = &entries[..split_at];
        let recent = &entries[split_at..];
        assert_eq!(old.len(), 2);
        assert_eq!(recent.len(), 3);

        // Verify old entries contain messages 0 and 1
        let summary = format_entries_for_summary(old);
        assert!(summary.contains("Message 0"));
        assert!(summary.contains("Message 1"));
        assert!(!summary.contains("Message 2"));
    }

    #[test]
    fn test_compaction_not_needed() {
        // Verify the token check: small transcript should not trigger compaction
        let entries = vec![TranscriptEntry::User {
            content: vec![ContentBlock::Text {
                text: "Short message".into(),
            }],
            timestamp: Utc::now(),
        }];
        let tokens = estimate_transcript_tokens(&entries);
        assert!(tokens < 100_000);
    }
}
