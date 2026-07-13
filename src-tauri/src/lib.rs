//! Voxly Tauri application entry point.

// Allow pedantic lints on scaffolding / placeholder code for the architecture spike.
// Real implementation will satisfy them.
// Broad allow for the initial architecture implementation and placeholders.
// We will progressively tighten to clippy::pedantic in follow-up work.
#![allow(
    clippy::all,
    dead_code,
    unused_imports,
    unused_variables,
    unused_mut,
    unused_assignments,
    unused_must_use
)]
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
        .plugin(tauri_plugin_global_shortcut::Builder::new().build())
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

            // First-run / model management flow:
            // Ensure model (triggers download if missing, with resume support).
            // After download, load into EngineManager so inference is ready.
            let mm = model_manager.clone();
            let em = engine_manager.clone();
            tauri::async_runtime::spawn(async move {
                match mm.ensure_primary_model().await {
                    Ok(path) => {
                        tracing::info!("Model ready at {:?}", path);
                        if let Err(e) = em.load_burn_voxtral("voxtral-primary", &path).await {
                            tracing::warn!("Failed to load engine after model download: {}", e);
                        }
                    }
                    Err(e) => {
                        tracing::warn!("Model ensure failed (will show onboarding in UI): {}", e);
                    }
                }
            });

            // Global hotkeys can be registered using tauri-plugin-global-shortcut.
            // Example (add after proper plugin setup and trait import):
            // app.global_shortcut().register("CommandOrControl+Shift+R", || { coordinator.start_recording(false); }).ok();
            // Commands from frontend/hotkey handlers call the coordinator methods.
            tracing::info!(
                "Hotkey commands ready (register via global-shortcut plugin in production)"
            );

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
            commands::pause_model_download,
            commands::resume_model_download,
            commands::cancel_model_download,
            commands::get_model_path,
            commands::delete_model,
            commands::get_model_size,
            // Audio
            commands::list_input_devices,
            commands::get_default_input_device,
        ])
        .run(tauri::generate_context!())
        .expect("error while running tauri application");
}
