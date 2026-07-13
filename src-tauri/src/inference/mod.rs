//! Inference engine abstraction layer.
//!
//! This is the core architectural piece that allows Voxly to support
//! multiple backends (starting with Burn + Voxtral) while keeping the rest
//! of the system (coordinator, audio, UI) decoupled from any specific model.
//!
//! Inspired by Handy's `LoadedEngine` enum + dispatch in `TranscriptionManager`,
//! but improved for our stack:
//! - Trait object (`dyn TranscriptionEngine`) for easy extension
//! - Tokio-friendly async API where appropriate (model load can be heavy)
//! - Clear ownership model for streaming (engine can be "leased" to a worker)
//! - Feature flags prepared for sidecar / plugin engines (CrispASR etc.)
//!
//! See ADR 0002 for rationale and detailed design.

pub mod burn_engine;
pub mod engine;
pub mod manager;

pub use engine::{TranscriptionEngine, TranscriptionUpdate};
pub use manager::{EngineManager, StreamCommand};

// Re-export the concrete implementation for wiring.
pub use burn_engine::BurnVoxtralEngine;
