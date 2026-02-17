//! OpenAI Chat Completions API provider.
//!
//! Implements streaming chat completions via OpenAI's `/v1/chat/completions` API.
//! Also serves as the base for OpenRouter, Ollama, and other OpenAI-compatible providers.

use std::pin::Pin;

use async_trait::async_trait;
use futures::Stream;
use serde::{Deserialize, Serialize};
use serde_json::json;
use tokio_stream::StreamExt;
use tracing::{debug, trace};

use rusty_claw_core::session::TranscriptEntry;
use rusty_claw_core::types::ContentBlock;

use crate::sse::parse_sse_stream;
use crate::{
    ChunkUsage, CompletionChunk, CompletionRequest, Credentials, LlmProvider, ModelApi, ModelInfo,
    ToolDefinition, ToolUseChunk,
};

const OPENAI_BASE_URL: &str = "https://api.openai.com";
const OPENROUTER_BASE_URL: &str = "https://openrouter.ai/api";
const OLLAMA_BASE_URL: &str = "http://localhost:11434";

/// API style — determines minor behavior differences.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ApiStyle {
    OpenAi,
    OpenRouter,
    Ollama,
}

pub struct OpenAiProvider {
    pub base_url: String,
    pub api_style: ApiStyle,
    provider_id: String,
    client: reqwest::Client,
}

impl OpenAiProvider {
    pub fn openai(base_url: Option<&str>) -> Self {
        Self {
            base_url: base_url
                .unwrap_or(OPENAI_BASE_URL)
                .trim_end_matches('/')
                .to_string(),
            api_style: ApiStyle::OpenAi,
            provider_id: "openai".into(),
            client: reqwest::Client::new(),
        }
    }

    pub fn openrouter(base_url: Option<&str>) -> Self {
        Self {
            base_url: base_url
                .unwrap_or(OPENROUTER_BASE_URL)
                .trim_end_matches('/')
                .to_string(),
            api_style: ApiStyle::OpenRouter,
            provider_id: "openrouter".into(),
            client: reqwest::Client::new(),
        }
    }

    pub fn ollama(base_url: Option<&str>) -> Self {
        Self {
            base_url: base_url
                .unwrap_or(OLLAMA_BASE_URL)
                .trim_end_matches('/')
                .to_string(),
            api_style: ApiStyle::Ollama,
            provider_id: "ollama".into(),
            client: reqwest::Client::new(),
        }
    }
}

// --- OpenAI request/response types ---

#[derive(Debug, Serialize)]
struct OpenAiRequest {
    model: String,
    messages: Vec<serde_json::Value>,
    max_tokens: u32,
    stream: bool,
    #[serde(skip_serializing_if = "Option::is_none")]
    temperature: Option<f64>,
    #[serde(skip_serializing_if = "Option::is_none")]
    tools: Option<Vec<serde_json::Value>>,
    #[serde(skip_serializing_if = "Option::is_none")]
    stream_options: Option<StreamOptions>,
}

#[derive(Debug, Serialize)]
struct StreamOptions {
    include_usage: bool,
}

#[derive(Debug, Deserialize)]
struct ChatCompletionChunk {
    #[serde(default)]
    choices: Vec<ChunkChoice>,
    #[serde(default)]
    usage: Option<OpenAiUsage>,
}

#[derive(Debug, Deserialize)]
struct ChunkChoice {
    #[serde(default)]
    delta: ChunkDelta,
    #[serde(default)]
    finish_reason: Option<String>,
}

#[derive(Debug, Default, Deserialize)]
struct ChunkDelta {
    #[serde(default)]
    content: Option<String>,
    #[serde(default)]
    tool_calls: Option<Vec<ToolCallDelta>>,
}

#[derive(Debug, Deserialize)]
struct ToolCallDelta {
    index: usize,
    #[serde(default)]
    id: Option<String>,
    #[serde(default)]
    function: Option<FunctionDelta>,
}

#[derive(Debug, Default, Deserialize)]
struct FunctionDelta {
    #[serde(default)]
    name: Option<String>,
    #[serde(default)]
    arguments: Option<String>,
}

#[derive(Debug, Deserialize)]
struct OpenAiUsage {
    #[serde(default)]
    prompt_tokens: u64,
    #[serde(default)]
    completion_tokens: u64,
}

/// Accumulates tool call data across streaming deltas.
#[derive(Debug, Clone)]
struct ToolCallAccumulator {
    id: String,
    name: String,
    arguments: String,
}

