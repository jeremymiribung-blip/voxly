//! EngineManager — owns and coordinates access to the active `TranscriptionEngine`.
//!
//! This is the direct evolution of Handy's `TranscriptionManager` + `LoadedEngine`
//! pattern, modernized for Voxly:
//!
//! - Uses `tokio::sync::Mutex` for async-friendly access
//! - Retains the atomic lease / worker-id technique for safe "take the engine
//!   out for a streaming worker" even in the presence of panics
//! - Provides a `StreamRouter`-like fast path for audio frames (cheap atomic check)
//! - Exposes high-level async methods used by the `Coordinator`
//!
//! The manager never blocks the audio hot path for long.

use super::{BurnVoxtralEngine, TranscriptionEngine, TranscriptionUpdate};
use crate::error::{Result, VoxlyError};
use std::sync::atomic::{AtomicBool, AtomicU64, Ordering};
use std::sync::Arc;
use tokio::sync::Mutex;
use tracing::{debug, info, warn};

/// Commands sent over the streaming channel (analogous to Handy's `StreamCmd`).
/// Audio frames and control messages share the channel for ordering.
#[derive(Debug)]
pub enum StreamCommand {
    /// A chunk of f32 PCM (already resampled to the model's rate).
    Feed(Vec<f32>),
    /// Request finalization. The reply channel receives the final text.
    Finalize(tokio::sync::oneshot::Sender<Result<String>>),
    /// Abort the current stream without producing final text.
    Cancel,
}

/// Fast-path router for audio frames coming from the capture thread / callback.
///
/// This is the direct analogue of Handy's `StreamRouter`.
/// A relaxed atomic load lets the hot path avoid any lock when nothing is streaming.
#[derive(Clone)]
pub struct StreamRouter {
    /// Protected sender. Only non-None while a stream worker is active.
    tx: Arc<Mutex<Option<tokio::sync::mpsc::Sender<StreamCommand>>>>,
    /// Fast path flag. Checked with `Relaxed` before touching the mutex.
    open: Arc<AtomicBool>,
}

impl StreamRouter {
    pub fn new() -> Self {
        Self {
            tx: Arc::new(Mutex::new(None)),
            open: Arc::new(AtomicBool::new(false)),
        }
    }

    /// Called by the coordinator / manager when a new streaming session begins.
    /// Returns the receiver that the worker task should read from.
    pub async fn open(&self) -> tokio::sync::mpsc::Receiver<StreamCommand> {
        let (tx, rx) = tokio::sync::mpsc::channel(256); // reasonable buffering for audio
        *self.tx.lock().await = Some(tx);
        self.open.store(true, Ordering::Relaxed);
        rx
    }

    /// Close the route. The worker should have already drained or the caller
    /// is about to drop the worker.
    pub async fn close(&self) {
        self.open.store(false, Ordering::Relaxed);
        *self.tx.lock().await = None;
    }

    /// Extremely cheap call from the audio callback / capture thread.
    /// If `!open` we do a single atomic load and return immediately.
    pub fn feed(&self, samples: &[f32]) {
        if !self.open.load(Ordering::Relaxed) {
            return;
        }

        // Only pay the mutex cost when we think a stream is active.
        // In a real hot path we could also use try_send on a lock-free queue.
        if let Ok(guard) = self.tx.try_lock() {
            if let Some(tx) = guard.as_ref() {
                // Best-effort; if the channel is full we drop the frame (rare).
                let _ = tx.try_send(StreamCommand::Feed(samples.to_vec()));
            }
        }
    }

    pub fn is_open(&self) -> bool {
        self.open.load(Ordering::Relaxed)
    }
}

/// RAII guard that ensures stream-related atomic state is cleaned up
/// even if the worker task panics (equivalent to Handy's StreamWorkerGuard).
struct StreamWorkerGuard {
    worker_id: u64,
    active_worker: Arc<AtomicU64>,
    engine_lease: Arc<AtomicU64>,
    stream_active: Arc<AtomicBool>,
}

impl Drop for StreamWorkerGuard {
    fn drop(&mut self) {
        // Only clear if we are still the active worker (prevents old workers
        // from stomping a newer session).
        if self.active_worker.load(Ordering::Acquire) == self.worker_id {
            self.stream_active.store(false, Ordering::Release);
        }
        let _ = self.engine_lease.compare_exchange(
            self.worker_id,
            0,
            Ordering::AcqRel,
            Ordering::Acquire,
        );
        let _ = self.active_worker.compare_exchange(
            self.worker_id,
            0,
            Ordering::AcqRel,
            Ordering::Acquire,
        );
    }
}

