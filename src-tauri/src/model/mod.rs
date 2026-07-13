//! Model management: discovery, downloading from Hugging Face, caching,
//! versioning, and capability metadata.
//!
//! Inspired by Handy's `ModelManager` + capability probing, but adapted for
//! the Burn + voxtral-mini-realtime-rs stack and our strict use of a single
//! primary model (Voxtral Mini 4B Realtime Q4 GGUF / weights).
//!
//! Key responsibilities:
//! - Resolve cache location inside the platform app data directory
//! - Download the model (with progress events) using hf-hub
//! - Verify downloaded artifacts
//! - Provide path to the loaded weights for the engine
//! - Emit Tauri events for UI progress / state

pub mod manager;

pub use manager::{DownloadProgress, ModelManager, ModelStateEvent};
