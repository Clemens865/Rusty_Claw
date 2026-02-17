//! Speech-to-text from raw audio bytes.

use anyhow::Result;
use tracing::debug;

use rusty_claw_core::config::TranscriptionConfig;

/// Wrap raw 16-bit PCM in a WAV container.
pub fn pcm_to_wav(pcm: &[i16], sample_rate: u32, channels: u16, bits_per_sample: u16) -> Vec<u8> {
    let data_len = pcm.len() * 2; // 2 bytes per i16 sample
    let byte_rate = sample_rate * channels as u32 * bits_per_sample as u32 / 8;
    let block_align = channels * bits_per_sample / 8;
    let file_size = 36 + data_len as u32;

    let mut wav = Vec::with_capacity(44 + data_len);

    // RIFF header
    wav.extend_from_slice(b"RIFF");
    wav.extend_from_slice(&file_size.to_le_bytes());
    wav.extend_from_slice(b"WAVE");

    // fmt chunk
    wav.extend_from_slice(b"fmt ");
    wav.extend_from_slice(&16u32.to_le_bytes()); // chunk size
    wav.extend_from_slice(&1u16.to_le_bytes()); // PCM format
    wav.extend_from_slice(&channels.to_le_bytes());
    wav.extend_from_slice(&sample_rate.to_le_bytes());
    wav.extend_from_slice(&byte_rate.to_le_bytes());
    wav.extend_from_slice(&block_align.to_le_bytes());
    wav.extend_from_slice(&bits_per_sample.to_le_bytes());

    // data chunk
    wav.extend_from_slice(b"data");
    wav.extend_from_slice(&(data_len as u32).to_le_bytes());
    for &sample in pcm {
        wav.extend_from_slice(&sample.to_le_bytes());
    }

    wav
}

/// Get the transcription API URL for a given provider.
pub fn provider_url(config: &TranscriptionConfig) -> &str {
    match config.provider.as_str() {
        "openai" => "https://api.openai.com/v1/audio/transcriptions",
        "groq" => "https://api.groq.com/openai/v1/audio/transcriptions",
        _ => "https://api.groq.com/openai/v1/audio/transcriptions",
    }
}

/// Transcribe raw 16-bit PCM audio bytes using the configured STT provider.
pub async fn transcribe_audio_bytes(
    pcm: &[i16],
    config: &TranscriptionConfig,
) -> Result<String> {
    let api_key = config
        .resolve_api_key()
        .ok_or_else(|| anyhow::anyhow!("No transcription API key configured"))?;

    let wav_data = pcm_to_wav(pcm, 16000, 1, 16);
    let url = provider_url(config);
    let model = config
        .model
        .as_deref()
        .unwrap_or("whisper-large-v3-turbo");

    debug!(url, model, wav_bytes = wav_data.len(), "Sending audio for transcription");

    let part = reqwest::multipart::Part::bytes(wav_data)
        .file_name("audio.wav")
        .mime_str("audio/wav")?;

    let form = reqwest::multipart::Form::new()
        .text("model", model.to_string())
        .text("response_format", "text")
        .part("file", part);

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
        anyhow::bail!("Transcription API error {status}: {body}");
    }

    let text = resp.text().await?;
    Ok(text.trim().to_string())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_wav_header_generation() {
        let pcm = vec![0i16; 16000]; // 1 second at 16kHz
        let wav = pcm_to_wav(&pcm, 16000, 1, 16);

        // WAV header is 44 bytes
        assert_eq!(wav.len(), 44 + 16000 * 2);

        // Check RIFF header
        assert_eq!(&wav[0..4], b"RIFF");
        assert_eq!(&wav[8..12], b"WAVE");
        assert_eq!(&wav[12..16], b"fmt ");
        assert_eq!(&wav[36..40], b"data");

        // Check sample rate (bytes 24-27)
        let sr = u32::from_le_bytes([wav[24], wav[25], wav[26], wav[27]]);
        assert_eq!(sr, 16000);
    }

    #[test]
    fn test_provider_url_selection() {
        let groq = TranscriptionConfig {
            provider: "groq".into(),
            api_key: None,
            api_key_env: None,
            model: None,
        };
        assert!(provider_url(&groq).contains("groq.com"));

        let openai = TranscriptionConfig {
            provider: "openai".into(),
            api_key: None,
            api_key_env: None,
            model: None,
        };
        assert!(provider_url(&openai).contains("openai.com"));
    }
}