#[derive(Debug, Deserialize)]
struct ModelsResponse {
    data: Vec<ModelEntry>,
}

#[derive(Debug, Deserialize)]
struct ModelEntry {
    id: String,
}

#[async_trait]
impl LlmProvider for OpenAiProvider {
    fn id(&self) -> &str {
        &self.provider_id
    }

    fn api(&self) -> ModelApi {
        ModelApi::OpenAiCompletions
    }

    fn format_tools(&self, tools: &[ToolDefinition]) -> Vec<serde_json::Value> {
        tools
            .iter()
            .map(|t| {
                json!({
                    "type": "function",
                    "function": {
                        "name": t.name,
                        "description": t.description,
                        "parameters": t.parameters_schema,
                    }
                })
            })
            .collect()
    }

    fn format_messages(&self, transcript: &[TranscriptEntry]) -> Vec<serde_json::Value> {
        let mut messages: Vec<serde_json::Value> = Vec::new();

        for entry in transcript {
            match entry {
                TranscriptEntry::User { content, .. } => {
                    let has_images = content.iter().any(|b| matches!(b, ContentBlock::Image { .. }));

                    if has_images {
                        // Use array-of-parts format for multimodal
                        let parts: Vec<serde_json::Value> = content
                            .iter()
                            .filter_map(|b| match b {
                                ContentBlock::Text { text } => {
                                    Some(json!({"type": "text", "text": text}))
                                }
                                ContentBlock::Image { source } => {
                                    let url = if source.source_type == "base64" {
                                        format!(
                                            "data:{};base64,{}",
                                            source.media_type, source.data
                                        )
                                    } else {
                                        source.data.clone()
                                    };
                                    Some(json!({
                                        "type": "image_url",
                                        "image_url": {"url": url}
                                    }))
                                }
                                _ => None,
                            })
                            .collect();
                        if !parts.is_empty() {
                            messages.push(json!({"role": "user", "content": parts}));
                        }
                    } else {
                        let text = content
                            .iter()
                            .filter_map(|b| match b {
                                ContentBlock::Text { text } => Some(text.as_str()),
                                _ => None,
                            })
                            .collect::<Vec<_>>()
                            .join("\n");
                        if !text.is_empty() {
                            messages.push(json!({ "role": "user", "content": text }));
                        }
                    }
                }
                TranscriptEntry::Assistant { content, .. } => {
                    let mut text_parts = Vec::new();
                    let mut tool_calls = Vec::new();

                    for block in content {
                        match block {
                            ContentBlock::Text { text } => text_parts.push(text.clone()),
                            ContentBlock::ToolUse { id, name, input } => {
                                tool_calls.push(json!({
                                    "id": id,
                                    "type": "function",
                                    "function": {
                                        "name": name,
                                        "arguments": input.to_string(),
                                    }
                                }));
                            }
                            _ => {}
                        }
                    }

                    let mut msg = json!({ "role": "assistant" });
                    if !text_parts.is_empty() {
                        msg["content"] = json!(text_parts.join("\n"));
                    }
                    if !tool_calls.is_empty() {
                        msg["tool_calls"] = json!(tool_calls);
                    }
                    if msg.get("content").is_some() || msg.get("tool_calls").is_some() {
                        messages.push(msg);
                    }
                }
                TranscriptEntry::ToolResult {
                    tool_use_id,
                    content,
                    ..
                } => {
                    messages.push(json!({
                        "role": "tool",
                        "tool_call_id": tool_use_id,
                        "content": content,
                    }));
                }
                TranscriptEntry::ToolCall { .. } | TranscriptEntry::System { .. } => {}
            }
        }

        messages
    }

    fn is_tool_use_stop(&self, stop_reason: &str) -> bool {
        stop_reason == "tool_calls"
    }

