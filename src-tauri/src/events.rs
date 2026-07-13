//! Helpers for emitting Tauri events in a consistent, typed way.
//!
//! All events that the frontend listens to should go through here so we
//! have a single place to evolve the event schema.

use serde::Serialize;
use tauri::{AppHandle, Emitter};

/// Emitted when recording (capture) has begun.
#[derive(Clone, Debug, Serialize)]
pub struct RecordingStartedEvent {
    pub timestamp: u64,
}

/// Emitted when recording stops (before or after finalize).
#[derive(Clone, Debug, Serialize)]
pub struct RecordingStoppedEvent {
    pub timestamp: u64,
}

/// Final committed transcription.
#[derive(Clone, Debug, Serialize)]
pub struct TranscriptionFinalEvent {
    pub text: String,
    pub timestamp: u64,
}

/// Generic error broadcast.
#[derive(Clone, Debug, Serialize)]
pub struct ErrorEvent {
    pub message: String,
}

pub fn emit_recording_started(app: &AppHandle) {
    let _ = app.emit(
        "recording-started",
        RecordingStartedEvent {
            timestamp: now_ms(),
        },
    );
}

pub fn emit_recording_stopped(app: &AppHandle) {
    let _ = app.emit(
        "recording-stopped",
        RecordingStoppedEvent {
            timestamp: now_ms(),
        },
    );
}

pub fn emit_transcription_final(app: &AppHandle, text: &str) {
    let _ = app.emit(
        "transcription-final",
        TranscriptionFinalEvent {
            text: text.to_string(),
            timestamp: now_ms(),
        },
    );
}

pub fn emit_error(app: &AppHandle, message: &str) {
    let _ = app.emit(
        "voxly-error",
        ErrorEvent {
            message: message.to_string(),
        },
    );
}

fn now_ms() -> u64 {
    std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .unwrap()
        .as_millis() as u64
}
