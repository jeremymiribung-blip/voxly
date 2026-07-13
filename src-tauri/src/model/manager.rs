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
use futures_util::StreamExt;
use reqwest::header::{HeaderMap, RANGE};
use std::path::{Path, PathBuf};
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::{Arc, Mutex};
use std::time::{Duration, Instant};
use tauri::{AppHandle, Emitter, Manager};
use tokio::io::AsyncWriteExt;
use tracing::{debug, info, instrument, warn};

/// Event emitted when model download progress updates.
/// Includes speed and ETA for nice UI.
#[derive(Clone, Debug, serde::Serialize)]
pub struct DownloadProgress {
    pub model_id: String,
    pub downloaded_bytes: u64,
    pub total_bytes: u64,
    pub percentage: f64,
    /// Download speed in bytes per second
    #[serde(default)]
    pub speed_bps: f64,
    /// Estimated time remaining in seconds
    #[serde(default)]
    pub eta_seconds: u64,
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
    /// Control flags for active download (pause/resume/cancel)
    download_paused: Arc<AtomicBool>,
    download_cancelled: Arc<AtomicBool>,
}

#[derive(Clone, Debug)]
struct PrimaryModel {
    /// Hugging Face repo id for the quantized weights (Q4 GGUF recommended)
    repo_id: String,
    /// Revision
    revision: String,
    /// Exact filename of the GGUF
    filename: String,
    /// Human friendly name
    display_name: String,
    /// Quant level for UI
    quant: String,
}

impl Default for PrimaryModel {
    fn default() -> Self {
        Self {
            repo_id: "TrevorJS/voxtral-mini-realtime-gguf".to_string(),
            revision: "main".to_string(),
            filename: "voxtral-q4.gguf".to_string(),
            display_name: "Voxtral Mini 4B Realtime (Q4 GGUF)".to_string(),
            quant: "Q4".to_string(),
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
            download_paused: Arc::new(AtomicBool::new(false)),
            download_cancelled: Arc::new(AtomicBool::new(false)),
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

    /// Download (or resume) the primary model with full control.
    ///
    /// Uses direct reqwest + HTTP Range for robust resumable downloads from HF.
    /// Supports pause/resume/cancel via flags.
    /// Emits detailed `model-download-progress` (with speed/ETA) and state events.
    #[instrument(skip(self))]
    pub async fn ensure_primary_model(&self) -> Result<PathBuf> {
        let model = &self.primary_model;
        let target_path = self.primary_model_path();

        if self.is_primary_model_downloaded() {
            tracing::info!("Primary model already present at {:?}", target_path);
            self.emit_state("ready", Some(model.display_name.clone()), None);
            return Ok(target_path);
        }

        // Reset control flags
        self.download_paused.store(false, Ordering::Relaxed);
        self.download_cancelled.store(false, Ordering::Relaxed);

        self.emit_state(
            "download_started",
            Some(model.display_name.clone()),
            Some(format!(
                "Downloading {} ({} quant) from Hugging Face...",
                model.display_name, model.quant
            )),
        );

        let url = format!(
            "https://huggingface.co/{}/resolve/{}/{}",
            model.repo_id, model.revision, model.filename
        );

        // Perform resumable download
        self.download_file_resumable(&url, &target_path, model)
            .await?;

        tracing::info!("Model downloaded to {:?}", target_path);

        self.emit_state("download_completed", Some(model.display_name.clone()), None);
        self.emit_state("ready", Some(model.display_name.clone()), None);

        Ok(target_path)
    }

    /// Core resumable downloader using reqwest Range requests.
    /// Emits progress with speed (B/s) and ETA.
    async fn download_file_resumable(
        &self,
        url: &str,
        dest: &Path,
        model: &PrimaryModel,
    ) -> Result<()> {
        let client = reqwest::Client::new();

        // Get total size (HEAD request)
        let head_resp = client
            .head(url)
            .send()
            .await
            .map_err(|e| VoxlyError::ModelDownload(e.to_string()))?;
        let total_size = head_resp.content_length().unwrap_or(0);

        // Check existing partial file
        let mut downloaded: u64 = 0;
        if dest.exists() {
            downloaded = tokio::fs::metadata(dest).await?.len();
        }

        if downloaded >= total_size && total_size > 0 {
            return Ok(());
        }

        let mut headers = HeaderMap::new();
        if downloaded > 0 {
            headers.insert(RANGE, format!("bytes={}-", downloaded).parse().unwrap());
        }

        let resp = client
            .get(url)
            .headers(headers)
            .send()
            .await
            .map_err(|e| VoxlyError::ModelDownload(e.to_string()))?;

        let status = resp.status();
        if !(status.is_success() || status == reqwest::StatusCode::PARTIAL_CONTENT) {
            return Err(VoxlyError::ModelDownload(format!("Bad status: {}", status)));
        }

        // Ensure parent dir
        if let Some(parent) = dest.parent() {
            tokio::fs::create_dir_all(parent).await?;
        }

        let mut file = tokio::fs::OpenOptions::new()
            .create(true)
            .append(true)
            .open(dest)
            .await
            .map_err(VoxlyError::Io)?;

        let mut stream = resp.bytes_stream();
        let start_time = Instant::now();
        let mut last_emit = Instant::now();

        while let Some(chunk) = stream.next().await {
            // Check controls
            if self.download_cancelled.load(Ordering::Relaxed) {
                return Err(VoxlyError::ModelDownload("Download cancelled".into()));
            }
            while self.download_paused.load(Ordering::Relaxed) {
                tokio::time::sleep(Duration::from_millis(200)).await;
                if self.download_cancelled.load(Ordering::Relaxed) {
                    return Err(VoxlyError::ModelDownload(
                        "Download cancelled while paused".into(),
                    ));
                }
            }

            let chunk = chunk.map_err(|e| VoxlyError::ModelDownload(e.to_string()))?;
            file.write_all(&chunk).await.map_err(VoxlyError::Io)?;
            downloaded += chunk.len() as u64;

            // Throttled progress with speed + ETA
            let now = Instant::now();
            if now.duration_since(last_emit) > Duration::from_millis(150)
                || downloaded >= total_size
            {
                let elapsed = start_time.elapsed().as_secs_f64().max(0.1);
                let speed = downloaded as f64 / elapsed; // bytes/sec
                let remaining = total_size.saturating_sub(downloaded) as f64;
                let eta = if speed > 0.0 {
                    (remaining / speed) as u64
                } else {
                    0
                };

                let percentage = if total_size > 0 {
                    (downloaded as f64 / total_size as f64) * 100.0
                } else {
                    0.0
                };

                let progress = DownloadProgress {
                    model_id: model.display_name.clone(),
                    downloaded_bytes: downloaded,
                    total_bytes: total_size,
                    percentage,
                    speed_bps: speed,
                    eta_seconds: eta,
                };

                let _ = self.app_handle.emit("model-download-progress", &progress);
                last_emit = now;
            }
        }

        file.flush().await.map_err(VoxlyError::Io)?;
        Ok(())
    }

    /// Pause current download (if active)
    pub fn pause_download(&self) {
        self.download_paused.store(true, Ordering::Relaxed);
        tracing::info!("Download paused");
    }

    /// Resume current download
    pub fn resume_download(&self) {
        self.download_paused.store(false, Ordering::Relaxed);
        tracing::info!("Download resumed");
    }

    /// Cancel current download
    pub fn cancel_download(&self) {
        self.download_cancelled.store(true, Ordering::Relaxed);
        self.download_paused.store(false, Ordering::Relaxed);
        tracing::info!("Download cancelled");
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