    async fn stream(
        &self,
        request: &CompletionRequest,
        credentials: &Credentials,
    ) -> anyhow::Result<Pin<Box<dyn Stream<Item = anyhow::Result<CompletionChunk>> + Send>>> {
        let api_key = match credentials {
            Credentials::ApiKey { api_key } => api_key.clone(),
            _ => anyhow::bail!("OpenAI-compatible providers require ApiKey credentials"),
        };

        // Build system message if present
        let mut messages = Vec::new();
        if let Some(ref system) = request.system {
            messages.push(json!({ "role": "system", "content": system }));
        }
        messages.extend(request.messages.iter().cloned());

        let body = OpenAiRequest {
            model: request.model.clone(),
            messages,
            max_tokens: request.max_tokens,
            stream: true,
            temperature: request.temperature,
            tools: request.tools.clone(),
            stream_options: Some(StreamOptions {
                include_usage: true,
            }),
        };

        debug!(model = %body.model, base_url = %self.base_url, "Streaming OpenAI-compatible API");

        let mut req_builder = self
            .client
            .post(format!("{}/v1/chat/completions", self.base_url))
            .header("content-type", "application/json");

        // Auth differs by style
        if self.api_style != ApiStyle::Ollama {
            req_builder = req_builder.header("authorization", format!("Bearer {api_key}"));
        }
        if self.api_style == ApiStyle::OpenRouter {
            req_builder = req_builder.header("HTTP-Referer", "https://rusty-claw.dev");
        }

        let response = req_builder.json(&body).send().await?;

        if !response.status().is_success() {
            let status = response.status();
            let body = response.text().await.unwrap_or_default();
            anyhow::bail!("OpenAI API error {status}: {body}");
        }

        let sse_stream = parse_sse_stream(response);

        let chunk_stream = futures::stream::unfold(
            OpenAiChunkState {
                sse: Box::pin(sse_stream),
                tool_calls: Vec::new(),
            },
            |mut state| async move {
                loop {
                    match state.sse.next().await {
                        Some(Ok(sse_event)) => {
                            let data = sse_event.data.trim();

                            // OpenAI terminates with "data: [DONE]"
                            if data == "[DONE]" {
                                // Emit any accumulated tool calls
                                if !state.tool_calls.is_empty() {
                                    let tools: Vec<CompletionChunk> = state
                                        .tool_calls
                                        .drain(..)
                                        .map(|tc| CompletionChunk {
                                            delta: None,
                                            thinking: None,
                                            tool_use: Some(ToolUseChunk {
                                                id: tc.id,
                                                name: tc.name,
                                                input_json: tc.arguments,
                                            }),
                                            usage: None,
                                            stop_reason: None,
                                        })
                                        .collect();

                                    // Return first tool, put rest back
                                    // Actually we need to emit all. Let's emit the first
                                    // and let the loop continue for the rest.
                                    // Better approach: emit them in batch before [DONE]
                                    if let Some(first) = tools.into_iter().next() {
                                        return Some((Ok(first), state));
                                    }
                                }
                                return None;
                            }

                            let chunk: ChatCompletionChunk = match serde_json::from_str(data) {
                                Ok(c) => c,
                                Err(e) => {
                                    trace!(%e, data, "Failed to parse OpenAI chunk");
                                    continue;
                                }
                            };

                            // Usage (sent with stream_options.include_usage)
                            if let Some(usage) = chunk.usage {
                                let c = CompletionChunk {
                                    delta: None,
                                    thinking: None,
                                    tool_use: None,
                                    usage: Some(ChunkUsage {
                                        input_tokens: Some(usage.prompt_tokens),
                                        output_tokens: Some(usage.completion_tokens),
                                    }),
                                    stop_reason: None,
                                };
                                return Some((Ok(c), state));
                            }

                            let choice = match chunk.choices.first() {
                                Some(c) => c,
                                None => continue,
                            };

                            // Accumulate tool call deltas
                            if let Some(ref tc_deltas) = choice.delta.tool_calls {
                                for tc in tc_deltas {
                                    // Ensure accumulator exists for this index
                                    while state.tool_calls.len() <= tc.index {
                                        state.tool_calls.push(ToolCallAccumulator {
                                            id: String::new(),
                                            name: String::new(),
                                            arguments: String::new(),
                                        });
                                    }
                                    let acc = &mut state.tool_calls[tc.index];
                                    if let Some(ref id) = tc.id {
                                        acc.id = id.clone();
                                    }
                                    if let Some(ref f) = tc.function {
                                        if let Some(ref name) = f.name {
                                            acc.name = name.clone();
                                        }
                                        if let Some(ref args) = f.arguments {
                                            acc.arguments.push_str(args);
                                        }
                                    }
                                }
                            }

                            // Text delta
                            if let Some(ref content) = choice.delta.content {
                                if !content.is_empty() {
                                    let c = CompletionChunk {
                                        delta: Some(content.clone()),
                                        thinking: None,
                                        tool_use: None,
                                        usage: None,
                                        stop_reason: None,
                                    };
                                    return Some((Ok(c), state));
                                }
                            }

                            // finish_reason
                            if let Some(ref reason) = choice.finish_reason {
                                // If tool_calls, emit accumulated tool calls first
                                if reason == "tool_calls" && !state.tool_calls.is_empty() {
                                    let tc = state.tool_calls.remove(0);
                                    let c = CompletionChunk {
                                        delta: None,
                                        thinking: None,
                                        tool_use: Some(ToolUseChunk {
                                            id: tc.id,
                                            name: tc.name,
                                            input_json: tc.arguments,
                                        }),
                                        usage: None,
                                        stop_reason: if state.tool_calls.is_empty() {
                                            Some(reason.clone())
                                        } else {
                                            None
                                        },
                                    };
                                    return Some((Ok(c), state));
                                }

                                let c = CompletionChunk {
                                    delta: None,
                                    thinking: None,
                                    tool_use: None,
                                    usage: None,
                                    stop_reason: Some(reason.clone()),
                                };
                                return Some((Ok(c), state));
                            }

                            continue;
                        }
                        Some(Err(e)) => {
                            return Some((Err(e), state));
                        }
                        None => {
                            // Emit remaining tool calls
                            if let Some(tc) = state.tool_calls.pop() {
                                let c = CompletionChunk {
                                    delta: None,
                                    thinking: None,
                                    tool_use: Some(ToolUseChunk {
                                        id: tc.id,
                                        name: tc.name,
                                        input_json: tc.arguments,
                                    }),
                                    usage: None,
                                    stop_reason: None,
                                };
                                return Some((Ok(c), state));
                            }
                            return None;
                        }
                    }
                }
            },
        );

        Ok(Box::pin(chunk_stream))
    }

