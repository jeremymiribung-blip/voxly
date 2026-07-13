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

/// Real-time transcription update with tentative vs committed text.
/// Frontend should display committed + tentative (e.g. tentative in italics or different color).
#[derive(Clone, Debug, Serialize)]
pub struct TranscriptionUpdateEvent {
    pub committed: String,
    pub tentative: Option<String>,
    pub timestamp: u64,
    /// Optional metrics for UI
    pub latency_ms: Option<u64>,
}

pub fn emit_transcription_update(
    app: &AppHandle,
    committed: &str,
    tentative: Option<&str>,
    latency_ms: Option<u64>,
) {
    let _ = app.emit(
        "transcription-update",
        TranscriptionUpdateEvent {
            committed: committed.to_string(),
            tentative: tentative.map(|s| s.to_string()),
            timestamp: now_ms(),
            latency_ms,
        },
    );
}

/// Session status for UI state machine.
#[derive(Clone, Debug, Serialize)]
pub struct SessionStatusEvent {
    pub state: String, // "idle", "listening", "processing", "error"
    pub timestamp: u64,
    pub metrics: Option<SessionMetrics>,
}

#[derive(Clone, Debug, Serialize)]
pub struct SessionMetrics {
    pub rtf: f32, // real time factor
    pub tokens_per_sec: f32,
    pub audio_duration_ms: u64,
}

pub fn emit_session_status(app: &AppHandle, state: &str, metrics: Option<SessionMetrics>) {
    let _ = app.emit(
        "session-status",
        SessionStatusEvent {
            state: state.to_string(),
            timestamp: now_ms(),
            metrics,
        },
    );
}

fn now_ms() -> u64 {
    std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .unwrap()
        .as_millis() as u64
}
