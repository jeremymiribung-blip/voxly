//! Voxly Tauri application entry point.

// Allow pedantic lints on scaffolding / placeholder code for the architecture spike.
// Real implementation will satisfy them.
// Broad allow for the initial architecture implementation and placeholders.
// We will progressively tighten to clippy::pedantic in follow-up work.
#![allow(clippy::all)]
//!
//! This file wires together the major architectural components:
//! - ModelManager (HF downloads + cache)
//! - EngineManager + BurnVoxtralEngine (abstraction + concrete impl)
//! - TranscriptionCoordinator (serialized lifecycle)
//! - AudioCapture (cpal + VAD)
//!
//! All heavy lifting is delegated to the modules under `src/{audio,inference,coordinator,model}`.
//! Tauri commands are thin facades.

use std::sync::Arc;

use tauri::{Manager, State};

mod audio;
mod commands;
mod config;
mod coordinator;
mod error;
mod events;
mod inference;
mod model;

// Re-export for convenience in commands / tests
pub use coordinator::TranscriptionCoordinator;
pub use error::{Result as VoxlyResult, VoxlyError};
pub use inference::EngineManager;
pub use model::ModelManager;

// Keep the original greet during early development (useful smoke test)
#[tauri::command]
fn greet(name: &str) -> String {
    format!("Hello, {}! You've been greeted from Rust!", name)
}

#[cfg_attr(mobile, tauri::mobile_entry_point)]
pub fn run() {
    tauri::Builder::default()
        .plugin(tauri_plugin_opener::init())
        .setup(|app| {
            // Resolve and create managers early.
            let model_manager = Arc::new(
                ModelManager::new(app.handle()).expect("Failed to initialize ModelManager"),
            );

            let engine_manager = Arc::new(EngineManager::new());

            // Coordinator owns the orchestration thread and receives commands from UI/hotkeys.
            let coordinator = Arc::new(TranscriptionCoordinator::new(
                app.handle().clone(),
                engine_manager.clone(),
            ));

            // Make everything available to commands via Tauri's managed state.
            app.manage(model_manager.clone());
            app.manage(engine_manager.clone());
            app.manage(coordinator.clone());

            // Example: eagerly ensure the model is present on startup in the background.
            // In a real app this would be driven by the frontend / onboarding.
            let mm = model_manager.clone();
            tauri::async_runtime::spawn(async move {
                if let Err(e) = mm.ensure_primary_model().await {
                    tracing::warn!("Background model download check failed: {}", e);
                }
            });

            // You can load the engine here once the model exists, or do it lazily
            // from a `load_model` command after the user confirms.
            // For now we leave it unloaded until the coordinator / UI requests it.

            tracing::info!("Voxly core managers initialized");

            Ok(())
        })
        .invoke_handler(tauri::generate_handler![
            greet,
            // Transcription
            commands::start_recording,
            commands::stop_recording,
            commands::cancel_recording,
            // Model
            commands::ensure_model_downloaded,
            commands::is_model_downloaded,
            // Audio
            commands::list_input_devices,
            commands::get_default_input_device,
        ])
        .run(tauri::generate_context!())
        .expect("error while running tauri application");
}
