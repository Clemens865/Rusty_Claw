//! Voice transcription tool using Whisper via Groq or OpenAI.

use async_trait::async_trait;
use serde_json::json;
use tracing::info;

use crate::{Tool, ToolContext, ToolOutput};

pub struct TranscriptionTool;

/// Get the API URL for the given provider.
fn provider_url(provider: &str) -> &str {
    match provider {
        "openai" => "https://api.openai.com/v1/audio/transcriptions",
        _ => "https://api.groq.com/openai/v1/audio/transcriptions",
    }
}

/// Get the default model for the given provider.
fn default_model(provider: &str) -> &str {
    match provider {
        "openai" => "whisper-1",
        _ => "whisper-large-v3-turbo",
    }
}

#[async_trait]
impl Tool for TranscriptionTool {
    fn name(&self) -> &str {
        "transcribe_audio"
    }

    fn description(&self) -> &str {
        "Transcribe an audio file to text using Whisper (via Groq or OpenAI)."
    }

    fn parameters_schema(&self) -> serde_json::Value {
        json!({
            "type": "object",
            "properties": {
                "file_path": {
                    "type": "string",
                    "description": "Path to the audio file to transcribe"
                },
                "language": {
                    "type": "string",
                    "description": "ISO 639-1 language code (e.g. 'en', 'de'). Optional."
                },
                "prompt": {
                    "type": "string",
                    "description": "Optional prompt to guide the transcription style"
                }
            },
            "required": ["file_path"]
        })
    }

    async fn execute(
        &self,
        params: serde_json::Value,
        context: &ToolContext,
    ) -> anyhow::Result<ToolOutput> {
        let file_path = params
            .get("file_path")
            .and_then(|v| v.as_str())
            .ok_or_else(|| anyhow::anyhow!("missing 'file_path' parameter"))?;

        let language = params.get("language").and_then(|v| v.as_str());
        let prompt = params.get("prompt").and_then(|v| v.as_str());

        // Check file exists
        let path = std::path::Path::new(file_path);
        if !path.exists() {
            return Ok(ToolOutput {
                content: format!("Audio file not found: {file_path}"),
                is_error: true,
                media: None,
            });
        }

        let transcription_config = context
            .config
            .tools
            .as_ref()
            .and_then(|t| t.transcription.as_ref());

        let provider = transcription_config
            .map(|c| c.provider.as_str())
            .unwrap_or("groq");

        let api_key = transcription_config
            .and_then(|c| c.resolve_api_key())
            .or_else(|| {
                let env_var = match provider {
                    "openai" => "OPENAI_API_KEY",
                    _ => "GROQ_API_KEY",
                };
                std::env::var(env_var).ok().filter(|v| !v.is_empty())
            });

        let api_key = match api_key {
            Some(key) => key,
            None => {
                return Ok(ToolOutput {
                    content: format!(
                        "Transcription not configured. Set tools.transcription.api_key in config or {} environment variable.",
                        if provider == "openai" { "OPENAI_API_KEY" } else { "GROQ_API_KEY" }
                    ),
                    is_error: true,
                    media: None,
                });
            }
        };

        let model = transcription_config
            .and_then(|c| c.model.as_deref())
            .unwrap_or_else(|| default_model(provider));

        let url = provider_url(provider);

        // Read the file
        let file_bytes = tokio::fs::read(path).await?;
        let file_name = path
            .file_name()
            .map(|n| n.to_string_lossy().to_string())
            .unwrap_or_else(|| "audio.mp3".into());

        // Build multipart form
        let file_part = reqwest::multipart::Part::bytes(file_bytes)
            .file_name(file_name.clone())
            .mime_str("application/octet-stream")?;

        let mut form = reqwest::multipart::Form::new()
            .part("file", file_part)
            .text("model", model.to_string())
            .text("response_format", "text");

        if let Some(lang) = language {
            form = form.text("language", lang.to_string());
        }
        if let Some(p) = prompt {
            form = form.text("prompt", p.to_string());
        }

        let client = reqwest::Client::new();
        let resp = client
            .post(url)
            .header("Authorization", format!("Bearer {api_key}"))
            .multipart(form)
            .send()
            .await?;

        if !resp.status().is_success() {
            let status = resp.status();
            let body = resp.text().await.unwrap_or_default();
            return Ok(ToolOutput {
                content: format!("Transcription API error ({status}): {body}"),
                is_error: true,
                media: None,
            });
        }

        let transcript = resp.text().await?;
        let transcript = transcript.trim().to_string();

        info!(
            provider,
            model,
            file = file_name,
            chars = transcript.len(),
            "Audio transcribed"
        );

        Ok(ToolOutput {
            content: transcript,
            is_error: false,
            media: None,
        })
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_parameters_schema_has_required_file_path() {
        let tool = TranscriptionTool;
        let schema = tool.parameters_schema();
        let required = schema["required"].as_array().unwrap();
        assert!(required.contains(&json!("file_path")));
    }

    #[test]
    fn test_provider_url_selection() {
        assert_eq!(
            provider_url("groq"),
            "https://api.groq.com/openai/v1/audio/transcriptions"
        );
        assert_eq!(
            provider_url("openai"),
            "https://api.openai.com/v1/audio/transcriptions"
        );
        // Default is groq
        assert_eq!(
            provider_url("unknown"),
            "https://api.groq.com/openai/v1/audio/transcriptions"
        );
    }

    #[tokio::test]
    async fn test_missing_file_returns_error() {
        let ctx = ToolContext {
            session_key: "test".into(),
            workspace: std::env::temp_dir(),
            config: std::sync::Arc::new(rusty_claw_core::config::Config::default()),
            restrict_to_workspace: false,
            sandbox_mode: rusty_claw_core::config::SandboxMode::default(),
            browser_pool: None,
        };

        let result = TranscriptionTool
            .execute(
                json!({"file_path": "/nonexistent/audio.mp3"}),
                &ctx,
            )
            .await
            .unwrap();
        assert!(result.is_error);
        assert!(result.content.contains("not found"));
    }
}
