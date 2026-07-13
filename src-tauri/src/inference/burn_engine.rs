//! Burn + Voxtral Mini Realtime concrete engine implementation.
//!
//! This file will contain the real integration with
//! `voxtral-mini-realtime-rs` (https://github.com/TrevorS/voxtral-mini-realtime-rs)
//! once the crate's public API stabilizes and we add it as a dependency.
//!
//! Current state: **placeholder / skeleton**.
//! All methods are documented with the intended behavior and the shape of the
//! integration we expect. The code compiles and can be used for architecture
//! testing and coordinator development.
//!
//! When the real crate lands we will:
//! - Add the git dependency under a feature flag `burn-voxtral`
//! - Replace the inner `Option<...>` with the actual model/session types
//! - Implement proper `feed_audio` that calls into the streaming forward pass
//! - Wire device selection (CPU / Metal / CUDA / WGPU via Burn)
//!
//! The Voxtral Realtime model is natively streaming with a causal audio encoder
//! and supports configurable delay (240 ms – 2.4 s). The Rust port is expected
//! to expose a low-level API that accepts PCM chunks and yields incremental text.

use super::engine::{TranscriptionEngine, TranscriptionUpdate};
use crate::error::{Result, VoxlyError};
use async_trait::async_trait;
use std::path::Path;
use tracing::{debug, info, warn};

/// Concrete implementation of [`TranscriptionEngine`] backed by Burn +
/// the official Voxtral Mini Realtime weights (via TrevorS's pure-Rust port).
pub struct BurnVoxtralEngine {
    /// Path to the currently loaded weights (for diagnostics / reload).
    loaded_path: Option<std::path::PathBuf>,

    /// In the real implementation this will hold something like:
    /// `Option<voxtral_mini_realtime::VoxtralSession>` or the Burn `Module` + tokenizer state.
    ///
    /// For now we keep a dummy state so the rest of the system can exercise
    /// the coordinator and audio paths.
    _inner: Option<()>, // placeholder

    /// Simple accumulating buffer used by the placeholder to simulate partial results.
    placeholder_buffer: String,

    /// How many samples we have seen in the current "utterance" (for fake progress).
    samples_seen: usize,
}

impl BurnVoxtralEngine {
    /// Create an unloaded engine instance.
    pub fn new() -> Self {
        Self {
            loaded_path: None,
            _inner: None,
            placeholder_buffer: String::new(),
            samples_seen: 0,
        }
    }

    /// Internal helper used only by the placeholder implementation.
    fn simulate_realtime_update(&mut self, new_samples: usize) -> Option<TranscriptionUpdate> {
        self.samples_seen += new_samples;

        // Every ~480 ms worth of audio (at 16 kHz) we "emit" a fake word.
        // This lets the coordinator, event emission, and UI be exercised immediately.
        if self.samples_seen > 0 && self.samples_seen.is_multiple_of(7680) {
            let word = match (self.samples_seen / 7680) % 5 {
                0 => "hello",
                1 => "from",
                2 => "voxly",
                3 => "realtime",
                _ => "placeholder",
            };
            self.placeholder_buffer.push(' ');
            self.placeholder_buffer.push_str(word);

            return Some(TranscriptionUpdate {
                committed: self.placeholder_buffer.trim().to_string(),
                tentative: Some(format!("{}...", word)),
                timestamps: None,
            });
        }
        None
    }
}

impl Default for BurnVoxtralEngine {
    fn default() -> Self {
        Self::new()
    }
}

#[async_trait]
impl TranscriptionEngine for BurnVoxtralEngine {
    async fn load(&mut self, model_path: &Path) -> Result<()> {
        if self.is_loaded() {
            info!("BurnVoxtralEngine already loaded; ignoring duplicate load");
            return Ok(());
        }

        // === REAL INTEGRATION TODO ===
        // 1. Use Burn's device selection (CPU, Metal, Cuda, Wgpu, etc.)
        // 2. Load the weights:
        //    let model = voxtral_mini_realtime_rs::load_model(model_path, device)?;
        //    let tokenizer = ...;
        //    self.inner = Some(VoxtralSession::new(model, tokenizer));
        //
        // The crate is expected to support both full-context and the native
        // streaming (causal) mode. We will prefer the streaming path for
        // low latency.

        if !model_path.exists() {
            return Err(VoxlyError::ModelNotFound {
                model_id: model_path.display().to_string(),
            });
        }

        // For the placeholder we just remember the path and pretend success.
        self.loaded_path = Some(model_path.to_path_buf());
        self.placeholder_buffer.clear();
        self.samples_seen = 0;

        info!(
            "BurnVoxtralEngine (placeholder) 'loaded' model from {:?}",
            model_path
        );
        debug!("In real build this will allocate Burn tensors / load GGUF weights");

        Ok(())
    }

    fn unload(&mut self) {
        if self.loaded_path.is_some() {
            info!("Unloading BurnVoxtralEngine (placeholder)");
            self.loaded_path = None;
            self._inner = None;
            self.placeholder_buffer.clear();
            self.samples_seen = 0;
        }
    }

    fn is_loaded(&self) -> bool {
        self.loaded_path.is_some()
    }

    fn feed_audio(&mut self, samples: &[f32]) -> Option<TranscriptionUpdate> {
        if !self.is_loaded() {
            return None;
        }

        // Hot path — keep this cheap.
        // Real implementation will run the causal encoder step here and
        // return partial text when the model produces a new token or word boundary.
        self.simulate_realtime_update(samples.len())
    }

    async fn finalize(&mut self) -> Result<String> {
        if !self.is_loaded() {
            return Ok(String::new());
        }

        let final_text = self.placeholder_buffer.trim().to_string();
        debug!("finalize() returning: {:?}", final_text);

        // In real code: flush any remaining decoder state, run final forward pass,
        // apply any post-processing (punctuation, casing), then reset buffers.
        self.placeholder_buffer.clear();
        self.samples_seen = 0;

        Ok(final_text)
    }

    fn reset(&mut self) {
        self.placeholder_buffer.clear();
        self.samples_seen = 0;
        // Real impl: clear KV cache / hidden state of the causal model
        debug!("BurnVoxtralEngine reset");
    }

    fn supports_streaming(&self) -> bool {
        // Voxtral Mini Realtime is designed for this.
        true
    }

    fn backend_name(&self) -> &'static str {
        "burn-voxtral-realtime"
    }
}
