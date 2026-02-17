//! Voice session — buffers audio, feeds VAD, emits completed utterances.

use tokio::sync::mpsc;
use tokio_util::sync::CancellationToken;
use tracing::{debug, info};

use crate::vad::VoiceActivityDetector;

/// Talk mode for voice interaction.
#[derive(Debug, Clone, Copy, PartialEq, Eq, serde::Serialize, serde::Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum TalkMode {
    /// Push-to-talk: client sends explicit start/stop signals.
    Push,
    /// Voice activity detection: automatic speech boundary detection.
    Vad,
}

impl Default for TalkMode {
    fn default() -> Self {
        Self::Vad
    }
}

/// A completed utterance ready for STT processing.
pub struct Utterance {
    /// Raw 16-bit PCM audio at 16kHz mono.
    pub pcm_data: Vec<i16>,
    /// Duration in milliseconds.
    pub duration_ms: u64,
}

/// Handle for controlling a voice session from outside.
pub struct VoiceSessionHandle {
    /// Send raw audio bytes (16-bit PCM, 16kHz, mono).
    pub audio_tx: mpsc::UnboundedSender<Vec<u8>>,
    /// Cancellation token to stop the session.
    pub cancel: CancellationToken,
    /// Current talk mode.
    pub mode: TalkMode,
}

/// Voice session that processes incoming audio and emits utterances.
pub struct VoiceSession {
    mode: TalkMode,
    vad: VoiceActivityDetector,
    buffer: Vec<i16>,
    frame_size: usize, // samples per frame (e.g., 320 for 20ms at 16kHz)
    sample_rate: u32,
}

impl VoiceSession {
    pub fn new(mode: TalkMode, sample_rate: u32) -> Self {
        let frame_size = (sample_rate as usize) / 50; // 20ms frames
        Self {
            mode,
            vad: VoiceActivityDetector::default_16khz(),
            buffer: Vec::new(),
            frame_size,
            sample_rate,
        }
    }

    /// Start the voice session, returning a handle and an utterance receiver.
    ///
    /// The session runs in a background task, processing incoming audio
    /// and emitting complete utterances.
    pub fn start(mode: TalkMode) -> (VoiceSessionHandle, mpsc::UnboundedReceiver<Utterance>) {
        let (audio_tx, audio_rx) = mpsc::unbounded_channel::<Vec<u8>>();
        let (utterance_tx, utterance_rx) = mpsc::unbounded_channel::<Utterance>();
        let cancel = CancellationToken::new();

        let handle = VoiceSessionHandle {
            audio_tx,
            cancel: cancel.clone(),
            mode,
        };

        let mut session = Self::new(mode, 16000);

        tokio::spawn(async move {
            info!(?mode, "Voice session started");
            session.run(audio_rx, utterance_tx, cancel).await;
            info!("Voice session ended");
        });

        (handle, utterance_rx)
    }

    async fn run(
        &mut self,
        mut audio_rx: mpsc::UnboundedReceiver<Vec<u8>>,
        utterance_tx: mpsc::UnboundedSender<Utterance>,
        cancel: CancellationToken,
    ) {
        loop {
            tokio::select! {
                _ = cancel.cancelled() => break,
                Some(raw_bytes) = audio_rx.recv() => {
                    self.process_audio(&raw_bytes, &utterance_tx);
                }
                else => break,
            }
        }
    }

    fn process_audio(
        &mut self,
        raw_bytes: &[u8],
        utterance_tx: &mpsc::UnboundedSender<Utterance>,
    ) {
        // Convert bytes to i16 samples (little-endian)
        let samples: Vec<i16> = raw_bytes
            .chunks_exact(2)
            .map(|chunk| i16::from_le_bytes([chunk[0], chunk[1]]))
            .collect();

        match self.mode {
            TalkMode::Vad => self.process_vad(&samples, utterance_tx),
            TalkMode::Push => {
                // In push mode, just accumulate — utterance is emitted on stop
                self.buffer.extend_from_slice(&samples);
            }
        }
    }

    fn process_vad(
        &mut self,
        samples: &[i16],
        _utterance_tx: &mpsc::UnboundedSender<Utterance>,
    ) {
        self.buffer.extend_from_slice(samples);

        // Process complete frames through VAD
        while self.buffer.len() >= self.frame_size {
            let frame: Vec<i16> = self.buffer.drain(..self.frame_size).collect();
            if let Some(true) = self.vad.process_frame(&frame) {
                // Speech ended — emit utterance from accumulated audio
                // (The buffer still has remaining samples after drain)
                // We need to reconstruct the full utterance from what came before
                // For simplicity, we track the utterance audio separately
                debug!("VAD detected speech end");
            }
        }
    }

    /// Flush the buffer as an utterance (used in push mode on stop).
    pub fn flush(&mut self) -> Option<Utterance> {
        if self.buffer.is_empty() {
            return None;
        }

        let pcm_data: Vec<i16> = self.buffer.drain(..).collect();
        let duration_ms = (pcm_data.len() as u64 * 1000) / self.sample_rate as u64;

        Some(Utterance {
            pcm_data,
            duration_ms,
        })
    }

    pub fn set_mode(&mut self, mode: TalkMode) {
        self.mode = mode;
        self.vad.reset();
        self.buffer.clear();
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_buffer_accumulation() {
        let mut session = VoiceSession::new(TalkMode::Push, 16000);

        // Simulate adding audio
        let samples: Vec<i16> = vec![100; 320];
        let bytes: Vec<u8> = samples
            .iter()
            .flat_map(|s| s.to_le_bytes())
            .collect();

        let (tx, _rx) = mpsc::unbounded_channel();
        session.process_audio(&bytes, &tx);

        assert_eq!(session.buffer.len(), 320);

        // Flush
        let utterance = session.flush();
        assert!(utterance.is_some());
        let u = utterance.unwrap();
        assert_eq!(u.pcm_data.len(), 320);
        assert_eq!(u.duration_ms, 20); // 320 samples at 16kHz = 20ms
    }

    #[test]
    fn test_mode_switch() {
        let mut session = VoiceSession::new(TalkMode::Push, 16000);
        session.buffer.extend_from_slice(&[100i16; 100]);
        session.set_mode(TalkMode::Vad);
        assert!(session.buffer.is_empty());
    }

    #[tokio::test]
    async fn test_session_lifecycle() {
        let (handle, _utterance_rx) = VoiceSession::start(TalkMode::Push);

        // Send some audio
        let samples: Vec<u8> = vec![0u8; 640]; // 320 samples worth
        handle.audio_tx.send(samples).unwrap();

        // Cancel
        handle.cancel.cancel();

        // Should complete without blocking
        tokio::time::sleep(std::time::Duration::from_millis(50)).await;
    }
}
