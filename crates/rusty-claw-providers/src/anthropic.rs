//! Anthropic Messages API provider.
//!
//! Implements streaming chat completions via the Anthropic Messages API.
//! This is the primary provider for Claude models.

use std::pin::Pin;

use async_trait::async_trait;
use futures::Stream;
use serde::{Deserialize, Serialize};
use tokio_stream::StreamExt;
use tracing::{debug, trace};

use rusty_claw_core::session::TranscriptEntry;
use rusty_claw_core::types::ContentBlock;

use crate::sse::parse_sse_stream;
use crate::{
    ChunkUsage, CompletionChunk, CompletionRequest, Credentials, LlmProvider, ModelApi, ModelInfo,
    ToolDefinition, ToolUseChunk,
};

const DEFAULT_BASE_URL: &str = "https://api.anthropic.com";
const API_VERSION: &str = "2023-06-01";

pub struct AnthropicProvider {
    pub base_url: String,
    client: reqwest::Client,
}

impl AnthropicProvider {
    pub fn new(base_url: Option<&str>) -> Self {
        Self {
            base_url: base_url.unwrap_or(DEFAULT_BASE_URL).trim_end_matches('/').to_string(),
            client: reqwest::Client::new(),
        }
    }
}

// --- Anthropic request/response types ---

#[derive(Debug, Serialize)]
struct AnthropicRequest {
    model: String,
    max_tokens: u32,
    #[serde(skip_serializing_if = "Option::is_none")]
    system: Option<String>,
    messages: Vec<serde_json::Value>,
    stream: bool,
    #[serde(skip_serializing_if = "Option::is_none")]
    temperature: Option<f64>,
    #[serde(skip_serializing_if = "Option::is_none")]
    tools: Option<Vec<serde_json::Value>>,
}

#[derive(Debug, Deserialize)]
struct MessageStart {
    #[serde(default, rename = "id")]
    _id: String,
    #[serde(default)]
    usage: Option<MessageUsage>,
}

#[derive(Debug, Deserialize)]
struct MessageUsage {
    #[serde(default)]
    input_tokens: u64,
    #[serde(default)]
    output_tokens: u64,
}

#[derive(Debug, Deserialize)]
struct ContentBlockStart {
    index: usize,
    content_block: ContentBlockInfo,
}

#[derive(Debug, Deserialize)]
#[serde(tag = "type")]
enum ContentBlockInfo {
    #[serde(rename = "text")]
    Text {
        #[serde(rename = "text")]
        _text: String,
    },
    #[serde(rename = "tool_use")]
    ToolUse { id: String, name: String },
    #[serde(rename = "thinking")]
    Thinking {
        #[serde(rename = "thinking")]
        _thinking: String,
    },
}

#[derive(Debug, Deserialize)]
struct ContentBlockDelta {
    index: usize,
    delta: DeltaInfo,
}

#[derive(Debug, Deserialize)]
#[serde(tag = "type")]
#[allow(clippy::enum_variant_names)] // Names match Anthropic API event types
enum DeltaInfo {
    #[serde(rename = "text_delta")]
    TextDelta { text: String },
    #[serde(rename = "input_json_delta")]
    InputJsonDelta { partial_json: String },
    #[serde(rename = "thinking_delta")]
    ThinkingDelta { thinking: String },
}

#[derive(Debug, Deserialize)]
struct MessageDelta {
    delta: MessageDeltaInner,
    #[serde(default)]
    usage: Option<MessageUsage>,
}

#[derive(Debug, Deserialize)]
struct MessageDeltaInner {
    stop_reason: Option<String>,
}

/// Track the state of each content block being built.
#[derive(Debug, Clone)]
enum BlockState {
    Text,
    ToolUse {
        id: String,
        name: String,
        input_json: String,
    },
    Thinking,
}

#[async_trait]
impl LlmProvider for AnthropicProvider {
    fn id(&self) -> &str {
        "anthropic"
    }

    fn api(&self) -> ModelApi {
        ModelApi::AnthropicMessages
    }

    fn format_tools(&self, tools: &[ToolDefinition]) -> Vec<serde_json::Value> {
        tools
            .iter()
            .map(|t| {
                serde_json::json!({
                    "name": t.name,
                    "description": t.description,
                    "input_schema": t.parameters_schema,
                })
            })
            .collect()
    }

