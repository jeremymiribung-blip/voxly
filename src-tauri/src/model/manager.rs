//! ModelManager implementation.
//!
//! This module is responsible for:
//! - Knowing the canonical Voxtral model(s) we support
//! - Downloading them on-demand from Hugging Face (with resume-friendly behavior via hf-hub)
//! - Maintaining a local cache under the app's data directory
//! - Emitting structured progress and state events to the frontend
//!
//! Design notes (ADR 0002):
//! - We start with a single hard-coded primary model for simplicity and correctness.
//! - The manager itself does **not** own the loaded inference engine (see `EngineManager`).
//! - Capability probing (GGUF header inspection) will be added once we have a concrete
//!   weight file format and can read metadata without loading the full model.
//! - Downloads are cancellable in the future (using a CancellationToken).

use crate::error::{Result, VoxlyError};
use crate::events;
use hf_hub::api::tokio::Api;
use hf_hub::{Repo, RepoType};
// The Progress trait may live under a slightly different path depending on hf-hub version.
// We implement the required methods directly.
trait HfProgress {
    fn init(&mut self, size: usize, filename: &str);
    fn update(&mut self, size: usize);
    fn finish(&mut self);
}
use std::path::{Path, PathBuf};
use std::sync::{Arc, Mutex};
use std::time::{Duration, Instant};
use tauri::{AppHandle, Emitter, Manager};

/// Event emitted when model download progress updates.
#[derive(Clone, Debug, serde::Serialize)]
pub struct DownloadProgress {
    pub model_id: String,
    pub downloaded_bytes: u64,
    pub total_bytes: u64,
    pub percentage: f64,
}

/// Events describing the lifecycle of a model (loading, ready, error, etc.).
/// These are emitted on the Tauri event bus for the frontend to react to.
#[derive(Clone, Debug, serde::Serialize)]
pub struct ModelStateEvent {
    pub event_type: String, // "download_started", "download_progress", "download_completed", "loading_started", "ready", "error", ...
    pub model_id: Option<String>,
    pub message: Option<String>,
}

/// The single source of truth for model files on disk.
#[derive(Clone)]
pub struct ModelManager {
    app_handle: AppHandle,
    cache_dir: PathBuf,
    /// Currently only one model is primary. In the future this can become a registry.
    primary_model: PrimaryModel,
}

#[derive(Clone, Debug)]
struct PrimaryModel {
    /// Hugging Face repo id, e.g. "mistralai/Voxtral-Mini-4B-Realtime-2602"
    repo_id: String,
    /// Revision / commit / tag
    revision: String,
    /// Filename inside the repo (the actual weights file or index).
    /// For the realtime crate this may be a .safetensors, GGUF, or a directory of shards.
    filename: String,
    /// Human friendly name
    display_name: String,
}

impl Default for PrimaryModel {
    fn default() -> Self {
        Self {
            repo_id: "mistralai/Voxtral-Mini-4B-Realtime-2602".to_string(),
            revision: "main".to_string(),
            // TODO: confirm exact artifact name once the Burn port publishes recommended files.
            // The TrevorS crate may expect a specific layout or GGUF Q4 file.
            filename: "model.safetensors".to_string(), // placeholder — will be adjusted during integration
            display_name: "Voxtral Mini 4B Realtime (Q4)".to_string(),
        }
    }
}

impl ModelManager {
    /// Create a new ModelManager. The cache directory is placed inside
    /// the platform app data dir under `models/`.
    pub fn new(app_handle: &AppHandle) -> Result<Self> {
        let app_data = app_handle
            .path()
            .app_data_dir()
            .map_err(|e| VoxlyError::Config(format!("failed to resolve app data dir: {e}")))?;

        let cache_dir = app_data.join("models");
        std::fs::create_dir_all(&cache_dir)?;

        Ok(Self {
            app_handle: app_handle.clone(),
            cache_dir,
            primary_model: PrimaryModel::default(),
        })
    }

    /// Returns the absolute path where the primary model should reside (or is cached).
    pub fn primary_model_path(&self) -> PathBuf {
        self.cache_dir.join(&self.primary_model.filename)
    }

    /// Returns true if the primary model appears to be present on disk.
    /// This is a cheap existence check; full integrity verification happens on load.
    pub fn is_primary_model_downloaded(&self) -> bool {
        let path = self.primary_model_path();
        path.exists() && path.is_file() && path.metadata().map(|m| m.len() > 1024).unwrap_or(false)
    }

    /// The human-friendly name of the primary model.
    pub fn primary_model_name(&self) -> &str {
        &self.primary_model.display_name
    }

    /// Download (or verify) the primary model.
    ///
    /// Emits `model-download-progress` and `model-state-changed` events.
    /// The implementation uses hf-hub's async API so it can be driven from a
    /// Tauri command (async) or background task.
    pub async fn ensure_primary_model(&self) -> Result<PathBuf> {
        let model = &self.primary_model;
        let target_path = self.primary_model_path();

        if self.is_primary_model_downloaded() {
            tracing::info!("Primary model already present at {:?}", target_path);
            self.emit_state("ready", Some(model.display_name.clone()), None);
            return Ok(target_path);
        }

        self.emit_state(
            "download_started",
            Some(model.display_name.clone()),
            Some(format!(
                "Downloading {} from Hugging Face...",
                model.display_name
            )),
        );

        let api = Api::new().map_err(|e| VoxlyError::ModelDownload(e.to_string()))?;

        let repo = api.repo(Repo::with_revision(
            model.repo_id.clone(),
            RepoType::Model,
            model.revision.clone(),
        ));

        // For the architecture spike we perform the download without a custom
        // progress adapter (hf-hub versions differ). Real progress + cancellation
        // will be wired properly once the exact hf-hub API surface is pinned.
        let downloaded = repo
            .get(&model.filename)
            .await
            .map_err(|e| VoxlyError::ModelDownload(format!("hf-hub get failed: {e}")))?;

        // For the initial version we simply copy into our cache dir.
        // In production we would either:
        //   a) point the engine directly at the hf-hub cache location, or
        //   b) use a hard link / move.
        if let Some(parent) = target_path.parent() {
            std::fs::create_dir_all(parent)?;
        }
        tokio::fs::copy(&downloaded, &target_path)
            .await
            .map_err(VoxlyError::Io)?;

        tracing::info!("Model downloaded to {:?}", target_path);

        self.emit_state("download_completed", Some(model.display_name.clone()), None);
        self.emit_state("ready", Some(model.display_name.clone()), None);

        Ok(target_path)
    }

    fn emit_state(&self, event_type: &str, name: Option<String>, message: Option<String>) {
        let _ = self.app_handle.emit(
            "model-state-changed",
            ModelStateEvent {
                event_type: event_type.to_string(),
                model_id: Some(self.primary_model.repo_id.clone()),
                message: message.or(name),
            },
        );
    }
}

// Real HfProgress adapter will be restored in a follow-up when we lock the
// exact hf-hub version and Progress trait location. For now we emit coarse
// "started" / "completed" state events only.
