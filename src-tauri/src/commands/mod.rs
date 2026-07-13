//! Tauri commands exposed to the frontend.
//!
//! These are thin adapters that delegate to the managers and coordinator.
//! They must be registered in `lib.rs` via `tauri::generate_handler!`.

pub mod audio;
pub mod model;
pub mod transcription;

pub use audio::*;
pub use model::*;
pub use transcription::*;
