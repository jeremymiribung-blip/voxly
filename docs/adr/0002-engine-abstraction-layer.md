# ADR 0002: Engine Abstraction Layer & Core Crate Structure

**Status:** Accepted  
**Date:** 2026-07-13  
**Deciders:** Engineering (following initial requirements + study of Handy)

## Context

Voxly must support a primary inference backend (Burn + `voxtral-mini-realtime-rs`) while remaining open to future backends (CrispASR, ONNX variants, sidecar processes, etc.).

The audio pipeline, coordinator, and UI must not be tightly coupled to any one model implementation.

We studied the Handy codebase in detail (TranscriptionCoordinator, LoadedEngine + dispatch, StreamRouter + atomic fast path, LoadingGuard / StreamWorkerGuard RAII + lease atomics with worker IDs, catch_unwind safety, VAD trait with dynamic hangover, ModelManager + hf-hub + capability probing).

## Decision

### 1. Directory & Module Structure (inside `src-tauri/src/`)

```
src/
├── audio/          # Capture (cpal), preprocessing (rubato), VAD, chunking, processor
├── inference/      # Engine abstraction + concrete impls
│   ├── engine.rs   # TranscriptionEngine trait + TranscriptionUpdate
│   ├── burn_engine.rs
│   └── manager.rs  # EngineManager + StreamRouter
├── coordinator/    # TranscriptionCoordinator (state machine + serialization)
├── model/          # HF download, cache, versioning, ModelManager
├── commands/       # Tauri command handlers (thin)
├── events.rs       # Typed event emitters
├── error.rs        # VoxlyError (thiserror) + Result
└── config.rs       # AppConfig (in-memory for now)
```

A root `Cargo.toml` workspace + `crates/` is already prepared for future extraction.

### 2. Engine Abstraction (`TranscriptionEngine` trait)

```rust
pub trait TranscriptionEngine: Send + Sync {
    async fn load(&mut self, path: &Path) -> Result<()>;
    fn unload(&mut self);
    fn feed_audio(&mut self, samples: &[f32]) -> Option<TranscriptionUpdate>;
    async fn finalize(&mut self) -> Result<String>;
    fn reset(&mut self);
    // ...
}
```

- `feed_audio` is synchronous and expected to be cheap (hot path).
- Load / finalize are async because they may be heavy.
- `TranscriptionUpdate { committed, tentative, ... }` for live preview.

### 3. EngineManager

- Holds `Arc<Mutex<Option<Box<dyn TranscriptionEngine>>>>`
- Provides `load_with(factory)`, `load_burn_voxtral`, `unload`
- Owns a `StreamRouter` (atomic fast-path + mpsc)
- Uses worker-id + lease atomics (`active_worker`, `engine_lease`) + RAII `StreamWorkerGuard`
- Streaming worker task **takes** the engine out of the mutex for the duration (prevents concurrent use)
- Returns ownership on exit (even on panic via Drop)

This is a direct, improved adaptation of Handy's lease + guard pattern using Tokio channels / `tokio::sync::Mutex` where beneficial.

### 4. BurnVoxtralEngine

- Placeholder today that simulates realistic streaming behavior (periodic partial + final text).
- Extensive documentation of the integration points with `voxtral-mini-realtime-rs`.
- Will become the real implementation behind a feature flag once the crate dependency and Burn device setup are finalized.

### 5. Coordinator

- Single dedicated OS thread running a `catch_unwind` loop (Handy pattern).
- `mpsc` command channel serializes Start/Stop/Cancel/Finalize.
- Owns `AudioCapture` lifecycle.
- Drives `EngineManager::start_stream()` / `stop_stream()`.
- Emits events via the new `events` module.

### 6. Audio Layer

- `cpal` + `rubato` for capture + resampling (16 kHz target).
- `VoiceActivityDetector` trait + `SimpleEnergyVad` placeholder (hangover support).
- `VadPolicy` (Disabled / Offline / Streaming) to choose tail length.
- Frames fed to `StreamRouter` (lock-free check) **and** a channel for the coordinator.

Future: replace energy VAD with proper Silero/TEN via `wavekat-vad` or equivalent.

### 7. Model Layer

- `ModelManager` focused on the primary Voxtral model.
- Uses `hf-hub` (tokio) for downloads.
- Progress events (`model-download-progress`).
- Cache under `app_data_dir()/models`.
- Cheap "is downloaded" check + async `ensure_primary_model`.

Capability probing (GGUF header) will be added in a follow-up once the exact weight artifact shape is known.

### 8. Error Handling & Observability

- `VoxlyError` with `thiserror` variants + `From<anyhow::Error>`.
- `tracing` (already added) for structured logs.
- Commands return `Result<_, String>` for simple frontend consumption.

## Consequences

**Positive**
- Clean separation: audio / inference / orchestration / model / UI are independent.
- Easy to add new engines (implement trait + register factory).
- Proven safety patterns from Handy (lease, guards, catch_unwind, atomic fast-path) are present and documented.
- Tokio-friendly while preserving the "single thread of truth" for lifecycle.
- Placeholders allow frontend + coordinator development to proceed before the heavy Burn integration.

**Trade-offs / Risks**
- The exact public API of `voxtral-mini-realtime-rs` is still evolving; the trait may need small adjustments.
- We are using a simple energy VAD initially — quality of segmentation will be lower until a real neural VAD is plugged in.
- Streaming worker takes ownership of the engine; we must be careful with unload-during-stream scenarios (the lease model helps).

## References

- ADR 0001 (initial architecture)
- Detailed study of `transcription_coordinator.rs`, `managers/transcription.rs`, `managers/model.rs`, `model_capabilities.rs`, `audio_toolkit/*` in Handy
- `voxtral-mini-realtime-rs` GitHub repository and Mistral Voxtral Realtime paper

---

Future ADRs will cover:
- Exact Burn device selection & feature flags
- Real VAD integration
- Model manifest / versioning format
- Sidecar engine plugin mechanism
