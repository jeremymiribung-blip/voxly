//! Robust low-latency audio pipeline.
//!
//! - Capture: cpal + lock-free ringbuf (non-blocking callback)
//! - Resample: rubato to 16 kHz mono
//! - VAD: wavekat-vad (Silero) + Smoothed policy with dynamic hangover + onset protection (2 frames)
//! - Chunking: overlapping 80 ms chunks aligned to Voxtral realtime needs
//! - Delivery: via Tokio mpsc to the coordinator
//!
//! Heavily inspired by Handy audio_toolkit (VAD policy, SmoothedVad, resampler reset hygiene,
//! cpal callback patterns) but rebuilt for Tokio channels, wavekat-vad, and ringbuf lock-freedom.

pub mod capture;
pub mod processor;
pub mod vad;

pub use capture::{default_input_device_name, list_input_devices, AudioCapture, CaptureConfig};
pub use processor::{AudioChunk, AudioProcessor, VOXTRAL_CHUNK_MS, VOXTRAL_CHUNK_SAMPLES};
pub use vad::{VadFrame, VadPolicy, VoiceActivityDetector, WavekatSmoothedVad};
