//! Application configuration and settings.
//!
//! In the long term this will be persisted (tauri-plugin-store or custom JSON
//! in the app data directory) and exposed to the frontend via commands/events.
//!
//! For the initial architecture we keep an in-memory `AppConfig` with
//! sensible defaults. The `EngineManager` and `Coordinator` read from here
//! (or receive updates via channels).

use serde::{Deserialize, Serialize};
use std::path::PathBuf;

/// Top level application configuration.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct AppConfig {
    /// Identifier of the currently selected model (e.g. "voxtral-mini-4b-realtime-q4").
    pub selected_model_id: String,

    /// Preferred input audio device name (or None for system default).
    pub input_device: Option<String>,

    /// Whether to use voice activity detection.
    pub vad_enabled: bool,

    /// VAD policy / aggressiveness. "streaming" uses a longer hangover tail.
    pub vad_policy: VadPolicy,

    /// Target sample rate for the model (Voxtral Realtime typically expects 16kHz or 24kHz).
    pub target_sample_rate: u32,

    /// Directory where downloaded models are cached (resolved at runtime from app data dir).
    /// This field is populated at startup.
    #[serde(skip)]
    pub model_cache_dir: Option<PathBuf>,
}

#[derive(Debug, Clone, Copy, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "lowercase")]
pub enum VadPolicy {
    Disabled,
    /// Tuned for offline / push-to-talk (shorter tail).
    Offline,
    /// Longer post-speech tail suitable for live streaming preview.
    Streaming,
}

impl Default for AppConfig {
    fn default() -> Self {
        Self {
            selected_model_id: "mistralai/Voxtral-Mini-4B-Realtime-2602".to_string(),
            input_device: None,
            vad_enabled: true,
            vad_policy: VadPolicy::Streaming,
            target_sample_rate: 16000,
            model_cache_dir: None,
        }
    }
}

impl AppConfig {
    /// Returns the model ID that should be used for inference.
    pub fn model_id(&self) -> &str {
        &self.selected_model_id
    }
}
