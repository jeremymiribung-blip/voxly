//! TranscriptionCoordinator implementation.
//!
//! See module docs for high-level responsibilities.
//!
//! The implementation borrows heavily from the proven design in Handy
//! (`TranscriptionCoordinator` + `TranscriptionManager`) but is rewritten
//! for Voxly's Tokio-first world and the Burn/Voxtral engine abstraction.
//!
//! Safety: the core loop runs inside `catch_unwind`.
//! Audio hot path never touches the coordinator mutex.

use crate::audio::{AudioCapture, CaptureConfig, VadPolicy};
use crate::error::Result;
use crate::events;
use crate::inference::{EngineManager, StreamCommand};
use std::sync::mpsc::{self, Sender};
use std::sync::Arc;
use std::thread;
use std::time::{Duration, Instant};
use tauri::AppHandle;
use tracing::{debug, error, info, warn};

/// Commands that can be sent to the coordinator (from hotkeys, UI, etc.).
#[derive(Debug)]
pub enum CoordinatorCommand {
    /// User pressed a push-to-talk or toggle button.
    /// `push_to_talk = true` means hold-to-talk semantics.
    StartRecording {
        push_to_talk: bool,
    },
    StopRecording,
    /// Cancel everything (emergency stop).
    Cancel,
    /// Request to finalize the current utterance and emit the result.
    Finalize,
    /// Shutdown the coordinator thread.
    Shutdown,
}

/// High-level state of the pipeline (owned exclusively by the coordinator thread).
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum PipelineState {
    Idle,
    Recording,
    Finalizing,
}

/// The central serializing coordinator.
pub struct TranscriptionCoordinator {
    tx: Sender<CoordinatorCommand>,
}

impl TranscriptionCoordinator {
    /// Spawn the coordinator thread and return a handle that can send commands.
    pub fn new(app: AppHandle, engine_manager: Arc<EngineManager>) -> Self {
        let (tx, rx) = mpsc::channel();

        let app_handle = app.clone();

        thread::spawn(move || {
            let result = std::panic::catch_unwind(std::panic::AssertUnwindSafe(|| {
                let mut state = PipelineState::Idle;
                let mut capture: Option<AudioCapture> = None;
                let mut is_ptt = false;

                // Simple debounce state
                let mut last_command = Instant::now();

                loop {
                    let cmd = match rx.recv() {
                        Ok(c) => c,
                        Err(_) => break,
                    };

                    if last_command.elapsed() < Duration::from_millis(30) {
                        // crude debounce for rapid commands
                        continue;
                    }
                    last_command = Instant::now();

                    match cmd {
                        CoordinatorCommand::StartRecording { push_to_talk } => {
                            if state != PipelineState::Idle {
                                debug!("Ignoring start — pipeline busy ({:?})", state);
                                continue;
                            }

                            is_ptt = push_to_talk;
                            state = PipelineState::Recording;

                            // Start audio capture
                            let cfg = CaptureConfig {
                                vad_policy: if push_to_talk {
                                    VadPolicy::Offline
                                } else {
                                    VadPolicy::Streaming
                                },
                                ..Default::default()
                            };

                            // Wire the live router so frames go straight to the engine worker
                            let router = engine_manager.router.clone();
                            type FrameCb = Arc<dyn Fn(&[f32]) + Send + Sync + 'static>;
                            let audio_cb: Option<FrameCb> = Some(Arc::new(move |frame| {
                                router.feed(frame);
                            }));

                            match AudioCapture::start(None, cfg, audio_cb) {
                                Ok(cap) => {
                                    capture = Some(cap);

                                    // Kick off the streaming worker on the engine
                                    // (non-blocking spawn inside EngineManager)
                                    let em = engine_manager.clone();
                                    tauri::async_runtime::spawn(async move {
                                        if let Err(e) = em.start_stream().await {
                                            warn!("Failed to start engine stream: {}", e);
                                        }
                                    });

                                    events::emit_recording_started(&app_handle);
                                    info!("Recording started (ptt={})", push_to_talk);
                                }
                                Err(e) => {
                                    error!("Failed to start audio capture: {}", e);
                                    state = PipelineState::Idle;
                                    events::emit_error(&app_handle, &e.to_string());
                                }
                            }
                        }

                        CoordinatorCommand::StopRecording | CoordinatorCommand::Finalize => {
                            if state != PipelineState::Recording {
                                continue;
                            }
                            state = PipelineState::Finalizing;

                            // Stop the capture first
                            if let Some(mut cap) = capture.take() {
                                cap.stop();
                            }

                            // Ask the engine to finalize
                            let em = engine_manager.clone();
                            let app2 = app_handle.clone();

                            tauri::async_runtime::spawn(async move {
                                match em.stop_stream().await {
                                    Ok(text) => {
                                        events::emit_transcription_final(&app2, &text);
                                        info!("Transcription finalized: {} chars", text.len());
                                    }
                                    Err(e) => {
                                        warn!("Finalize failed: {}", e);
                                        events::emit_error(&app2, &e.to_string());
                                    }
                                }
                            });

                            state = PipelineState::Idle;
                            events::emit_recording_stopped(&app_handle);
                        }

                        CoordinatorCommand::Cancel => {
                            if let Some(mut cap) = capture.take() {
                                cap.stop();
                            }
                            // Best effort cancel on engine
                            let em = engine_manager.clone();
                            tauri::async_runtime::spawn(async move {
                                let _ = em.stop_stream().await;
                            });

                            state = PipelineState::Idle;
                            events::emit_recording_stopped(&app_handle);
                            info!("Recording cancelled");
                        }

                        CoordinatorCommand::Shutdown => {
                            if let Some(mut cap) = capture.take() {
                                cap.stop();
                            }
                            break;
                        }
                    }
                }

                info!("TranscriptionCoordinator loop exited cleanly");
            }));

            if let Err(panic) = result {
                error!("TranscriptionCoordinator panicked: {:?}", panic);
            }
        });

        Self { tx }
    }

    pub fn send(&self, cmd: CoordinatorCommand) {
        if self.tx.send(cmd).is_err() {
            warn!("Coordinator channel closed");
        }
    }

    pub fn start_recording(&self, push_to_talk: bool) {
        self.send(CoordinatorCommand::StartRecording { push_to_talk });
    }

    pub fn stop_recording(&self) {
        self.send(CoordinatorCommand::StopRecording);
    }

    pub fn cancel(&self) {
        self.send(CoordinatorCommand::Cancel);
    }
}
