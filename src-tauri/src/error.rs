//! Centralized error handling for Voxly.
//!
//! Uses `thiserror` for rich domain-specific errors and `anyhow` for
//! flexible error propagation in application glue code (commands, coordinators).
//!
//! All public Tauri commands should return `Result<T, String>` (or use a
//! custom serializable error) so that the frontend receives human-readable
//! messages. Internally we use the rich `VoxlyError` type.

use thiserror::Error;

/// The primary error type for Voxly operations.
///
/// Each variant carries context appropriate for logging (via `tracing`) and
/// for surfacing to the user.
#[derive(Debug, Error)]
pub enum VoxlyError {
    /// Audio device or capture failure (no devices, permission denied, format mismatch, etc.)
    #[error("audio error: {0}")]
    Audio(String),

    /// VAD (voice activity detection) initialization or processing error.
    #[error("VAD error: {0}")]
    Vad(String),

    /// Problems during model download (network, checksum, cancellation, disk).
    #[error("model download error: {0}")]
    ModelDownload(String),

    /// Model not found in cache or on disk when load was requested.
    #[error("model not found: {model_id}")]
    ModelNotFound { model_id: String },

    /// Failure to load or initialize the inference engine / model weights.
    #[error("inference engine error: {0}")]
    Inference(String),

    /// The engine does not support a requested capability (e.g. streaming on a batch-only backend).
    #[error("engine does not support {capability}")]
    UnsupportedCapability { capability: String },

    /// Coordinator / state machine invariant violation or invalid command in current state.
    #[error("coordinator error: {0}")]
    Coordinator(String),

    /// Configuration or settings problem.
    #[error("config error: {0}")]
    Config(String),

    /// Generic I/O or serialization problem.
    #[error(transparent)]
    Io(#[from] std::io::Error),

    /// Any other error (use sparingly; prefer a specific variant).
    #[error(transparent)]
    Other(#[from] anyhow::Error),
}

impl VoxlyError {
    /// Convenience constructor for audio errors.
    pub fn audio(msg: impl Into<String>) -> Self {
        Self::Audio(msg.into())
    }

    /// Convenience for inference errors.
    pub fn inference(msg: impl Into<String>) -> Self {
        Self::Inference(msg.into())
    }
}

/// Result alias used throughout the Rust backend.
pub type Result<T> = std::result::Result<T, VoxlyError>;

/// Convert our error into a string for Tauri command boundaries.
/// The frontend receives the Display representation.
impl From<VoxlyError> for String {
    fn from(err: VoxlyError) -> Self {
        err.to_string()
    }
}
