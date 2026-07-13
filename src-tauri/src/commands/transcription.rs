//! Transcription-related Tauri commands.

use crate::coordinator::TranscriptionCoordinator;
use std::sync::Arc;
use tauri::State;

#[tauri::command]
pub fn start_recording(
    coordinator: State<'_, Arc<TranscriptionCoordinator>>,
    push_to_talk: bool,
) -> Result<(), String> {
    coordinator.start_recording(push_to_talk);
    Ok(())
}

#[tauri::command]
pub fn stop_recording(coordinator: State<'_, Arc<TranscriptionCoordinator>>) -> Result<(), String> {
    coordinator.stop_recording();
    Ok(())
}

#[tauri::command]
pub fn cancel_recording(
    coordinator: State<'_, Arc<TranscriptionCoordinator>>,
) -> Result<(), String> {
    coordinator.cancel();
    Ok(())
}
