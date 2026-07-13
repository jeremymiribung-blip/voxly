//! The `TranscriptionEngine` trait — the central abstraction.
//!
//! Any backend that can perform realtime or batched speech-to-text must
//! implement this trait. The rest of Voxly (coordinator, audio pipeline)
//! only talks to the trait.
//!
//! Design goals:
//! - Simple, focused API suitable for streaming 16 kHz (or 24 kHz) PCM
//! - Async where loading or finalization may block for a long time
//! - Synchronous `feed_audio` for the hot audio path (low latency, called from
//!   audio callback or dedicated thread)
//! - Explicit lifecycle (load / unload / reset) so we can manage memory
//! - Capability queries so the UI and coordinator can make decisions

use crate::error::Result;
use async_trait::async_trait;
use std::path::Path;

/// A partial or incremental transcription update produced while streaming.
///
/// `committed` is stable text that will not change anymore.
/// `tentative` (if present) is the current hypothesis that the model may still revise.
#[derive(Clone, Debug, Default, serde::Serialize)]
pub struct TranscriptionUpdate {
    pub committed: String,
    pub tentative: Option<String>,
    /// Optional per-token or word timestamps (model dependent).
    pub timestamps: Option<Vec<(f32, f32, String)>>,
}

/// The engine abstraction trait.
///
/// Implementors are expected to be `Send + Sync` so they can live behind
/// `Arc<Mutex<...>>` or be moved into dedicated worker tasks/threads.
///
/// We use `#[async_trait]` so that `async fn` methods are object-safe
/// (`Box<dyn TranscriptionEngine>` works).
#[async_trait]
pub trait TranscriptionEngine: Send + Sync {
    /// Load the model weights from the given path.
    ///
    /// This is expected to be a potentially expensive operation (hundreds of MB
    /// to several GB). Callers should usually run it on a blocking task or
    /// background thread.
    ///
    /// After successful load the engine is ready to accept `feed_audio`.
    async fn load(&mut self, model_path: &Path) -> Result<()>;

    /// Unload the model and release associated resources (GPU memory, etc.).
    /// Safe to call when not loaded.
    fn unload(&mut self);

    /// Returns true if a model is currently resident in memory.
    fn is_loaded(&self) -> bool;

    /// Feed a chunk of mono f32 PCM samples.
    ///
    /// The sample rate must match what the loaded model expects (usually 16 kHz).
    /// The implementation should be as fast as possible — this is on the
    /// critical audio path.
    ///
    /// Returns `Some(update)` when the engine has new committed or tentative text
    /// to report. The coordinator / UI layer decides how to surface it.
    ///
    /// For models that produce output only on `finalize`, this may return `None`
    /// until the end of the utterance.
    fn feed_audio(&mut self, samples: &[f32]) -> Option<TranscriptionUpdate>;

    /// Signal end of current utterance / recording.
    ///
    /// The engine should flush any internal buffers / state and return the final
    /// committed transcription for the segment.
    async fn finalize(&mut self) -> Result<String>;

    /// Reset all internal state (hidden states, buffers, etc.) without unloading
    /// the model. Useful between separate recordings when the same model stays loaded.
    fn reset(&mut self);

    /// Optional: does this backend support true low-latency streaming (partial
    /// results while audio is still arriving)?
    fn supports_streaming(&self) -> bool {
        true
    }

    /// Human readable name of the backend implementation (for diagnostics).
    fn backend_name(&self) -> &'static str;
}
