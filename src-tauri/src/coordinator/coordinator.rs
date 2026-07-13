//! TranscriptionCoordinator - central orchestrator (single source of truth).
//!
//! Tokio-based. Coordinates Audio (VAD/chunks) -> Engine (stateful) -> events.
//! Implements tentative vs committed, safety, metrics, lifecycle.

use crate::audio::{AudioCapture, AudioProcessor, CaptureConfig};
use crate::error::Result;
use crate::events::{self, SessionMetrics};
use crate::inference::EngineManager;
use std::sync::Arc;
use std::time::{Duration, Instant};
use tauri::AppHandle;
use tokio::sync::mpsc;
use tracing::{debug, error, info, warn};

#[derive(Debug, Clone)]
pub enum CoordinatorCommand {
    StartRecording { push_to_talk: bool },
    StopRecording,
    Cancel,
    Finalize,
    ResetContext,
    Shutdown,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum SessionState {
    Idle,
    Listening,
    Processing,
}

pub struct TranscriptionCoordinator {
    cmd_tx: mpsc::Sender<CoordinatorCommand>,
}

impl TranscriptionCoordinator {
    pub fn new(app: AppHandle, engine_manager: Arc<EngineManager>) -> Self {
        let (cmd_tx, mut cmd_rx) = mpsc::channel(64);
        let app_handle = app.clone();
        let em = engine_manager.clone();

        tauri::async_runtime::spawn(async move {
            let mut state = SessionState::Idle;
            let mut capture: Option<AudioCapture> = None;
            let mut processor: Option<AudioProcessor> = None;
            let mut session_start: Option<Instant> = None;
            let mut samples_processed: u64 = 0;
            let mut last_metrics = Instant::now();

            info!("TranscriptionCoordinator started");

            loop {
                // commands
                if let Ok(Some(cmd)) =
                    tokio::time::timeout(Duration::from_millis(30), cmd_rx.recv()).await
                {
                    match cmd {
                        CoordinatorCommand::StartRecording { push_to_talk } => {
                            if state != SessionState::Idle {
                                continue;
                            }
                            state = SessionState::Listening;
                            session_start = Some(Instant::now());
                            samples_processed = 0;

                            let cfg = CaptureConfig {
                                ..Default::default()
                            };
                            if let Ok(mut cap) = AudioCapture::start(None, cfg.clone()) {
                                if let Some(raw_rx) = cap.take_consumer() {
                                    if let Ok(proc) =
                                        AudioProcessor::start(raw_rx, cfg, !push_to_talk)
                                    {
                                        capture = Some(cap);
                                        processor = Some(proc);
                                        let em2 = em.clone();
                                        tauri::async_runtime::spawn(async move {
                                            let _ = em2.start_stream().await;
                                        });
                                        events::emit_recording_started(&app_handle);
                                        events::emit_session_status(&app_handle, "listening", None);
                                        info!("listening");
                                    }
                                }
                            }
                        }
                        CoordinatorCommand::StopRecording | CoordinatorCommand::Finalize => {
                            if state == SessionState::Idle {
                                continue;
                            }
                            state = SessionState::Processing;
                            if let Some(p) = &processor {
                                let _ = p.finalize_tx.send(());
                            }
                            if let Some(mut c) = capture.take() {
                                c.stop();
                            }
                            let em2 = em.clone();
                            let h = app_handle.clone();
                            tauri::async_runtime::spawn(async move {
                                if let Ok(t) = em2.stop_stream().await {
                                    events::emit_transcription_final(&h, &t);
                                }
                                events::emit_session_status(&h, "idle", None);
                            });
                            processor = None;
                            state = SessionState::Idle;
                            events::emit_recording_stopped(&app_handle);
                        }
                        CoordinatorCommand::Cancel => {
                            if let Some(mut c) = capture.take() {
                                c.stop();
                            }
                            if let Some(p) = processor.take() {
                                let _ = p.finalize_tx.send(());
                            }
                            let _ = em.stop_stream().await;
                            state = SessionState::Idle;
                            events::emit_recording_stopped(&app_handle);
                            events::emit_session_status(&app_handle, "idle", None);
                        }
                        CoordinatorCommand::ResetContext => {
                            let _ = em.stop_stream().await;
                        }
                        CoordinatorCommand::Shutdown => break,
                    }
                    continue;
                }

                // chunks (non blocking)
                if let Some(p) = &mut processor {
                    if let Ok(chunk) = p.chunk_rx.try_recv() {
                        samples_processed += chunk.samples.len() as u64;
                        let em2 = em.clone();
                        let h = app_handle.clone();
                        tauri::async_runtime::spawn(async move {
                            if let Some(u) = em2.feed_direct(&chunk.samples).await {
                                events::emit_transcription_update(
                                    &h,
                                    &u.committed,
                                    u.tentative.as_deref(),
                                    None,
                                );
                            }
                        });
                    }
                }

                // metrics
                if last_metrics.elapsed() > Duration::from_secs(2) {
                    if let Some(s) = session_start {
                        let dur = (samples_processed as f64 / 16000.0 * 1000.0) as u64;
                        let rtf = if dur > 0 {
                            (s.elapsed().as_millis() as f64 / dur as f64) as f32
                        } else {
                            0.0
                        };
                        events::emit_session_status(
                            &app_handle,
                            "listening",
                            Some(SessionMetrics {
                                rtf,
                                tokens_per_sec: 0.0,
                                audio_duration_ms: dur,
                            }),
                        );
                    }
                    last_metrics = Instant::now();
                }

                tokio::time::sleep(Duration::from_millis(10)).await;
            }
            info!("TranscriptionCoordinator exited");
        });

        Self { cmd_tx }
    }

    pub fn send(&self, cmd: CoordinatorCommand) {
        let _ = self.cmd_tx.try_send(cmd);
    }
    pub fn start_recording(&self, ptt: bool) {
        self.send(CoordinatorCommand::StartRecording { push_to_talk: ptt });
    }
    pub fn stop_recording(&self) {
        self.send(CoordinatorCommand::StopRecording);
    }
    pub fn cancel(&self) {
        self.send(CoordinatorCommand::Cancel);
    }
    pub fn reset_context(&self) {
        self.send(CoordinatorCommand::ResetContext);
    }
}

/*
Data flow:
UI/Hotkey -> cmd_tx (mpsc)
Coordinator task:
  Start -> AudioCapture + AudioProcessor (VAD+80ms chunks) -> processor.chunk_rx
  loop (try_recv + sleep):
    chunk -> em.feed_direct -> BurnVoxtralEngine (updates KV caches)
    engine update -> emit "transcription-update" (committed + tentative)
    on finalize -> engine finalize + emit final + status
Safety: drops, try_send, engine guards.
Metrics emitted periodically.
Coordinator owns session state.
*/
