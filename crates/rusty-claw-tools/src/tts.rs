//! Text-to-speech tool using ElevenLabs API.

use async_trait::async_trait;
use serde_json::json;
use tracing::info;

use crate::{Tool, ToolContext, ToolOutput};

const DEFAULT_VOICE_ID: &str = "21m00Tcm4TlvDq8ikWAM"; // ElevenLabs "Rachel"
const DEFAULT_MODEL: &str = "eleven_monolingual_v1";
const DEFAULT_OUTPUT_FORMAT: &str = "mp3_44100_128";

pub struct TtsTool;

/// Generate a unique output filename.
fn output_filename(format: &str) -> std::path::PathBuf {
    let dir = rusty_claw_core::config::data_dir().join("audio");
    let ts = chrono::Utc::now().format("%Y%m%d_%H%M%S");
    let id = uuid::Uuid::new_v4().simple().to_string();
    let ext = match format {
        f if f.starts_with("mp3") => "mp3",
        f if f.starts_with("pcm") => "pcm",
        f if f.starts_with("ulaw") => "ulaw",
        _ => "mp3",
    };
    dir.join(format!("tts_{ts}_{}.{ext}", &id[..8]))
}

#[async_trait]
impl Tool for TtsTool {
    fn name(&self) -> &str {
        "tts"
    }

    fn description(&self) -> &str {
        "Convert text to speech using ElevenLabs. Returns the path to the generated audio file."
    }

    fn parameters_schema(&self) -> serde_json::Value {
        json!({
            "type": "object",
            "properties": {
                "text": {
                    "type": "string",
                    "description": "The text to convert to speech"
                },
                "voice_id": {
                    "type": "string",
                    "description": "ElevenLabs voice ID (optional, uses default voice)"
                },
                "model_id": {
                    "type": "string",
                    "description": "ElevenLabs model ID (optional)"
                },
                "output_format": {
                    "type": "string",
                    "description": "Output format (e.g. mp3_44100_128)"
                }
            },
            "required": ["text"]
        })
    }

    async fn execute(
        &self,
        params: serde_json::Value,
        context: &ToolContext,
    ) -> anyhow::Result<ToolOutput> {
        let text = params
            .get("text")
            .and_then(|v| v.as_str())
            .ok_or_else(|| anyhow::anyhow!("missing 'text' parameter"))?;

        // Resolve TTS config
        let tts_config = context
            .config
            .tools
            .as_ref()
            .and_then(|t| t.tts.as_ref());

        let api_key = tts_config
            .and_then(|c| c.resolve_api_key())
            .or_else(|| std::env::var("ELEVENLABS_API_KEY").ok().filter(|v| !v.is_empty()));

        let api_key = match api_key {
            Some(key) => key,
            None => {
                return Ok(ToolOutput {
                    content: "TTS not configured. Set tools.tts.api_key in config or ELEVENLABS_API_KEY environment variable.".into(),
                    is_error: true,
                    media: None,
                });
            }
        };

        let voice_id = params
            .get("voice_id")
            .and_then(|v| v.as_str())
            .or_else(|| tts_config.and_then(|c| c.default_voice.as_deref()))
            .unwrap_or(DEFAULT_VOICE_ID);

        let model_id = params
            .get("model_id")
            .and_then(|v| v.as_str())
            .or_else(|| tts_config.and_then(|c| c.default_model.as_deref()))
            .unwrap_or(DEFAULT_MODEL);

        let output_format = params
            .get("output_format")
            .and_then(|v| v.as_str())
            .or_else(|| tts_config.and_then(|c| c.output_format.as_deref()))
            .unwrap_or(DEFAULT_OUTPUT_FORMAT);

        let url = format!(
            "https://api.elevenlabs.io/v1/text-to-speech/{voice_id}/stream?output_format={output_format}"
        );

        let client = reqwest::Client::new();
        let resp = client
            .post(&url)
            .header("xi-api-key", &api_key)
            .header("Content-Type", "application/json")
            .json(&json!({
                "text": text,
                "model_id": model_id,
                "voice_settings": {
                    "stability": 0.5,
                    "similarity_boost": 0.75
                }
            }))
            .send()
            .await?;

        if !resp.status().is_success() {
            let status = resp.status();
            let body = resp.text().await.unwrap_or_default();
            return Ok(ToolOutput {
                content: format!("ElevenLabs API error ({status}): {body}"),
                is_error: true,
                media: None,
            });
        }

        let bytes = resp.bytes().await?;
        let file_path = output_filename(output_format);

        // Ensure directory exists
        if let Some(parent) = file_path.parent() {
            tokio::fs::create_dir_all(parent).await?;
        }
        tokio::fs::write(&file_path, &bytes).await?;

        let size_kb = bytes.len() / 1024;
        info!(
            path = %file_path.display(),
            size_kb,
            voice = voice_id,
            model = model_id,
            "TTS audio generated"
        );

        Ok(ToolOutput {
            content: format!(
                "Audio saved to: {}\nSize: {}KB\nVoice: {}\nModel: {}",
                file_path.display(),
                size_kb,
                voice_id,
                model_id,
            ),
            is_error: false,
            media: None,
        })
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_parameters_schema_has_required_text() {
        let tool = TtsTool;
        let schema = tool.parameters_schema();
        let required = schema["required"].as_array().unwrap();
        assert!(required.contains(&json!("text")));
    }

    #[test]
    fn test_filename_generation_unique() {
        let f1 = output_filename("mp3_44100_128");
        let f2 = output_filename("mp3_44100_128");
        assert_ne!(f1, f2);
        assert!(f1.extension().unwrap() == "mp3");
    }

    #[tokio::test]
    async fn test_missing_config_returns_error() {
        let ctx = ToolContext {
            session_key: "test".into(),
            workspace: std::env::temp_dir(),
            config: std::sync::Arc::new(rusty_claw_core::config::Config::default()),
            restrict_to_workspace: false,
        };

        // Ensure env var is not set for this test
        let saved = std::env::var("ELEVENLABS_API_KEY").ok();
        unsafe { std::env::remove_var("ELEVENLABS_API_KEY") };

        let result = TtsTool
            .execute(json!({"text": "hello"}), &ctx)
            .await
            .unwrap();
        assert!(result.is_error);
        assert!(result.content.contains("not configured"));

        // Restore env var if it was set
        if let Some(val) = saved {
            unsafe { std::env::set_var("ELEVENLABS_API_KEY", val) };
        }
    }
}
