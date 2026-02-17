//! Agent runtime loop — orchestrates LLM streaming + tool calling.

use std::collections::HashMap;
use std::sync::Arc;
use std::time::Instant;

use chrono::Utc;
use serde_json::json;
use tokio::sync::mpsc;
use tokio_stream::StreamExt;
use tracing::{debug, error, info, warn};

use rusty_claw_core::config::Config;
use rusty_claw_core::session::{Session, TranscriptEntry, Usage};
use rusty_claw_core::types::{ContentBlock, ImageSource, InboundMessage, ThinkingLevel};
use rusty_claw_plugins::{HookContext, HookEvent, HookRegistry};
use rusty_claw_providers::{CompletionRequest, Credentials, LlmProvider, ToolDefinition};
use rusty_claw_tools::{ToolContext, ToolRegistry};

use crate::prompt::build_system_prompt_with_persona;
use crate::{AgentEvent, AgentErrorKind, AgentPayload, AgentRunError, AgentRunMeta, AgentRunResult};

/// Build a [`HookContext`] for the current session.
fn hook_ctx(session: &Session) -> HookContext {
    HookContext {
        session_key: session.meta.key.hash_key(),
        timestamp: Utc::now(),
        metadata: HashMap::new(),
    }
}

/// Run the agent loop: stream LLM, execute tools, emit events.
#[allow(clippy::too_many_arguments)]
pub async fn run_agent(
    session: &mut Session,
    message: InboundMessage,
    config: &Arc<Config>,
    tools: &ToolRegistry,
    provider: &dyn LlmProvider,
    credentials: &Credentials,
    event_tx: mpsc::UnboundedSender<AgentEvent>,
    hooks: &Arc<HookRegistry>,
) -> anyhow::Result<AgentRunResult> {
    let start = Instant::now();
    let max_iterations = config.max_tool_iterations();
    let workspace = config.workspace_dir();

    // 1. Build system prompt (no active skills in this code path — gateway can set per-session)
    let active_skills: Vec<&rusty_claw_core::skills::SkillDefinition> = Vec::new();
    let system_prompt = build_system_prompt_with_persona(
        config,
        tools,
        &workspace,
        &active_skills,
        session.meta.custom_system_prompt.as_deref(),
    );

    // 2. Append user message to transcript
    let mut user_content: Vec<ContentBlock> = match &message.text {
        Some(text) => vec![ContentBlock::Text { text: text.clone() }],
        None => vec![ContentBlock::Text {
            text: "(empty message)".into(),
        }],
    };

    // Convert media attachments to Image content blocks
    for media in &message.media {
        if media.mime_type.starts_with("image/") {
            if let Some(ref data) = media.data {
                use base64::Engine;
                let b64 = base64::engine::general_purpose::STANDARD.encode(data);
                user_content.push(ContentBlock::Image {
                    source: ImageSource {
                        source_type: "base64".into(),
                        media_type: media.mime_type.clone(),
                        data: b64,
                    },
                });
            } else if let Some(ref url) = media.url {
                user_content.push(ContentBlock::Image {
                    source: ImageSource {
                        source_type: "url".into(),
                        media_type: media.mime_type.clone(),
                        data: url.clone(),
                    },
                });
            }
        }
    }

    session.append(TranscriptEntry::User {
        content: user_content,
        timestamp: Utc::now(),
    });

    // --- Hook: BeforeAgentStart ---
    let _ = hooks
        .fire(
            HookEvent::BeforeAgentStart,
            hook_ctx(session),
            json!({
                "message": message.text,
                "session_key": session.meta.key.hash_key(),
            }),
        )
        .await;

    let mut total_input_tokens: u64 = 0;
    let mut total_output_tokens: u64 = 0;
    let mut tool_call_count: u32 = 0;
    let mut final_text = String::new();

    // Auto-compact if enabled and transcript exceeds limit
    if config
        .session
        .as_ref()
        .is_some_and(|s| s.auto_compact)
    {
        match crate::compaction::compact_transcript(session, config, provider, credentials, hooks)
            .await
        {
            Ok(true) => {
                info!("Auto-compaction performed before agent run");
            }
            Ok(false) => {}
            Err(e) => {
                warn!(%e, "Auto-compaction failed, continuing anyway");
            }
        }
    }

    // 3. Tool loop
    for iteration in 0..max_iterations {
        debug!(iteration, "Agent loop iteration");

        // Build completion request from transcript
        let messages = provider.format_messages(&session.transcript);
        let tool_defs = if tools.list().is_empty() {
            None
        } else {
            let definitions: Vec<ToolDefinition> = tools
                .tools()
                .iter()
                .map(|t| ToolDefinition {
                    name: t.name().to_string(),
                    description: t.description().to_string(),
                    parameters_schema: t.parameters_schema(),
                })
                .collect();
            Some(provider.format_tools(&definitions))
        };

        // Map thinking level to budget tokens
        let thinking_budget = config
            .agents
            .as_ref()
            .and_then(|a| a.defaults.as_ref())
            .and_then(|d| d.thinking_budget_tokens)
            .or(match session.meta.thinking_level {
                ThinkingLevel::Off => None,
                ThinkingLevel::Minimal => Some(1024),
                ThinkingLevel::Low => Some(2048),
                ThinkingLevel::Medium => Some(4096),
                ThinkingLevel::High => Some(8192),
                ThinkingLevel::XHigh => Some(16384),
            });

        let request = CompletionRequest {
            model: session
                .meta
                .model
                .clone()
                .unwrap_or_else(|| config.default_model()),
            messages,
            max_tokens: config.max_tokens(),
            temperature: config.temperature(),
            tools: tool_defs,
            system: Some(system_prompt.clone()),
            thinking_budget_tokens: thinking_budget,
        };

        // --- Hook: LlmInput ---
        let _ = hooks
            .fire(
                HookEvent::LlmInput,
                hook_ctx(session),
                json!({
                    "model": &request.model,
                    "max_tokens": request.max_tokens,
                }),
            )
            .await;

        // Stream LLM response
        let stream = match provider.stream(&request, credentials).await {
            Ok(s) => s,
            Err(e) => {
                error!(%e, "Provider stream error");
                let _ = event_tx.send(AgentEvent::Error {
                    kind: "provider_error".into(),
                    message: e.to_string(),
                });
                return Ok(AgentRunResult {
                    payloads: vec![AgentPayload {
                        text: Some(format!("Provider error: {e}")),
                        media_urls: vec![],
                        is_error: true,
                    }],
                    meta: AgentRunMeta {
                        duration_ms: start.elapsed().as_millis() as u64,
                        input_tokens: total_input_tokens,
                        output_tokens: total_output_tokens,
                        tool_calls: tool_call_count,
                        aborted: false,
                        stop_reason: None,
                        error: Some(AgentRunError {
                            kind: AgentErrorKind::ProviderError,
                            message: e.to_string(),
                        }),
                    },
                });
            }
        };

        let mut stream = std::pin::pin!(stream);
        let mut response_text = String::new();
        let mut tool_uses: Vec<(String, String, serde_json::Value)> = Vec::new(); // (id, name, input)
        let mut stop_reason = None;

        while let Some(chunk_result) = stream.next().await {
            match chunk_result {
                Ok(chunk) => {
                    // Text delta
                    if let Some(ref delta) = chunk.delta {
                        response_text.push_str(delta);
                        let _ = event_tx.send(AgentEvent::PartialReply {
                            delta: delta.clone(),
                        });
                    }

                    // Thinking delta
                    if let Some(ref thinking) = chunk.thinking {
                        let _ = event_tx.send(AgentEvent::ReasoningStream {
                            text: thinking.clone(),
                        });
                    }

                    // Tool use
                    if let Some(ref tool_use) = chunk.tool_use {
                        let input: serde_json::Value =
                            serde_json::from_str(&tool_use.input_json).unwrap_or(json!({}));
                        tool_uses.push((
                            tool_use.id.clone(),
                            tool_use.name.clone(),
                            input,
                        ));
                    }

                    // Usage
                    if let Some(ref usage) = chunk.usage {
                        if let Some(inp) = usage.input_tokens {
                            total_input_tokens = inp;
                        }
                        if let Some(out) = usage.output_tokens {
                            total_output_tokens = out;
                        }
                    }

                    // Stop reason
                    if let Some(ref reason) = chunk.stop_reason {
                        stop_reason = Some(reason.clone());
                    }
                }
                Err(e) => {
                    error!(%e, "Stream chunk error");
                    let _ = event_tx.send(AgentEvent::Error {
                        kind: "provider_error".into(),
                        message: e.to_string(),
                    });
                    break;
                }
            }
        }

        // Build assistant content blocks
        let mut assistant_content: Vec<ContentBlock> = Vec::new();
        if !response_text.is_empty() {
            assistant_content.push(ContentBlock::Text {
                text: response_text.clone(),
            });
        }
        for (id, name, input) in &tool_uses {
            assistant_content.push(ContentBlock::ToolUse {
                id: id.clone(),
                name: name.clone(),
                input: input.clone(),
            });
        }

        // Append assistant response to transcript
        session.append(TranscriptEntry::Assistant {
            content: assistant_content,
            usage: Some(Usage {
                input_tokens: total_input_tokens,
                output_tokens: total_output_tokens,
                cache_read_tokens: None,
                cache_write_tokens: None,
            }),
            timestamp: Utc::now(),
        });

        // --- Hook: LlmOutput ---
        let _ = hooks
            .fire(
                HookEvent::LlmOutput,
                hook_ctx(session),
                json!({
                    "content": &response_text,
                    "usage": {
                        "input_tokens": total_input_tokens,
                        "output_tokens": total_output_tokens,
                    },
                }),
            )
            .await;

        let _ = event_tx.send(AgentEvent::Usage {
            input_tokens: total_input_tokens,
            output_tokens: total_output_tokens,
        });

        // Check stop reason
        let is_tool_use = stop_reason
            .as_deref()
            .is_some_and(|r| provider.is_tool_use_stop(r));

        if !is_tool_use || tool_uses.is_empty() {
            // No tools to call — we're done
            final_text = response_text;
            let _ = event_tx.send(AgentEvent::BlockReply {
                text: final_text.clone(),
                is_final: true,
            });
            break;
        }

        // Execute tools
        for (id, name, input) in &tool_uses {
            tool_call_count += 1;
            info!(tool = %name, "Executing tool");
            let _ = event_tx.send(AgentEvent::ToolCall {
                tool: name.clone(),
                params: input.clone(),
            });

            // --- Hook: BeforeToolCall (can cancel) ---
            let hook_data = json!({ "tool": name, "params": input });
            let tool_cancelled = match hooks
                .fire_or_cancel(HookEvent::BeforeToolCall, hook_ctx(session), hook_data)
                .await
            {
                Ok(_) => false,
                Err(reason) => {
                    warn!(tool = %name, reason = %reason, "Tool call cancelled by hook");
                    // Record cancellation as tool result
                    session.append(TranscriptEntry::ToolResult {
                        tool_use_id: id.clone(),
                        tool: name.clone(),
                        content: format!("Tool call cancelled: {reason}"),
                        is_error: true,
                        timestamp: Utc::now(),
                    });
                    let _ = event_tx.send(AgentEvent::ToolResult {
                        tool: name.clone(),
                        content: format!("Tool call cancelled: {reason}"),
                        is_error: true,
                    });
                    true
                }
            };

            if tool_cancelled {
                continue;
            }

            // Record tool call in transcript
            session.append(TranscriptEntry::ToolCall {
                tool: name.clone(),
                params: input.clone(),
                timestamp: Utc::now(),
            });

            let sandbox_mode = config
                .agents
                .as_ref()
                .and_then(|a| a.defaults.as_ref())
                .and_then(|d| d.sandbox.as_ref())
                .map(|s| s.mode)
                .unwrap_or_default();

            let restrict_to_workspace = config
                .agents
                .as_ref()
                .and_then(|a| a.defaults.as_ref())
                .and_then(|d| d.sandbox.as_ref())
                .map(|s| s.restrict_to_workspace)
                .unwrap_or(true);

            let tool_context = ToolContext {
                session_key: session.meta.key.hash_key(),
                workspace: workspace.clone(),
                config: config.clone(),
                restrict_to_workspace,
                sandbox_mode,
                browser_pool: None, // Set by gateway when browser is available
            };

            let tool_output = match tools.get(name) {
                Some(tool) => match tool.execute(input.clone(), &tool_context).await {
                    Ok(output) => output,
                    Err(e) => {
                        warn!(%e, tool = %name, "Tool execution error");
                        rusty_claw_tools::ToolOutput {
                            content: format!("Tool error: {e}"),
                            is_error: true,
                            media: None,
                        }
                    }
                },
                None => rusty_claw_tools::ToolOutput {
                    content: format!("Unknown tool: {name}"),
                    is_error: true,
                    media: None,
                },
            };

            // --- Hook: AfterToolCall ---
            let _ = hooks
                .fire(
                    HookEvent::AfterToolCall,
                    hook_ctx(session),
                    json!({
                        "tool": name,
                        "result": &tool_output.content,
                        "is_error": tool_output.is_error,
                    }),
                )
                .await;

            let _ = event_tx.send(AgentEvent::ToolResult {
                tool: name.clone(),
                content: tool_output.content.clone(),
                is_error: tool_output.is_error,
            });

            // Record tool result in transcript
            session.append(TranscriptEntry::ToolResult {
                tool_use_id: id.clone(),
                tool: name.clone(),
                content: tool_output.content,
                is_error: tool_output.is_error,
                timestamp: Utc::now(),
            });
        }

        // Continue the loop — LLM will see the tool results
    }

    // --- Hook: AgentEnd ---
    let _ = hooks
        .fire(
            HookEvent::AgentEnd,
            hook_ctx(session),
            json!({
                "duration_ms": start.elapsed().as_millis() as u64,
                "tool_calls": tool_call_count,
                "input_tokens": total_input_tokens,
                "output_tokens": total_output_tokens,
            }),
        )
        .await;

    Ok(AgentRunResult {
        payloads: vec![AgentPayload {
            text: if final_text.is_empty() {
                None
            } else {
                Some(final_text)
            },
            media_urls: vec![],
            is_error: false,
        }],
        meta: AgentRunMeta {
            duration_ms: start.elapsed().as_millis() as u64,
            input_tokens: total_input_tokens,
            output_tokens: total_output_tokens,
            tool_calls: tool_call_count,
            aborted: false,
            stop_reason: Some("end_turn".into()),
            error: None,
        },
    })
}
