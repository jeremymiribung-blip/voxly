//! Voice Activity Detection (VAD).
//!
//! Voxly will eventually use a high-quality Silero or TEN (wavekat-vad) model
//! running at 30 ms frames (480 samples @ 16 kHz).
//!
//! For the initial architecture we provide:
//! - A simple energy-based VAD (good enough to exercise the pipeline)
//! - The `VoiceActivityDetector` trait that mirrors Handy's design
//! - Support for `set_hangover_frames` (dynamic trailing silence)

use crate::error::Result;

/// Classification of a single analysis frame.
#[derive(Debug, Clone, Copy, PartialEq)]
pub enum VadFrame<'a> {
    /// Contains speech. The payload is the audio that should be kept.
    Speech(&'a [f32]),
    /// Non-speech (silence or noise). Can be dropped for transcription.
    Noise,
}

impl<'a> VadFrame<'a> {
    pub fn is_speech(&self) -> bool {
        matches!(self, VadFrame::Speech(_))
    }
}

/// Trait for any VAD implementation.
/// Frame size is usually fixed (e.g. 480 samples = 30 ms @ 16 kHz).
pub trait VoiceActivityDetector: Send + Sync {
    /// Feed one analysis frame. Returns whether it (plus any prefill/hangover)
    /// should be treated as speech.
    fn push_frame<'a>(&'a mut self, frame: &'a [f32]) -> Result<VadFrame<'a>>;

    /// Convenience wrapper.
    fn is_voice(&mut self, frame: &[f32]) -> Result<bool> {
        Ok(self.push_frame(frame)?.is_speech())
    }

    /// Configure how many post-speech frames we keep (trailing silence).
    /// This is the key "dynamic trailing" parameter that changes between
    /// offline and streaming modes.
    fn set_hangover_frames(&mut self, frames: usize);

    /// Reset internal state (LSTM hidden state for neural VADs, energy history, etc.).
    fn reset(&mut self);
}

/// How VAD filtering should behave for a capture session.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Default)]
pub enum VadPolicy {
    Disabled,
    /// Shorter tail, good for push-to-talk / offline.
    Offline,
    /// Longer post-speech tail for live streaming preview.
    #[default]
    Streaming,
}

/// Extremely simple energy-based VAD used as a placeholder.
/// Threshold is on RMS of the frame.
pub struct SimpleEnergyVad {
    threshold: f32,
    hangover: usize,
    hangover_remaining: usize,
    prefill: Vec<f32>, // simple ring for pre-speech context
}

impl SimpleEnergyVad {
    pub fn new(threshold: f32) -> Self {
        Self {
            threshold,
            hangover: 15, // ~450 ms at 30 ms frames
            hangover_remaining: 0,
            prefill: Vec::new(),
        }
    }

    fn rms(frame: &[f32]) -> f32 {
        if frame.is_empty() {
            return 0.0;
        }
        let sum_sq: f32 = frame.iter().map(|s| s * s).sum();
        (sum_sq / frame.len() as f32).sqrt()
    }
}

impl VoiceActivityDetector for SimpleEnergyVad {
    fn push_frame<'a>(&'a mut self, frame: &'a [f32]) -> Result<VadFrame<'a>> {
        let energy = Self::rms(frame);

        if energy > self.threshold {
            self.hangover_remaining = self.hangover;
            // For the placeholder we just return the current frame.
            // A real implementation would return prefill + current + hangover tail.
            Ok(VadFrame::Speech(frame))
        } else if self.hangover_remaining > 0 {
            self.hangover_remaining -= 1;
            Ok(VadFrame::Speech(frame))
        } else {
            Ok(VadFrame::Noise)
        }
    }

    fn set_hangover_frames(&mut self, frames: usize) {
        self.hangover = frames;
    }

    fn reset(&mut self) {
        self.hangover_remaining = 0;
        self.prefill.clear();
    }
}