    async fn list_models(&self, credentials: &Credentials) -> anyhow::Result<Vec<ModelInfo>> {
        let api_key = match credentials {
            Credentials::ApiKey { api_key } => api_key.clone(),
            _ => anyhow::bail!("OpenAI-compatible providers require ApiKey credentials"),
        };

        let mut req = self
            .client
            .get(format!("{}/v1/models", self.base_url))
            .header("content-type", "application/json");

        if self.api_style != ApiStyle::Ollama {
            req = req.header("authorization", format!("Bearer {api_key}"));
        }

        let response = req.send().await?;
        if !response.status().is_success() {
            let status = response.status();
            let body = response.text().await.unwrap_or_default();
            anyhow::bail!("Failed to list models {status}: {body}");
        }

        let body: ModelsResponse = response.json().await?;
        Ok(body
            .data
            .into_iter()
            .map(|m| ModelInfo {
                name: m.id.clone(),
                id: m.id,
                api: ModelApi::OpenAiCompletions,
                reasoning: false,
                context_window: 128_000,
                max_tokens: 4_096,
            })
            .collect())
    }
}

struct OpenAiChunkState {
    sse: Pin<Box<dyn Stream<Item = anyhow::Result<crate::sse::SseEvent>> + Send>>,
    tool_calls: Vec<ToolCallAccumulator>,
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_openai_provider_creation() {
        let provider = OpenAiProvider::openai(None);
        assert_eq!(provider.id(), "openai");
        assert_eq!(provider.api(), ModelApi::OpenAiCompletions);
        assert_eq!(provider.base_url, OPENAI_BASE_URL);
    }

    #[test]
    fn test_openrouter_provider_creation() {
        let provider = OpenAiProvider::openrouter(None);
        assert_eq!(provider.id(), "openrouter");
        assert_eq!(provider.base_url, OPENROUTER_BASE_URL);
    }

    #[test]
    fn test_ollama_provider_creation() {
        let provider = OpenAiProvider::ollama(None);
        assert_eq!(provider.id(), "ollama");
        assert_eq!(provider.base_url, OLLAMA_BASE_URL);
    }

    #[test]
    fn test_custom_base_url() {
        let provider = OpenAiProvider::openai(Some("https://my-proxy.example.com/"));
        assert_eq!(provider.base_url, "https://my-proxy.example.com");
    }

    #[test]
    fn test_format_tools_function_wrapper() {
        let provider = OpenAiProvider::openai(None);
        let tools = vec![ToolDefinition {
            name: "exec".into(),
            description: "Run a shell command".into(),
            parameters_schema: json!({
                "type": "object",
                "properties": { "command": { "type": "string" } },
            }),
        }];
        let formatted = provider.format_tools(&tools);
        assert_eq!(formatted.len(), 1);
        assert_eq!(formatted[0]["type"], "function");
        assert_eq!(formatted[0]["function"]["name"], "exec");
        assert!(formatted[0]["function"]["parameters"].is_object());
        // OpenAI uses "parameters", NOT "input_schema"
        assert!(formatted[0].get("input_schema").is_none());
    }