/// Owns the currently active transcription engine and coordinates safe access.
///
/// This struct is usually wrapped in `Arc<EngineManager>` and stored in
/// Tauri's managed state.
pub struct EngineManager {
    /// The engine itself. We use a Tokio mutex so async methods can await cleanly.
    /// For the streaming hot path we still use the lease + take pattern.
    engine: Arc<Mutex<Option<Box<dyn TranscriptionEngine + Send + Sync>>>>,

    /// Current model identifier (for UI / diagnostics).
    current_model_id: Arc<Mutex<Option<String>>>,

    /// Atomic flags for streaming worker coordination (see Handy patterns).
    next_worker_id: Arc<AtomicU64>,
    active_worker: Arc<AtomicU64>,
    engine_lease: Arc<AtomicU64>,
    stream_active: Arc<AtomicBool>,

    /// The router that the audio layer feeds directly.
    pub router: Arc<StreamRouter>,
}

impl EngineManager {
    pub fn new() -> Self {
        Self {
            engine: Arc::new(Mutex::new(None)),
            current_model_id: Arc::new(Mutex::new(None)),
            next_worker_id: Arc::new(AtomicU64::new(1)),
            active_worker: Arc::new(AtomicU64::new(0)),
            engine_lease: Arc::new(AtomicU64::new(0)),
            stream_active: Arc::new(AtomicBool::new(false)),
            router: Arc::new(StreamRouter::new()),
        }
    }

    /// Load a model using the provided engine implementation.
    ///
    /// The `engine_factory` lets the caller decide which concrete backend to
    /// instantiate (BurnVoxtral, future sidecar, mock for tests, ...).
    pub async fn load_with<F>(
        &self,
        model_id: &str,
        model_path: &std::path::Path,
        engine_factory: F,
    ) -> Result<()>
    where
        F: FnOnce() -> Box<dyn TranscriptionEngine + Send + Sync>,
    {
        // Unload any previous engine first (frees memory before allocating new one).
        {
            let mut guard = self.engine.lock().await;
            if let Some(e) = guard.as_mut() {
                e.unload();
            }
            *guard = None;
        }

        let mut new_engine = engine_factory();

        info!(
            "Loading engine for model '{}' from {:?}",
            model_id, model_path
        );
        new_engine.load(model_path).await?;

        {
            let mut guard = self.engine.lock().await;
            *guard = Some(new_engine);
        }
        {
            let mut id = self.current_model_id.lock().await;
            *id = Some(model_id.to_string());
        }

        info!("Engine for '{}' loaded successfully", model_id);
        Ok(())
    }

    /// Convenience loader that uses `BurnVoxtralEngine` as the implementation.
    pub async fn load_burn_voxtral(
        &self,
        model_id: &str,
        model_path: &std::path::Path,
    ) -> Result<()> {
        self.load_with(model_id, model_path, || Box::new(BurnVoxtralEngine::new()))
            .await
    }

    pub async fn unload(&self) {
        let mut guard = self.engine.lock().await;
        if let Some(e) = guard.as_mut() {
            e.unload();
        }
        *guard = None;

        let mut id = self.current_model_id.lock().await;
        *id = None;

        debug!("Engine unloaded");
    }

    pub async fn is_model_loaded(&self) -> bool {
        let guard = self.engine.lock().await;
        let lease = self.engine_lease.load(Ordering::Acquire) != 0;
        guard.is_some() || lease
    }

    pub async fn current_model_id(&self) -> Option<String> {
        self.current_model_id.lock().await.clone()
    }

