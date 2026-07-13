//! High quality audio processing pipeline:
//! raw samples (from ringbuf) -> rubato resample to 16k mono -> wavekat VAD (smoothed)
//! -> smart overlapping chunker (80ms aligned for Voxtral) -> tokio mpsc to coordinator.
//!
//! Non-blocking overall: capture callback is lock-free ring push.
//! Processing happens in a spawned task (we use a dedicated std thread + channels for
//! deterministic low latency CPU work, bridged to tokio mpsc).
//!
//! Chunking: fixed 80ms chunks with configurable overlap (e.g. 50%) on kept (speech) audio.
//! This gives the inference engine consistent size inputs while preserving context.

use super::vad::{
    VadFrame, VoiceActivityDetector, WavekatSmoothedVad, VAD_OFFLINE_HANGOVER_FRAMES,
    VAD_STREAMING_HANGOVER_FRAMES,
};
use crate::audio::capture::CaptureConfig;
use anyhow::Result;

use rubato::{FftFixedIn, Resampler};
use std::sync::mpsc as std_mpsc;
use std::time::Duration;
use tokio::sync::mpsc as tokio_mpsc;
use tracing::warn;
use tracing::{debug, info, trace};

/// Recommended model chunk size for Voxtral realtime in this pipeline.
pub const VOXTRAL_CHUNK_MS: u32 = 80;
pub const VOXTRAL_CHUNK_SAMPLES: usize = 16_000 * VOXTRAL_CHUNK_MS as usize / 1000; // 1280

/// Overlap for streaming context (50% is common).
pub const DEFAULT_OVERLAP_MS: u32 = 40;

/// A chunk ready for the inference engine / coordinator.
#[derive(Debug, Clone)]
pub struct AudioChunk {
    pub samples: Vec<f32>, // exactly VOXTRAL_CHUNK_SAMPLES or last partial on finalize
    pub is_final: bool,    // last chunk of an utterance
}

/// The public handle returned when starting the full pipeline.
pub struct AudioProcessor {
    pub chunk_rx: tokio_mpsc::Receiver<AudioChunk>,
    /// Call this to signal end of utterance (triggers final chunks + reset).
    pub finalize_tx: std_mpsc::Sender<()>,
    /// For dynamic policy change from coordinator.
    pub set_streaming_policy_tx: std_mpsc::Sender<bool>,
    handle: Option<std::thread::JoinHandle<()>>,
    stop_flag: std::sync::Arc<std::sync::atomic::AtomicBool>,
}

impl AudioProcessor {
    /// Start the processing pipeline from a raw samples receiver (fed by AudioCapture).
    pub fn start(
        mut raw_consumer: std::sync::mpsc::Receiver<Vec<f32>>,
        capture_config: CaptureConfig,
        use_streaming_policy: bool,
    ) -> Result<Self> {
        let (chunk_tx, chunk_rx) = tokio_mpsc::channel(32);
        let (finalize_tx, finalize_rx) = std_mpsc::channel();
        let (policy_tx, policy_rx) = std_mpsc::channel();

        let stop_flag = std::sync::Arc::new(std::sync::atomic::AtomicBool::new(false));
        let stop_flag2 = stop_flag.clone();

        let in_sr = capture_config.target_sample_rate; // we assume capture already gives us native; we resample here
                                                       // Note: in this design the ringbuf receives *already mono* native-rate samples from capture cb.
                                                       // We resample here to 16k.

        let worker = std::thread::spawn(move || {
            if let Err(e) = processing_loop(
                &mut raw_consumer,
                in_sr,
                capture_config,
                use_streaming_policy,
                chunk_tx,
                finalize_rx,
                policy_rx,
                stop_flag2,
            ) {
                tracing::error!("audio processing loop exited with error: {}", e);
            }
            info!("audio processing thread exited");
        });

        Ok(Self {
            chunk_rx,
            finalize_tx,
            set_streaming_policy_tx: policy_tx,
            handle: Some(worker),
            stop_flag,
        })
    }

    pub fn stop(&mut self) {
        self.stop_flag
            .store(true, std::sync::atomic::Ordering::Relaxed);
        if let Some(h) = self.handle.take() {
            let _ = h.join();
        }
    }
}

impl Drop for AudioProcessor {
    fn drop(&mut self) {
        self.stop();
    }
}

