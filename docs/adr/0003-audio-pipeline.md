# ADR 0003: Audio Pipeline Architecture

**Status:** Accepted  
**Date:** 2026-07-14  
**Deciders:** Project team  
**Related:** ADR 0001, ADR 0002

## Context

Voxly requires a low-latency (<600ms end-to-end target), robust, privacy-focused audio capture and preprocessing pipeline to feed the Voxtral Mini Realtime inference engine.

Key challenges:
- Cross-platform audio input (macOS, Windows primary; Linux secondary)
- High-quality resampling to 16kHz mono (model requirement)
- Voice activity detection (VAD) with dynamic trailing silence for streaming vs push-to-talk use cases
- Overlapping chunking aligned with model frame rates (~80ms)
- Non-blocking operation to avoid jank in UI or inference
- Lock-free or low-contention data transfer from OS audio callbacks
- Support for device selection and basic configuration

We studied Handy's audio toolkit (cpal usage, rubato resampler with reset hygiene, Silero VAD + SmoothedVad with prefill/onset/hangover, VadPolicy).

## Decision

### Core Components

1. **Capture Layer** (`audio/capture.rs`):
   - Direct `cpal` (preferred over `decibri`/`stream-audio` at this stage for full control and to match rubato requirement).
   - Non-blocking callback writing to a transfer mechanism (initially bounded mpsc; prepared for `ringbuf` SPSC lock-free).
   - Mono mixing, device selection by name or default.
   - Stop flag for clean shutdown.

2. **Preprocessing & VAD** (`audio/processor.rs` + `vad.rs`):
   - `rubato` (FftFixedIn or similar) for high-quality resampling to 16kHz.
   - `wavekat-vad` (Silero backend preferred; TEN-VAD supported) as primary VAD.
   - `VoiceActivityDetector` trait + `WavekatSmoothedVad` wrapper implementing:
     - Dynamic hangover/trailing time (longer for streaming/live preview, shorter for precision/PTT).
     - Onset protection (minimum consecutive positive frames, default 2).
     - Prefill buffering for natural speech starts.
   - Constants inspired by Handy: `VAD_STREAMING_HANGOVER_FRAMES`, `VAD_OFFLINE_HANGOVER_FRAMES`, `VAD_ONSET_FRAMES`, `VAD_PREFILL_FRAMES`.

3. **Chunking**:
   - Smart overlapping chunks (80ms = `VOXTRAL_CHUNK_SAMPLES` at 16kHz, 50% hop by default).
   - `AudioChunk { samples, is_final }` produced for the coordinator/engine.
   - Overlap preserves context for streaming inference.

4. **Delivery**:
   - `AudioProcessor` spawns a dedicated worker thread (CPU-bound work) that bridges to `tokio::mpsc` channels for the coordinator.
   - Backpressure via bounded channels.
   - Policy switching (streaming vs offline) at runtime.

### Why not pure decibri or other?
- `decibri` is mature and bundles VAD/resampling, but we require explicit `rubato` + `wavekat-vad` control and wanted to follow "use cpal directly with lock-free ring buffer pattern when ideal."
- Direct cpal + our stack gives precise alignment to Voxtral's 80ms and Handy's proven VAD policy.

### Safety & Performance
- Capture callback is extremely lightweight (no locks in hot path where possible).
- Worker thread + channels prevent blocking the main Tauri/async runtime.
- Reset hygiene on resampler and VAD between utterances (prevents leakage, as learned from Handy tests).

## Consequences

**Positive:**
- Matches model input requirements exactly.
- Excellent VAD behavior for both always-on and PTT scenarios.
- Testable chunker/VAD logic.
- Clear separation: capture → processor → coordinator.

**Trade-offs / Risks:**
- More code to maintain than using a single high-level crate.
- Q4 GGUF + custom shaders on GPU side means audio pipeline must be reliable; any buffer issues show up as high latency or cut-off speech.
- Wavekat-vad (ONNX) adds a runtime dependency (handled via features).

**Future Work:**
- Full `ringbuf` SPSC for capture → processor transfer.
- Optional `decibri` backend behind feature flag.
- More sophisticated onset / hangover tuning per device or user.

## References
- Handy `audio_toolkit/{audio,resampler,vad}`
- wavekat-vad crate docs (Silero/TEN frame sizes, trait)
- rubato streaming resampler patterns
- Voxtral Mini Realtime paper / crate README (16kHz, causal encoder, configurable delay)
