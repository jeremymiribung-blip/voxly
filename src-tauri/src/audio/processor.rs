//! Audio processing utilities: chunking, buffering, simple feature extraction.
//!
//! For the realtime Voxtral path most work happens inside the model itself.
//! This module provides helpers used by the coordinator when doing
//! push-to-talk or "finalize current buffer" flows.

use std::collections::VecDeque;

/// A simple rolling buffer that can be used to accumulate audio until
/// a finalize command or VAD decides an utterance has ended.
pub struct AudioBuffer {
    buffer: VecDeque<f32>,
    max_samples: usize,
}

impl AudioBuffer {
    pub fn new(max_duration_seconds: f32, sample_rate: u32) -> Self {
        let max_samples = (max_duration_seconds * sample_rate as f32) as usize;
        Self {
            buffer: VecDeque::with_capacity(max_samples),
            max_samples,
        }
    }

    pub fn push(&mut self, samples: &[f32]) {
        for &s in samples {
            if self.buffer.len() >= self.max_samples {
                self.buffer.pop_front();
            }
            self.buffer.push_back(s);
        }
    }

    pub fn take_all(&mut self) -> Vec<f32> {
        self.buffer.drain(..).collect()
    }

    pub fn len(&self) -> usize {
        self.buffer.len()
    }

    pub fn is_empty(&self) -> bool {
        self.buffer.is_empty()
    }

    pub fn clear(&mut self) {
        self.buffer.clear();
    }
}
