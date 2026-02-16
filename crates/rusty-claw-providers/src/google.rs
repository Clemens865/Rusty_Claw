//! Google Generative AI (Gemini) provider.
//!
//! Implements streaming via the `streamGenerateContent` endpoint with SSE.
//! Auth is via API key in query parameter.

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

const DEFAULT_BASE_URL: &str = "https://generativelanguage.googleapis.com";

pub struct GeminiProvider {
    pub base_url: String,
    client: reqwest::Client,
}

impl GeminiProvider {
    pub fn new(base_url: Option<&str>) -> Self {
        Self {
            base_url: base_url
                .unwrap_or(DEFAULT_BASE_URL)
                .trim_end_matches('/')
                .to_string(),
            client: reqwest::Client::new(),
        }
    }
}

// --- Gemini request/response types ---

#[derive(Debug, Serialize)]
#[serde(rename_all = "camelCase")]
struct GeminiRequest {
    contents: Vec<serde_json::Value>,
    #[serde(skip_serializing_if = "Option::is_none")]
    system_instruction: Option<serde_json::Value>,
    #[serde(skip_serializing_if = "Option::is_none")]
    tools: Option<Vec<serde_json::Value>>,
    #[serde(skip_serializing_if = "Option::is_none")]
    generation_config: Option<GenerationConfig>,
}

#[derive(Debug, Serialize)]
#[serde(rename_all = "camelCase")]
struct GenerationConfig {
    #[serde(skip_serializing_if = "Option::is_none")]
    max_output_tokens: Option<u32>,
    #[serde(skip_serializing_if = "Option::is_none")]
    temperature: Option<f64>,
}

#[derive(Debug, Deserialize)]
#[serde(rename_all = "camelCase")]
struct GeminiStreamChunk {
    #[serde(default)]
    candidates: Vec<Candidate>,
    #[serde(default)]
    usage_metadata: Option<UsageMetadata>,
}

#[derive(Debug, Deserialize)]
#[serde(rename_all = "camelCase")]
struct Candidate {
    #[serde(default)]
    content: Option<CandidateContent>,
    #[serde(default)]
    finish_reason: Option<String>,
}

#[derive(Debug, Deserialize)]
struct CandidateContent {
    #[serde(default)]
    parts: Vec<GeminiPart>,
}

#[derive(Debug, Deserialize)]
#[serde(rename_all = "camelCase")]
struct GeminiPart {
    #[serde(default)]
    text: Option<String>,
    #[serde(default)]
    function_call: Option<FunctionCallPart>,
}

#[derive(Debug, Deserialize)]
struct FunctionCallPart {
    name: String,
    #[serde(default)]
    args: Option<serde_json::Value>,
}

#[derive(Debug, Deserialize)]
#[serde(rename_all = "camelCase")]
struct UsageMetadata {
    #[serde(default)]
    prompt_token_count: u64,
    #[serde(default)]
    candidates_token_count: u64,
}

#[async_trait]
impl LlmProvider for GeminiProvider {
    fn id(&self) -> &str {
        "google"
    }

    fn api(&self) -> ModelApi {
        ModelApi::GoogleGenerativeAi
    }

    fn format_tools(&self, tools: &[ToolDefinition]) -> Vec<serde_json::Value> {
        // Gemini wraps tools in a single object with function_declarations array
        let declarations: Vec<serde_json::Value> = tools
            .iter()
            .map(|t| {
                json!({
                    "name": t.name,
                    "description": t.description,
                    "parameters": t.parameters_schema,
                })
            })
            .collect();

        vec![json!({ "function_declarations": declarations })]
    }

    fn format_messages(&self, transcript: &[TranscriptEntry]) -> Vec<serde_json::Value> {
        let mut contents: Vec<serde_json::Value> = Vec::new();

        for entry in transcript {
            match entry {
                TranscriptEntry::User { content, .. } => {
                    let parts: Vec<serde_json::Value> = content
                        .iter()
                        .filter_map(|b| match b {
                            ContentBlock::Text { text } => Some(json!({ "text": text })),
                            _ => None,
                        })
                        .collect();
                    if !parts.is_empty() {
                        contents.push(json!({ "role": "user", "parts": parts }));
                    }
                }
                TranscriptEntry::Assistant { content, .. } => {
                    let mut parts = Vec::new();
                    for block in content {
                        match block {
                            ContentBlock::Text { text } => {
                                parts.push(json!({ "text": text }));
                            }
                            ContentBlock::ToolUse { name, input, .. } => {
                                parts.push(json!({
                                    "functionCall": {
                                        "name": name,
                                        "args": input,
                                    }
                                }));
                            }
                            _ => {}
                        }
                    }
                    if !parts.is_empty() {
                        contents.push(json!({ "role": "model", "parts": parts }));
                    }
                }
                TranscriptEntry::ToolResult {
                    tool: tool_name,
                    content,
                    ..
                } => {
                    contents.push(json!({
                        "role": "user",
                        "parts": [{
                            "functionResponse": {
                                "name": tool_name,
                                "response": {
                                    "content": content,
                                }
                            }
                        }]
                    }));
                }
                TranscriptEntry::ToolCall { .. } | TranscriptEntry::System { .. } => {}
            }
        }

        contents
    }

