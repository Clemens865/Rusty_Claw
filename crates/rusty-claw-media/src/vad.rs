//! Energy-based Voice Activity Detection (VAD).

/// Voice Activity Detector using RMS energy threshold on 16-bit PCM.
pub struct VoiceActivityDetector {
    /// RMS threshold for speech detection.
    threshold: f64,
    /// Minimum consecutive silent frames before declaring speech end.
    min_silent_frames: usize,
    /// Current state: true = speech active.
    speech_active: bool,
    /// Count of consecutive silent frames.
    silent_count: usize,
}

impl VoiceActivityDetector {
    /// Create a new VAD with given RMS threshold and minimum silent frame count.
    pub fn new(threshold: f64, min_silent_frames: usize) -> Self {
        Self {
            threshold,
            min_silent_frames,
            speech_active: false,
            silent_count: 0,
        }
    }

    /// Create with sensible defaults for 16kHz 20ms frames.
    pub fn default_16khz() -> Self {
        Self::new(300.0, 15) // ~300ms of silence at 20ms frames
    }

    /// Compute RMS energy of a PCM frame.
    pub fn rms(samples: &[i16]) -> f64 {
        if samples.is_empty() {
            return 0.0;
        }
        let sum: f64 = samples.iter().map(|&s| (s as f64) * (s as f64)).sum();
        (sum / samples.len() as f64).sqrt()
    }

    /// Process a single audio frame.
    ///
    /// Returns:
    /// - `Some(true)` — speech just ended (utterance complete)
    /// - `Some(false)` — speech just started
    /// - `None` — no state change
    pub fn process_frame(&mut self, pcm: &[i16]) -> Option<bool> {
        let energy = Self::rms(pcm);
        let is_speech = energy > self.threshold;

        if is_speech {
            self.silent_count = 0;
            if !self.speech_active {
                self.speech_active = true;
                return Some(false); // speech started
            }
        } else if self.speech_active {
            self.silent_count += 1;
            if self.silent_count >= self.min_silent_frames {
                self.speech_active = false;
                self.silent_count = 0;
                return Some(true); // speech ended
            }
        }

        None
    }

    /// Whether speech is currently active.
    pub fn is_active(&self) -> bool {
        self.speech_active
    }

    /// Reset the detector state.
    pub fn reset(&mut self) {
        self.speech_active = false;
        self.silent_count = 0;
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_rms_calculation() {
        // Silence
        let silence = vec![0i16; 320];
        assert_eq!(VoiceActivityDetector::rms(&silence), 0.0);

        // Known signal
        let signal = vec![100i16; 320];
        let rms = VoiceActivityDetector::rms(&signal);
        assert!((rms - 100.0).abs() < 0.01);

        // Empty
        assert_eq!(VoiceActivityDetector::rms(&[]), 0.0);
    }

    #[test]
    fn test_vad_transitions() {
        let mut vad = VoiceActivityDetector::new(50.0, 3);

        // Silence — no change
        let silence = vec![0i16; 320];
        assert_eq!(vad.process_frame(&silence), None);

        // Speech starts
        let speech = vec![500i16; 320];
        assert_eq!(vad.process_frame(&speech), Some(false)); // started

        // Continued speech — no change
        assert_eq!(vad.process_frame(&speech), None);

        // One silent frame — not enough
        assert_eq!(vad.process_frame(&silence), None);

        // Two more silent frames — speech ends at 3
        assert_eq!(vad.process_frame(&silence), None);
        assert_eq!(vad.process_frame(&silence), Some(true)); // ended
        assert!(!vad.is_active());
    }

    #[test]
    fn test_vad_reset() {
        let mut vad = VoiceActivityDetector::new(50.0, 3);
        let speech = vec![500i16; 320];
        vad.process_frame(&speech);
        assert!(vad.is_active());
        vad.reset();
        assert!(!vad.is_active());
    }
}
