//! Central orchestrator for the transcription lifecycle.
//!
//! This is Voxly's equivalent of Handy's `TranscriptionCoordinator`.
//!
//! Responsibilities:
//! - Serialize all high-level commands (start/stop recording, hotkey events, etc.)
//! - Maintain a simple state machine (Idle / Recording / Finalizing)
//! - Coordinate the `AudioCapture` and `EngineManager`
//! - Drive the live streaming path via `StreamRouter` when appropriate
//! - Emit events to the frontend
//! - Handle graceful shutdown and panic recovery (catch_unwind)
//!
//! We use a dedicated thread (like Handy) for the coordinator loop so that
//! the state machine cannot be interleaved by async tasks. Audio frames
//! still flow through the lock-free-ish `StreamRouter`.

pub mod coordinator;

pub use coordinator::{CoordinatorCommand, TranscriptionCoordinator};