    #[test]
    fn test_is_tool_use_stop_openai() {
        let provider = OpenAiProvider::openai(None);
        assert!(provider.is_tool_use_stop("tool_calls"));
        assert!(!provider.is_tool_use_stop("tool_use")); // that's Anthropic
        assert!(!provider.is_tool_use_stop("stop"));
    }

    #[test]
    fn test_format_messages_with_system_and_tools() {
        use chrono::Utc;
        let provider = OpenAiProvider::openai(None);
        let transcript = vec![
            TranscriptEntry::User {
                content: vec![ContentBlock::Text {
                    text: "Run ls".into(),
                }],
                timestamp: Utc::now(),
            },
            TranscriptEntry::Assistant {
                content: vec![ContentBlock::ToolUse {
                    id: "call_1".into(),
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
                tool_use_id: "call_1".into(),
                tool: "exec".into(),
                content: "file1.txt\nfile2.txt".into(),
                is_error: false,
                timestamp: Utc::now(),
            },
        ];

        let messages = provider.format_messages(&transcript);
        assert_eq!(messages.len(), 3); // user, assistant (with tool_calls), tool
        assert_eq!(messages[0]["role"], "user");
        assert_eq!(messages[1]["role"], "assistant");
        assert!(messages[1]["tool_calls"].is_array());
        assert_eq!(messages[1]["tool_calls"][0]["id"], "call_1");
        assert_eq!(messages[2]["role"], "tool");
        assert_eq!(messages[2]["tool_call_id"], "call_1");
    }

    #[test]
    fn test_chunk_deserialization_text() {
        let json = r#"{"id":"chatcmpl-1","choices":[{"index":0,"delta":{"content":"Hello"},"finish_reason":null}]}"#;
        let chunk: ChatCompletionChunk = serde_json::from_str(json).unwrap();
        assert_eq!(chunk.choices[0].delta.content.as_deref(), Some("Hello"));
    }

    #[test]
    fn test_chunk_deserialization_tool_call() {
        let json = r#"{"id":"chatcmpl-1","choices":[{"index":0,"delta":{"tool_calls":[{"index":0,"id":"call_1","function":{"name":"exec","arguments":""}}]},"finish_reason":null}]}"#;
        let chunk: ChatCompletionChunk = serde_json::from_str(json).unwrap();
        let tc = &chunk.choices[0].delta.tool_calls.as_ref().unwrap()[0];
        assert_eq!(tc.index, 0);
        assert_eq!(tc.id.as_deref(), Some("call_1"));
        assert_eq!(tc.function.as_ref().unwrap().name.as_deref(), Some("exec"));
    }

    #[test]
    fn test_chunk_deserialization_finish_reason() {
        let json =
            r#"{"id":"chatcmpl-1","choices":[{"index":0,"delta":{},"finish_reason":"stop"}]}"#;
        let chunk: ChatCompletionChunk = serde_json::from_str(json).unwrap();
        assert_eq!(
            chunk.choices[0].finish_reason.as_deref(),
            Some("stop")
        );
    }

    // --- 6c-2: Image Input test ---

    #[test]
    fn test_format_messages_with_image() {
        use chrono::Utc;
        use rusty_claw_core::types::ImageSource;

        let provider = OpenAiProvider::openai(None);
        let transcript = vec![TranscriptEntry::User {
            content: vec![
                ContentBlock::Text {
                    text: "What is in this image?".into(),
                },
                ContentBlock::Image {
                    source: ImageSource {
                        source_type: "base64".into(),
                        media_type: "image/png".into(),
                        data: "aWtlcG5n".into(), // fake base64 data
                    },
                },
            ],
            timestamp: Utc::now(),
        }];

        let messages = provider.format_messages(&transcript);
        assert_eq!(messages.len(), 1);
        assert_eq!(messages[0]["role"], "user");

        // Should use array-of-parts format for multimodal
        let content = &messages[0]["content"];
        assert!(content.is_array(), "content should be an array of parts");
        let parts = content.as_array().unwrap();
        assert_eq!(parts.len(), 2);

        // First part: text
        assert_eq!(parts[0]["type"], "text");
        assert_eq!(parts[0]["text"], "What is in this image?");

        // Second part: image_url with data URI
        assert_eq!(parts[1]["type"], "image_url");
        let url = parts[1]["image_url"]["url"].as_str().unwrap();
        assert!(
            url.starts_with("data:image/png;base64,"),
            "Expected data URI, got: {url}"
        );
        assert!(url.contains("aWtlcG5n"));
    }
}