    fn format_messages(&self, transcript: &[TranscriptEntry]) -> Vec<serde_json::Value> {
        let mut messages: Vec<serde_json::Value> = Vec::new();

        for entry in transcript {
            match entry {
                TranscriptEntry::User { content, .. } => {
                    let blocks: Vec<serde_json::Value> =
                        content.iter().map(anthropic_content_block).collect();
                    messages.push(serde_json::json!({
                        "role": "user",
                        "content": blocks,
                    }));
                }
                TranscriptEntry::Assistant { content, .. } => {
                    let blocks: Vec<serde_json::Value> =
                        content.iter().map(anthropic_content_block).collect();
                    if !blocks.is_empty() {
                        messages.push(serde_json::json!({
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
                    messages.push(serde_json::json!({
                        "role": "user",
                        "content": [{
                            "type": "tool_result",
                            "tool_use_id": tool_use_id,
                            "content": content,
                            "is_error": is_error,
                        }],
                    }));
                }
                TranscriptEntry::ToolCall { .. } | TranscriptEntry::System { .. } => {}
            }
        }

        messages
    }

    fn is_tool_use_stop(&self, stop_reason: &str) -> bool {
        stop_reason == "tool_use"
    }

    async fn stream(
        &self,
        request: &CompletionRequest,
        credentials: &Credentials,
    ) -> anyhow::Result<Pin<Box<dyn Stream<Item = anyhow::Result<CompletionChunk>> + Send>>> {
        let api_key = match credentials {
            Credentials::ApiKey { api_key } => api_key.clone(),
            _ => anyhow::bail!("Anthropic requires ApiKey credentials"),
        };

        let body = AnthropicRequest {
            model: request.model.clone(),
            max_tokens: request.max_tokens,
            system: request.system.clone(),
            messages: request.messages.clone(),
            stream: true,
            temperature: request.temperature,
            tools: request.tools.clone(),
        };

        debug!(model = %body.model, "Streaming Anthropic Messages API");

        let response = self
            .client
            .post(format!("{}/v1/messages", self.base_url))
            .header("x-api-key", &api_key)
            .header("anthropic-version", API_VERSION)
            .header("content-type", "application/json")
            .json(&body)
            .send()
            .await?;

        if !response.status().is_success() {
            let status = response.status();
            let body = response.text().await.unwrap_or_default();
            anyhow::bail!("Anthropic API error {status}: {body}");
        }

        let sse_stream = parse_sse_stream(response);

        // Transform SSE events into CompletionChunks
        let chunk_stream = futures::stream::unfold(
            ChunkState {
                sse: Box::pin(sse_stream),
                blocks: Vec::new(),
            },
            |mut state| async move {
                loop {
                    match state.sse.next().await {
                        Some(Ok(sse_event)) => {
                            let event_type = sse_event.event.as_deref().unwrap_or("");

                            match event_type {
                                "message_start" => {
                                    // Parse initial usage
                                    if let Ok(msg) =
                                        serde_json::from_str::<serde_json::Value>(&sse_event.data)
                                    {
                                        if let Some(message) = msg.get("message") {
                                            let _msg_start: MessageStart =
                                                serde_json::from_value(message.clone())
                                                    .unwrap_or(MessageStart {
                                                        _id: String::new(),
                                                        usage: None,
                                                    });
                                            if let Some(usage) = _msg_start.usage {
                                                let chunk = CompletionChunk {
                                                    delta: None,
                                                    thinking: None,
                                                    tool_use: None,
                                                    usage: Some(ChunkUsage {
                                                        input_tokens: Some(usage.input_tokens),
                                                        output_tokens: Some(usage.output_tokens),
                                                    }),
                                                    stop_reason: None,
                                                };
                                                return Some((Ok(chunk), state));
                                            }
                                        }
                                    }
                                    continue;
                                }
                                "content_block_start" => {
                                    if let Ok(cbs) = serde_json::from_str::<ContentBlockStart>(
                                        &sse_event.data,
                                    ) {
                                        let block_state = match &cbs.content_block {
                                            ContentBlockInfo::Text { .. } => BlockState::Text,
                                            ContentBlockInfo::ToolUse { id, name } => {
                                                BlockState::ToolUse {
                                                    id: id.clone(),
                                                    name: name.clone(),
                                                    input_json: String::new(),
                                                }
                                            }
                                            ContentBlockInfo::Thinking { .. } => {
                                                BlockState::Thinking
                                            }
                                        };
                                        // Ensure blocks vec is large enough
                                        while state.blocks.len() <= cbs.index {
                                            state.blocks.push(BlockState::Text);
                                        }
                                        state.blocks[cbs.index] = block_state;
                                    }
                                    continue;
                                }
                                "content_block_delta" => {
                                    if let Ok(cbd) = serde_json::from_str::<ContentBlockDelta>(
                                        &sse_event.data,
                                    ) {
                                        match cbd.delta {
                                            DeltaInfo::TextDelta { text } => {
                                                let chunk = CompletionChunk {
                                                    delta: Some(text),
                                                    thinking: None,
                                                    tool_use: None,
                                                    usage: None,
                                                    stop_reason: None,
                                                };
                                                return Some((Ok(chunk), state));
                                            }
                                            DeltaInfo::InputJsonDelta { partial_json } => {
                                                // Accumulate JSON for tool use
                                                if let Some(BlockState::ToolUse {
                                                    input_json,
                                                    ..
                                                }) =
                                                    state.blocks.get_mut(cbd.index)
                                                {
                                                    input_json.push_str(&partial_json);
                                                }
                                                continue;
                                            }
                                            DeltaInfo::ThinkingDelta { thinking } => {
                                                let chunk = CompletionChunk {
                                                    delta: None,
                                                    thinking: Some(thinking),
                                                    tool_use: None,
                                                    usage: None,
                                                    stop_reason: None,
                                                };
                                                return Some((Ok(chunk), state));
                                            }
                                        }
                                    }
                                    continue;
                                }
                                "content_block_stop" => {
                                    // Emit tool_use chunk if this was a tool block
                                    if let Ok(stop) = serde_json::from_str::<serde_json::Value>(
                                        &sse_event.data,
                                    ) {
                                        if let Some(idx) =
                                            stop.get("index").and_then(|v| v.as_u64())
                                        {
                                            if let Some(BlockState::ToolUse {
                                                id,
                                                name,
                                                input_json,
                                            }) = state.blocks.get(idx as usize)
                                            {
                                                let chunk = CompletionChunk {
                                                    delta: None,
                                                    thinking: None,
                                                    tool_use: Some(ToolUseChunk {
                                                        id: id.clone(),
                                                        name: name.clone(),
                                                        input_json: input_json.clone(),
                                                    }),
                                                    usage: None,
                                                    stop_reason: None,
                                                };
                                                return Some((Ok(chunk), state));
                                            }
                                        }
                                    }
                                    continue;
                                }
                                "message_delta" => {
                                    if let Ok(md) =
                                        serde_json::from_str::<MessageDelta>(&sse_event.data)
                                    {
                                        let chunk = CompletionChunk {
                                            delta: None,
                                            thinking: None,
                                            tool_use: None,
                                            usage: md.usage.map(|u| ChunkUsage {
                                                input_tokens: Some(u.input_tokens),
                                                output_tokens: Some(u.output_tokens),
                                            }),
                                            stop_reason: md.delta.stop_reason,
                                        };
                                        return Some((Ok(chunk), state));
                                    }
                                    continue;
                                }
                                "message_stop" => {
                                    return None;
                                }
                                "ping" => continue,
                                "error" => {
                                    return Some((
                                        Err(anyhow::anyhow!(
                                            "Anthropic SSE error: {}",
                                            sse_event.data
                                        )),
                                        state,
                                    ));
                                }
                                other => {
                                    trace!(event = other, "Unknown SSE event type");
                                    continue;
                                }
                            }
                        }
                        Some(Err(e)) => {
                            return Some((Err(e), state));
                        }
                        None => {
                            return None;
                        }
                    }
                }
            },
        );

        Ok(Box::pin(chunk_stream))
    }

    async fn list_models(&self, credentials: &Credentials) -> anyhow::Result<Vec<ModelInfo>> {
        // Return known Anthropic models (the API doesn't have a list endpoint)
        let _ = credentials;
        Ok(vec![
            ModelInfo {
                id: "claude-opus-4-20250514".into(),
                name: "Claude Opus 4".into(),
                api: ModelApi::AnthropicMessages,
                reasoning: true,
                context_window: 200_000,
                max_tokens: 32_768,
            },
            ModelInfo {
                id: "claude-sonnet-4-20250514".into(),
                name: "Claude Sonnet 4".into(),
                api: ModelApi::AnthropicMessages,
                reasoning: true,
                context_window: 200_000,
                max_tokens: 16_384,
            },
            ModelInfo {
                id: "claude-haiku-3-5-20241022".into(),
                name: "Claude 3.5 Haiku".into(),
                api: ModelApi::AnthropicMessages,
                reasoning: false,
                context_window: 200_000,
                max_tokens: 8_192,
            },
        ])
    }
}

struct ChunkState {
    sse: Pin<Box<dyn Stream<Item = anyhow::Result<crate::sse::SseEvent>> + Send>>,
    blocks: Vec<BlockState>,
}

/// Convert a ContentBlock to Anthropic JSON format.
fn anthropic_content_block(block: &ContentBlock) -> serde_json::Value {
    match block {
        ContentBlock::Text { text } => serde_json::json!({
            "type": "text",
            "text": text,
        }),
        ContentBlock::Image { source } => serde_json::json!({
            "type": "image",
            "source": {
                "type": source.source_type,
                "media_type": source.media_type,
                "data": source.data,
            },
        }),
        ContentBlock::ToolUse { id, name, input } => serde_json::json!({
            "type": "tool_use",
            "id": id,
            "name": name,
            "input": input,
        }),
        ContentBlock::ToolResult {
            tool_use_id,
            content,
            is_error,
        } => serde_json::json!({
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

    #[test]
    fn test_content_block_start_text() {
        let json = r#"{"index":0,"content_block":{"type":"text","text":""}}"#;
        let cbs: ContentBlockStart = serde_json::from_str(json).unwrap();
        assert_eq!(cbs.index, 0);
        assert!(matches!(cbs.content_block, ContentBlockInfo::Text { .. }));
    }

    #[test]
    fn test_content_block_start_tool_use() {
        let json = r#"{"index":1,"content_block":{"type":"tool_use","id":"toolu_123","name":"exec"}}"#;
        let cbs: ContentBlockStart = serde_json::from_str(json).unwrap();
        assert_eq!(cbs.index, 1);
        match cbs.content_block {
            ContentBlockInfo::ToolUse { id, name } => {
                assert_eq!(id, "toolu_123");
                assert_eq!(name, "exec");
            }
            _ => panic!("expected ToolUse"),
        }
    }

    #[test]
    fn test_content_block_delta_text() {
        let json = r#"{"index":0,"delta":{"type":"text_delta","text":"Hello"}}"#;
        let cbd: ContentBlockDelta = serde_json::from_str(json).unwrap();
        assert_eq!(cbd.index, 0);
        match cbd.delta {
            DeltaInfo::TextDelta { text } => assert_eq!(text, "Hello"),
            _ => panic!("expected TextDelta"),
        }
    }

    #[test]
    fn test_content_block_delta_json() {
        let json = r#"{"index":1,"delta":{"type":"input_json_delta","partial_json":"{\"command\""}}"#;
        let cbd: ContentBlockDelta = serde_json::from_str(json).unwrap();
        match cbd.delta {
            DeltaInfo::InputJsonDelta { partial_json } => {
                assert_eq!(partial_json, r#"{"command""#);
            }
            _ => panic!("expected InputJsonDelta"),
        }
    }

    #[test]
    fn test_message_delta() {
        let json =
            r#"{"delta":{"stop_reason":"end_turn"},"usage":{"input_tokens":100,"output_tokens":50}}"#;
        let md: MessageDelta = serde_json::from_str(json).unwrap();
        assert_eq!(md.delta.stop_reason.as_deref(), Some("end_turn"));
        assert_eq!(md.usage.unwrap().output_tokens, 50);
    }

    #[test]
    fn test_anthropic_provider_creation() {
        let provider = AnthropicProvider::new(None);
        assert_eq!(provider.id(), "anthropic");
        assert_eq!(provider.api(), ModelApi::AnthropicMessages);
        assert_eq!(provider.base_url, DEFAULT_BASE_URL);
    }

    #[test]
    fn test_anthropic_provider_custom_url() {
        let provider = AnthropicProvider::new(Some("https://custom.api.com/"));
        assert_eq!(provider.base_url, "https://custom.api.com");
    }

    #[test]
    fn test_format_tools_uses_input_schema() {
        let provider = AnthropicProvider::new(None);
        let tools = vec![ToolDefinition {
            name: "exec".into(),
            description: "Run a shell command".into(),
            parameters_schema: serde_json::json!({
                "type": "object",
                "properties": { "command": { "type": "string" } },
                "required": ["command"],
            }),
        }];
        let formatted = provider.format_tools(&tools);
        assert_eq!(formatted.len(), 1);
        assert_eq!(formatted[0]["name"], "exec");
        assert!(formatted[0].get("input_schema").is_some());
        assert!(formatted[0].get("parameters").is_none());
    }

    #[test]
    fn test_is_tool_use_stop() {
        let provider = AnthropicProvider::new(None);
        assert!(provider.is_tool_use_stop("tool_use"));
        assert!(!provider.is_tool_use_stop("end_turn"));
        assert!(!provider.is_tool_use_stop("stop"));
        assert!(!provider.is_tool_use_stop("tool_calls"));
    }

    #[test]
    fn test_format_messages_basic() {
        use chrono::Utc;
        let provider = AnthropicProvider::new(None);
        let transcript = vec![
            TranscriptEntry::User {
                content: vec![ContentBlock::Text {
                    text: "Hello".into(),
                }],
                timestamp: Utc::now(),
            },
            TranscriptEntry::Assistant {
                content: vec![ContentBlock::Text {
                    text: "Hi there".into(),
                }],
                usage: None,
                timestamp: Utc::now(),
            },
        ];
        let messages = provider.format_messages(&transcript);
        assert_eq!(messages.len(), 2);
        assert_eq!(messages[0]["role"], "user");
        assert_eq!(messages[1]["role"], "assistant");
        assert_eq!(messages[1]["content"][0]["text"], "Hi there");
    }
}