    fn is_tool_use_stop(&self, stop_reason: &str) -> bool {
        // Gemini doesn't use a stop reason for tool use; instead,
        // the presence of functionCall parts indicates tool use.
        // We map it to "TOOL_USE" internally when we detect functionCall parts.
        stop_reason == "TOOL_USE"
    }

    async fn stream(
        &self,
        request: &CompletionRequest,
        credentials: &Credentials,
    ) -> anyhow::Result<Pin<Box<dyn Stream<Item = anyhow::Result<CompletionChunk>> + Send>>> {
        let api_key = match credentials {
            Credentials::ApiKey { api_key } => api_key.clone(),
            _ => anyhow::bail!("Gemini requires ApiKey credentials"),
        };

        let system_instruction = request.system.as_ref().map(|s| {
            json!({
                "parts": [{ "text": s }]
            })
        });

        let body = GeminiRequest {
            contents: request.messages.clone(),
            system_instruction,
            tools: request.tools.clone(),
            generation_config: Some(GenerationConfig {
                max_output_tokens: Some(request.max_tokens),
                temperature: request.temperature,
            }),
        };

        let url = format!(
            "{}/v1beta/models/{}:streamGenerateContent?alt=sse&key={}",
            self.base_url, request.model, api_key
        );

        debug!(model = %request.model, "Streaming Gemini API");

        let response = self
            .client
            .post(&url)
            .header("content-type", "application/json")
            .json(&body)
            .send()
            .await?;

        if !response.status().is_success() {
            let status = response.status();
            let body = response.text().await.unwrap_or_default();
            anyhow::bail!("Gemini API error {status}: {body}");
        }

        let sse_stream = parse_sse_stream(response);

        let chunk_stream = futures::stream::unfold(
            GeminiChunkState {
                sse: Box::pin(sse_stream),
                tool_call_counter: 0,
            },
            |mut state| async move {
                loop {
                    match state.sse.next().await {
                        Some(Ok(sse_event)) => {
                            let data = sse_event.data.trim();
                            let chunk: GeminiStreamChunk = match serde_json::from_str(data) {
                                Ok(c) => c,
                                Err(e) => {
                                    trace!(%e, "Failed to parse Gemini chunk");
                                    continue;
                                }
                            };

                            // Usage
                            if let Some(usage) = chunk.usage_metadata {
                                let c = CompletionChunk {
                                    delta: None,
                                    thinking: None,
                                    tool_use: None,
                                    usage: Some(ChunkUsage {
                                        input_tokens: Some(usage.prompt_token_count),
                                        output_tokens: Some(usage.candidates_token_count),
                                    }),
                                    stop_reason: None,
                                };
                                return Some((Ok(c), state));
                            }

                            let candidate = match chunk.candidates.first() {
                                Some(c) => c,
                                None => continue,
                            };

                            // Process parts
                            if let Some(ref content) = candidate.content {
                                for part in &content.parts {
                                    if let Some(ref text) = part.text {
                                        let c = CompletionChunk {
                                            delta: Some(text.clone()),
                                            thinking: None,
                                            tool_use: None,
                                            usage: None,
                                            stop_reason: None,
                                        };
                                        return Some((Ok(c), state));
                                    }

                                    if let Some(ref fc) = part.function_call {
                                        state.tool_call_counter += 1;
                                        let id = format!(
                                            "gemini_call_{}",
                                            state.tool_call_counter
                                        );
                                        let input_json = fc
                                            .args
                                            .as_ref()
                                            .map(|a| a.to_string())
                                            .unwrap_or_else(|| "{}".into());

                                        let c = CompletionChunk {
                                            delta: None,
                                            thinking: None,
                                            tool_use: Some(ToolUseChunk {
                                                id,
                                                name: fc.name.clone(),
                                                input_json,
                                            }),
                                            usage: None,
                                            // Set TOOL_USE stop reason so the agent loop knows
                                            stop_reason: Some("TOOL_USE".into()),
                                        };
                                        return Some((Ok(c), state));
                                    }
                                }
                            }

                            // Finish reason
                            if let Some(ref reason) = candidate.finish_reason {
                                if reason != "STOP" {
                                    trace!(reason, "Gemini finish reason");
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
            _ => anyhow::bail!("Gemini requires ApiKey credentials"),
        };

        // Return well-known Gemini models (the list API requires auth)
        let _ = api_key;
        Ok(vec![
            ModelInfo {
                id: "gemini-2.0-flash".into(),
                name: "Gemini 2.0 Flash".into(),
                api: ModelApi::GoogleGenerativeAi,
                reasoning: false,
                context_window: 1_048_576,
                max_tokens: 8_192,
            },
            ModelInfo {
                id: "gemini-2.0-pro".into(),
                name: "Gemini 2.0 Pro".into(),
                api: ModelApi::GoogleGenerativeAi,
                reasoning: false,
                context_window: 2_097_152,
                max_tokens: 8_192,
            },
            ModelInfo {
                id: "gemini-2.5-flash-preview-05-20".into(),
                name: "Gemini 2.5 Flash".into(),
                api: ModelApi::GoogleGenerativeAi,
                reasoning: true,
                context_window: 1_048_576,
                max_tokens: 65_536,
            },
            ModelInfo {
                id: "gemini-2.5-pro-preview-05-06".into(),
                name: "Gemini 2.5 Pro".into(),
                api: ModelApi::GoogleGenerativeAi,
                reasoning: true,
                context_window: 1_048_576,
                max_tokens: 65_536,
            },
        ])
    }
}

struct GeminiChunkState {
    sse: Pin<Box<dyn Stream<Item = anyhow::Result<crate::sse::SseEvent>> + Send>>,
    tool_call_counter: u32,
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_gemini_provider_creation() {
        let provider = GeminiProvider::new(None);
        assert_eq!(provider.id(), "google");
        assert_eq!(provider.api(), ModelApi::GoogleGenerativeAi);
        assert_eq!(provider.base_url, DEFAULT_BASE_URL);
    }

    #[test]
    fn test_format_tools_function_declarations() {
        let provider = GeminiProvider::new(None);
        let tools = vec![ToolDefinition {
            name: "exec".into(),
            description: "Run a command".into(),
            parameters_schema: json!({"type": "object", "properties": {"cmd": {"type": "string"}}}),
        }];
        let formatted = provider.format_tools(&tools);
        assert_eq!(formatted.len(), 1);
        assert!(formatted[0]["function_declarations"].is_array());
        assert_eq!(formatted[0]["function_declarations"][0]["name"], "exec");
    }

    #[test]
    fn test_format_messages_contents_parts() {
        use chrono::Utc;
        let provider = GeminiProvider::new(None);
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
        assert_eq!(messages[0]["parts"][0]["text"], "Hello");
        assert_eq!(messages[1]["role"], "model"); // Gemini uses "model", not "assistant"
        assert_eq!(messages[1]["parts"][0]["text"], "Hi there");
    }

    #[test]
    fn test_format_messages_with_function_call() {
        use chrono::Utc;
        let provider = GeminiProvider::new(None);
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
                content: "file.txt".into(),
                is_error: false,
                timestamp: Utc::now(),
            },
        ];
        let messages = provider.format_messages(&transcript);
        assert_eq!(messages.len(), 3);
        assert!(messages[1]["parts"][0]["functionCall"].is_object());
        assert!(messages[2]["parts"][0]["functionResponse"].is_object());
    }

    #[test]
    fn test_is_tool_use_stop_gemini() {
        let provider = GeminiProvider::new(None);
        assert!(provider.is_tool_use_stop("TOOL_USE"));
        assert!(!provider.is_tool_use_stop("STOP"));
        assert!(!provider.is_tool_use_stop("tool_use"));
    }

    #[test]
    fn test_gemini_chunk_deserialization() {
        let json = r#"{"candidates":[{"content":{"parts":[{"text":"Hello"}]},"finishReason":"STOP"}],"usageMetadata":{"promptTokenCount":10,"candidatesTokenCount":5}}"#;
        let chunk: GeminiStreamChunk = serde_json::from_str(json).unwrap();
        assert_eq!(chunk.candidates.len(), 1);
        assert_eq!(
            chunk.candidates[0]
                .content
                .as_ref()
                .unwrap()
                .parts[0]
                .text
                .as_deref(),
            Some("Hello")
        );
        assert_eq!(chunk.usage_metadata.unwrap().prompt_token_count, 10);
    }

    #[test]
    fn test_gemini_function_call_chunk() {
        let json = r#"{"candidates":[{"content":{"parts":[{"functionCall":{"name":"exec","args":{"command":"ls"}}}]}}]}"#;
        let chunk: GeminiStreamChunk = serde_json::from_str(json).unwrap();
        let fc = chunk.candidates[0]
            .content
            .as_ref()
            .unwrap()
            .parts[0]
            .function_call
            .as_ref()
            .unwrap();
        assert_eq!(fc.name, "exec");
        assert_eq!(fc.args.as_ref().unwrap()["command"], "ls");
    }
}