    /// Start a live streaming session.
    ///
    /// Spawns an internal task that owns the engine (via lease) and processes
    /// `StreamCommand`s coming from the `StreamRouter`.
    ///
    /// This is the heart of the realtime path.
    pub async fn start_stream(&self) -> Result<()> {
        if self.router.is_open() || self.active_worker.load(Ordering::Acquire) != 0 {
            warn!("start_stream called while another stream is active");
            return Ok(());
        }

        let worker_id = self.next_worker_id.fetch_add(1, Ordering::Relaxed);
        if self
            .active_worker
            .compare_exchange(0, worker_id, Ordering::AcqRel, Ordering::Acquire)
            .is_err()
        {
            warn!("start_stream lost race");
            return Ok(());
        }

        let mut rx = self.router.open().await;
        self.stream_active.store(false, Ordering::Release);

        let manager = self.clone_for_worker();
        tokio::spawn(async move {
            let _guard = StreamWorkerGuard {
                worker_id,
                active_worker: manager.active_worker.clone(),
                engine_lease: manager.engine_lease.clone(),
                stream_active: manager.stream_active.clone(),
            };

            // Take ownership of the engine for the duration of the stream.
            let lease_ok = manager
                .engine_lease
                .compare_exchange(0, worker_id, Ordering::AcqRel, Ordering::Acquire)
                .is_ok();

            if !lease_ok {
                warn!("Could not acquire engine lease");
                let _ = manager.router.close().await;
                return;
            }

            let mut engine_opt = manager.engine.lock().await.take();

            if let Some(engine) = engine_opt.as_mut() {
                manager.stream_active.store(true, Ordering::Release);
                info!("Streaming worker {} started", worker_id);

                while let Some(cmd) = rx.recv().await {
                    match cmd {
                        StreamCommand::Feed(samples) => {
                            if let Some(update) = engine.feed_audio(&samples) {
                                // In a real system we would emit a Tauri event here
                                // or send the update through a broadcast channel
                                // to the coordinator / UI.
                                debug!("stream update: committed={:?}", update.committed);
                            }
                        }
                        StreamCommand::Finalize(reply) => {
                            let result = engine.finalize().await;
                            let _ = reply.send(result);
                            break;
                        }
                        StreamCommand::Cancel => {
                            engine.reset();
                            break;
                        }
                    }
                }
            }

            // Return the engine to the manager when the worker exits.
            if let Some(e) = engine_opt {
                *manager.engine.lock().await = Some(e);
            }
            let _ = manager.router.close().await;
            manager.stream_active.store(false, Ordering::Release);
            debug!("Streaming worker {} exited", worker_id);
        });

        Ok(())
    }

    /// Stop the current stream (if any) and return the final transcription.
    pub async fn stop_stream(&self) -> Result<String> {
        if !self.router.is_open() {
            return Ok(String::new());
        }

        let (tx, rx) = tokio::sync::oneshot::channel();
        // Send finalize through the router channel so ordering is preserved.
        if let Ok(guard) = self.router.tx.try_lock() {
            if let Some(sender) = guard.as_ref() {
                let _ = sender.send(StreamCommand::Finalize(tx)).await;
            }
        }

        // Wait for the worker to reply (with timeout in real code).
        match tokio::time::timeout(std::time::Duration::from_secs(10), rx).await {
            Ok(Ok(Ok(text))) => Ok(text),
            Ok(Ok(Err(e))) => Err(e),
            _ => {
                // best effort cleanup
                let _ = self.router.close().await;
                Ok(String::new())
            }
        }
    }

    /// Feed a frame directly (used when not in full streaming mode or for tests).
    pub async fn feed_direct(&self, samples: &[f32]) -> Option<TranscriptionUpdate> {
        let mut guard = self.engine.lock().await;
        if let Some(e) = guard.as_mut() {
            e.feed_audio(samples)
        } else {
            None
        }
    }

    fn clone_for_worker(&self) -> Self {
        Self {
            engine: self.engine.clone(),
            current_model_id: self.current_model_id.clone(),
            next_worker_id: self.next_worker_id.clone(),
            active_worker: self.active_worker.clone(),
            engine_lease: self.engine_lease.clone(),
            stream_active: self.stream_active.clone(),
            router: self.router.clone(),
        }
    }
}

impl Clone for EngineManager {
    fn clone(&self) -> Self {
        Self {
            engine: self.engine.clone(),
            current_model_id: self.current_model_id.clone(),
            next_worker_id: self.next_worker_id.clone(),
            active_worker: self.active_worker.clone(),
            engine_lease: self.engine_lease.clone(),
            stream_active: self.stream_active.clone(),
            router: self.router.clone(),
        }
    }
}

impl Default for EngineManager {
    fn default() -> Self {
        Self::new()
    }
}
