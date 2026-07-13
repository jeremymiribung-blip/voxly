//! Low-level audio capture using cpal.
//!
//! This module opens an input stream and delivers resampled mono f32 frames
//! at the target sample rate (usually 16 kHz) to a consumer.
//!
//! The consumer is typically the `StreamRouter` (for live) or a bounded
//! channel that the coordinator drains for batch transcription.
//!
//! Heavily inspired by Handy's `AudioRecorder` but written with more
//! emphasis on Tokio and our chosen VAD policy model.

use super::vad::{SimpleEnergyVad, VadFrame, VadPolicy, VoiceActivityDetector};
use crate::error::{Result, VoxlyError};
use cpal::traits::{DeviceTrait, HostTrait, StreamTrait};
use cpal::{Device, Sample, Stream, StreamConfig};
use rubato::{
    Resampler, SincFixedIn, SincInterpolationParameters, SincInterpolationType, WindowFunction,
};
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::mpsc;
use std::sync::Arc;
use std::thread;
use tracing::{debug, error, info, warn};

/// Configuration for a capture session.
#[derive(Clone, Debug)]
pub struct CaptureConfig {
    pub target_sample_rate: u32,
    pub vad_enabled: bool,
    pub vad_policy: VadPolicy,
    pub frame_size_ms: u32, // 30 ms is typical for VAD
}

impl Default for CaptureConfig {
    fn default() -> Self {
        Self {
            target_sample_rate: 16000,
            vad_enabled: true,
            vad_policy: VadPolicy::Streaming,
            frame_size_ms: 30,
        }
    }
}

/// The public handle returned when capture is started.
/// Dropping it (or calling stop) ends the stream.
pub struct AudioCapture {
    stream: Option<Stream>,
    stop_flag: Arc<AtomicBool>,
    /// Channel that receives clean 16 kHz frames that survived VAD (when enabled).
    pub frame_rx: mpsc::Receiver<Vec<f32>>,
}

impl AudioCapture {
    /// Start capturing from the default input device (or a named device).
    ///
    /// Returns a handle. While the handle lives, audio is captured in a
    /// background thread (cpal requirement).
    pub fn start(
        device_name: Option<&str>,
        config: CaptureConfig,
        audio_cb: Option<Arc<dyn Fn(&[f32]) + Send + Sync + 'static>>,
    ) -> Result<Self> {
        let host = cpal::default_host();

        let device = if let Some(name) = device_name {
            host.input_devices()
                .map_err(|e| VoxlyError::audio(e.to_string()))?
                .find(|d| d.name().map(|n| n == name).unwrap_or(false))
                .ok_or_else(|| VoxlyError::audio(format!("device not found: {name}")))?
        } else {
            host.default_input_device()
                .ok_or_else(|| VoxlyError::audio("no default input device"))?
        };

        info!("Using input device: {:?}", device.name());

        let device_config = device
            .default_input_config()
            .map_err(|e| VoxlyError::audio(e.to_string()))?;

        let sample_rate_in = device_config.sample_rate().0;
        let channels = device_config.channels() as usize;

        let target_sr = config.target_sample_rate;

        // Build a resampler (only if rates differ).
        let mut resampler: Option<SincFixedIn<f32>> = if sample_rate_in != target_sr {
            let params = SincInterpolationParameters {
                sinc_len: 256,
                f_cutoff: 0.95,
                interpolation: SincInterpolationType::Linear,
                oversampling_factor: 256,
                window: WindowFunction::BlackmanHarris2,
            };
            Some(
                SincFixedIn::<f32>::new(
                    target_sr as f64 / sample_rate_in as f64,
                    1.0,
                    params,
                    1024, // chunk size for resampler
                    channels,
                )
                .map_err(|e| VoxlyError::audio(format!("resampler init: {e}")))?,
            )
        } else {
            None
        };

        let frame_samples = (target_sr * config.frame_size_ms / 1000) as usize;

        let (frame_tx, frame_rx) = mpsc::channel::<Vec<f32>>();

        let stop_flag = Arc::new(AtomicBool::new(false));
        let stop_flag_clone = stop_flag.clone();

        let mut vad: Option<Box<dyn VoiceActivityDetector>> = if config.vad_enabled {
            let mut v = Box::new(SimpleEnergyVad::new(0.02)); // tunable
            let hangover = match config.vad_policy {
                VadPolicy::Streaming => 50, // ~1.5 s
                VadPolicy::Offline => 15,
                VadPolicy::Disabled => 0,
            };
            v.set_hangover_frames(hangover);
            Some(v)
        } else {
            None
        };

        // The actual cpal stream callback.
        let stream = device
            .build_input_stream(
                &StreamConfig {
                    channels: device_config.channels(),
                    sample_rate: device_config.sample_rate(),
                    buffer_size: cpal::BufferSize::Default,
                },
                move |data: &[f32], _info: &cpal::InputCallbackInfo| {
                    if stop_flag_clone.load(Ordering::Relaxed) {
                        return;
                    }

                    // Convert to mono if needed (very naive average).
                    let mono: Vec<f32> = if channels == 1 {
                        data.to_vec()
                    } else {
                        data.chunks(channels)
                            .map(|chunk| chunk.iter().sum::<f32>() / channels as f32)
                            .collect()
                    };

                    // Resample if necessary.
                    let resampled = if let Some(r) = &mut resampler {
                        // rubato expects chunks; for simplicity we feed what we have
                        match r.process(&[mono.clone()], None) {
                            Ok(mut out) => out.remove(0),
                            Err(_) => mono, // fallback (moved on error path)
                        }
                    } else {
                        mono
                    };

                    // Feed through VAD (or bypass).
                    let frames_to_emit: Vec<Vec<f32>> = if let Some(v) = &mut vad {
                        // Split into VAD-sized frames
                        let mut out = Vec::new();
                        for chunk in resampled.chunks(frame_samples) {
                            if chunk.len() < frame_samples {
                                continue;
                            }
                            if let Ok(VadFrame::Speech(s)) = v.push_frame(chunk) {
                                out.push(s.to_vec());
                            }
                        }
                        out
                    } else {
                        // VAD disabled — just chunk the audio
                        resampled
                            .chunks(frame_samples)
                            .map(|c| c.to_vec())
                            .collect()
                    };

                    for frame in frames_to_emit {
                        // Optional external callback (used by StreamRouter)
                        if let Some(cb) = &audio_cb {
                            cb(&frame);
                        }

                        // Send to whoever owns the capture handle (coordinator / buffer)
                        let _ = frame_tx.send(frame);
                    }
                },
                move |err| {
                    error!("Audio stream error: {}", err);
                },
                None,
            )
            .map_err(|e| VoxlyError::audio(e.to_string()))?;

        stream
            .play()
            .map_err(|e| VoxlyError::audio(e.to_string()))?;

        info!("Audio capture started (target {} Hz)", target_sr);

        Ok(Self {
            stream: Some(stream),
            stop_flag,
            frame_rx,
        })
    }

    /// Stop capture. The handle can be dropped after this.
    pub fn stop(&mut self) {
        self.stop_flag.store(true, Ordering::Relaxed);
        if let Some(s) = self.stream.take() {
            drop(s); // stops the stream
        }
        debug!("Audio capture stopped");
    }
}

impl Drop for AudioCapture {
    fn drop(&mut self) {
        self.stop();
    }
}
