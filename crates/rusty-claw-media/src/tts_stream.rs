//! Streaming TTS — sends audio chunks as they arrive from the provider.

use anyhow::Result;
use futures::StreamExt;
use tokio::sync::mpsc;
use tracing::debug;

use rusty_claw_core::config::TtsConfig;

/// Stream TTS audio from the configured provider, sending chunks as they arrive.
///
/// Audio is streamed as raw PCM 16-bit 16kHz mono bytes.
pub async fn stream_tts(
    text: &str,
    config: &TtsConfig,
    chunk_tx: mpsc::UnboundedSender<Vec<u8>>,
) -> Result<()> {
    let api_key = config
        .resolve_api_key()
        .ok_or_else(|| anyhow::anyhow!("No TTS API key configured"))?;

    let voice = config.default_voice.as_deref().unwrap_or("Rachel");
    let model = config.default_model.as_deref().unwrap_or("eleven_turbo_v2");

    let url = format!(
        "https://api.elevenlabs.io/v1/text-to-speech/{voice}/stream"
    );

    debug!(voice, model, text_len = text.len(), "Starting TTS stream");

    let client = reqwest::Client::new();
    let resp = client
        .post(&url)
        .header("xi-api-key", &api_key)
        .header("Content-Type", "application/json")
        .json(&serde_json::json!({
            "text": text,
            "model_id": model,
            "output_format": "pcm_16000",
        }))
        .send()
        .await?;

    if !resp.status().is_success() {
        let status = resp.status();
        let body = resp.text().await.unwrap_or_default();
        anyhow::bail!("TTS API error {status}: {body}");
    }

    let mut stream = resp.bytes_stream();

    while let Some(chunk_result) = stream.next().await {
        match chunk_result {
            Ok(bytes) => {
                if chunk_tx.send(bytes.to_vec()).is_err() {
                    debug!("TTS chunk receiver dropped, stopping stream");
                    break;
                }
            }
            Err(e) => {
                anyhow::bail!("TTS stream error: {e}");
            }
        }
    }

    Ok(())
}

/// Build the ElevenLabs streaming TTS request URL for a given voice.
pub fn build_tts_url(voice: &str) -> String {
    format!("https://api.elevenlabs.io/v1/text-to-speech/{voice}/stream")
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_request_construction() {
        let url = build_tts_url("Rachel");
        assert!(url.contains("Rachel"));
        assert!(url.contains("stream"));
        assert!(url.starts_with("https://api.elevenlabs.io"));
    }
}