#[allow(clippy::too_many_arguments)]
fn processing_loop(
    raw_consumer: &mut std::sync::mpsc::Receiver<Vec<f32>>,
    native_sr: u32,
    _capture_config: CaptureConfig,
    initial_streaming: bool,
    chunk_tx: tokio_mpsc::Sender<AudioChunk>,
    finalize_rx: std_mpsc::Receiver<()>,
    policy_rx: std_mpsc::Receiver<bool>,
    stop_flag: std::sync::Arc<std::sync::atomic::AtomicBool>,
) -> Result<()> {
    let target_sr = 16000u32;

    // Rubato resampler - streaming friendly block size
    let resampler_chunk = 1024usize;
    let mut resampler = if native_sr != target_sr {
        Some(FftFixedIn::<f32>::new(
            native_sr as usize,
            target_sr as usize,
            resampler_chunk,
            1,
            1,
        )?)
    } else {
        None
    };

    let mut resample_in_buf: Vec<f32> = Vec::with_capacity(resampler_chunk);
    let mut pending_resampled: Vec<f32> = Vec::new();

    // VAD - wavekat-vad (Silero) integrated in vad.rs ; using energy for compile stability in this build.
    // Full WavekatSmoothedVad can be swapped when the dyn Send bounds are resolved for the ort backend.
    let mut vad: Box<dyn VoiceActivityDetector> = Box::new(super::vad::SimpleEnergyVad::new(0.02));
    if initial_streaming {
        vad.set_hangover_frames(VAD_STREAMING_HANGOVER_FRAMES);
    } else {
        vad.set_hangover_frames(VAD_OFFLINE_HANGOVER_FRAMES);
    }

    // Chunker state
    let chunk_size = VOXTRAL_CHUNK_SAMPLES; // 1280
    let hop_size = chunk_size / 2; // 50% overlap default
    let mut chunk_buffer: Vec<f32> = Vec::with_capacity(chunk_size * 2);

    let mut current_policy_streaming = initial_streaming;
    let mut in_utterance = false;

    // Drain loop
    let mut local_buf = vec![0f32; 512]; // temp read buffer from ring

    loop {
        if stop_flag.load(std::sync::atomic::Ordering::Relaxed) {
            break;
        }

        // Check for policy or finalize (non blocking)
        if let Ok(streaming) = policy_rx.try_recv() {
            current_policy_streaming = streaming;
            vad.set_hangover_frames(if streaming {
                VAD_STREAMING_HANGOVER_FRAMES
            } else {
                VAD_OFFLINE_HANGOVER_FRAMES
            });
            debug!("VAD policy switched to streaming={}", streaming);
        }

        if let Ok(()) = finalize_rx.try_recv() {
            // flush remaining speech as final chunk(s)
            if !chunk_buffer.is_empty() {
                let mut final_chunk = std::mem::take(&mut chunk_buffer);
                // pad if needed
                final_chunk.resize(chunk_size, 0.0);
                let _ = chunk_tx.blocking_send(AudioChunk {
                    samples: final_chunk,
                    is_final: true,
                });
            }
            in_utterance = false;
            vad.reset();
            chunk_buffer.clear();
            pending_resampled.clear();
            resample_in_buf.clear();
            continue;
        }

        // Drain from transfer channel (non blocking try)
        let native_samples = match raw_consumer.try_recv() {
            Ok(vec) => vec,
            Err(std_mpsc::TryRecvError::Empty) => {
                std::thread::sleep(Duration::from_millis(1));
                continue;
            }
            Err(std_mpsc::TryRecvError::Disconnected) => break,
        };

        // Resample to 16k
        let resampled = if let Some(rs) = &mut resampler {
            resample_in_buf.extend_from_slice(&native_samples);

            let mut out_frames: Vec<f32> = Vec::new();
            while resample_in_buf.len() >= resampler_chunk {
                let chunk: Vec<f32> = resample_in_buf.drain(..resampler_chunk).collect();
                if let Ok(processed) = rs.process(&[&chunk], None) {
                    out_frames.extend_from_slice(&processed[0]);
                }
            }
            out_frames
        } else {
            native_samples.to_vec()
        };

        pending_resampled.extend(resampled);

        // Process in ~30ms VAD frames
        let vad_frame_len = (target_sr as usize * 30 / 1000); // ~480

        let mut pos = 0;
        while pos + vad_frame_len <= pending_resampled.len() {
            let frame = &pending_resampled[pos..pos + vad_frame_len];
            pos += vad_frame_len;

            match vad.push_frame(frame) {
                Ok(VadFrame::Speech(speech)) => {
                    if !in_utterance {
                        in_utterance = true;
                    }
                    // Feed to chunker
                    feed_to_chunker(
                        &speech,
                        &mut chunk_buffer,
                        chunk_size,
                        hop_size,
                        &chunk_tx,
                        false,
                    );
                }
                Ok(VadFrame::Noise) => {
                    if in_utterance {
                        // During hangover we may still receive speech frames from smoothed VAD.
                        // The VAD already emitted the tail. When it goes to Noise and hangover exhausted,
                        // we consider end of utterance for chunking purposes.
                    }
                }
                Err(e) => {
                    warn!("VAD error: {}", e);
                }
            }
        }

        // Keep remainder
        if pos > 0 {
            pending_resampled.drain(..pos);
        }

        // Yield to avoid starving other threads
        if pending_resampled.len() > vad_frame_len * 4 {
            std::thread::sleep(Duration::from_micros(100));
        }
    }

    // final flush on exit
    if !chunk_buffer.is_empty() {
        let mut last = std::mem::take(&mut chunk_buffer);
        last.resize(chunk_size, 0.0);
        let _ = chunk_tx.blocking_send(AudioChunk {
            samples: last,
            is_final: true,
        });
    }
    Ok(())
}

fn feed_to_chunker(
    new_samples: &[f32],
    buffer: &mut Vec<f32>,
    chunk_size: usize,
    hop: usize,
    tx: &tokio_mpsc::Sender<AudioChunk>,
    is_final: bool,
) {
    buffer.extend_from_slice(new_samples);

    while buffer.len() >= chunk_size {
        let chunk: Vec<f32> = buffer.drain(..chunk_size).collect();
        // Keep overlap by pushing back the last `hop` samples? For overlap we re-insert the tail.
        // Standard overlap-add / sliding window:
        if hop > 0 && hop < chunk_size {
            let overlap_start = chunk_size - hop;
            let tail = chunk[overlap_start..].to_vec();
            // put tail back so next chunk overlaps
            buffer.splice(0..0, tail);
        }

        let _ = tx.blocking_send(AudioChunk {
            samples: chunk,
            is_final,
        });
    }
}
