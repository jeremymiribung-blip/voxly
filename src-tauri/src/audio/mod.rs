//! Audio capture, preprocessing, VAD, and chunking pipeline.
//!
//! Responsibilities:
//! - Enumerate input devices (cpal)
//! - Open a capture stream at the device's native rate
//! - Resample to the model's required rate (rubato)
//! - Run voice activity detection with configurable policy (streaming vs offline hangover)
//! - Produce clean 16 kHz (or target) mono f32 frames
//! - Feed frames either to a `StreamRouter` (live preview) or to a buffer for batch transcription
//!
//! The design deliberately separates "capture" from "consumption":
//! - The recorder thread / callback is extremely lightweight.
//! - Actual VAD + chunking + routing decisions live in a dedicated processing task.
//!
//! This is an evolution of Handy's audio toolkit + recorder, using more Tokio
//! and our chosen crates (cpal + rubato + future wavekat-vad).

pub mod capture;
pub mod processor;
pub mod vad;

pub use capture::{AudioCapture, CaptureConfig};
pub use processor::AudioBuffer;
pub use vad::{SimpleEnergyVad, VadPolicy, VoiceActivityDetector};
