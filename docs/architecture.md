# Voxly Architecture

This document gives a deeper overview than the README. See the individual ADRs for the "why".

## High-Level Layers

```
┌─────────────────────────────────────────────────────────────┐
│                        Svelte 5 Frontend                     │
│  (runes, Tailwind, events via @tauri-apps/api)              │
│  - Transcription view (committed + tentative)               │
│  - Onboarding / download progress                           │
│  - Settings, device selection, controls                     │
└──────────────────────────┬──────────────────────────────────┘
                           │ Tauri IPC (commands + events)
┌──────────────────────────▼──────────────────────────────────┐
│                    Rust Tauri Backend (v2)                   │
│                                                              │
│  TranscriptionCoordinator (tokio task)                       │
│   ├── owns session state                                     │
│   ├── drives AudioCapture + AudioProcessor                   │
│   ├── feeds chunks → EngineManager / Engine                  │
│   └── emits UI events + collects metrics                     │
│                                                              │
│  Audio Pipeline                                              │
│   ├── capture.rs (cpal)                                      │
│   ├── processor.rs (rubato + VAD + 80ms overlapping chunks)  │
│   └── vad.rs (wavekat-vad + Smoothed policy)                 │
│                                                              │
│  Inference                                                   │
│   ├── engine.rs (TranscriptionEngine trait)                  │
│   ├── burn_engine.rs (BurnVoxtralEngine + KV cache)          │
│   └── manager.rs (EngineManager + StreamRouter + guards)     │
│                                                              │
│  Model Management                                            │
│   └── model/ (resumable HF downloads, cache, quants)         │
└──────────────────────────────────────────────────────────────┘
```

## Data Flow (Live Transcription)

1. User clicks Start (or hotkey).
2. Coordinator starts `AudioCapture` → raw PCM frames.
3. `AudioProcessor` consumes raw → resamples → VAD (with dynamic hangover) → overlapping `AudioChunk`s via `tokio::mpsc`.
4. Coordinator receives chunks and calls into the engine (directly or via `EngineManager` streaming path).
5. `BurnVoxtralEngine` runs stateful forward passes, maintaining KV caches across chunks.
6. Engine returns `TranscriptionUpdate { committed, tentative }`.
7. Coordinator decides boundaries and emits `transcription-update` + `session-status`.
8. Frontend updates runes → UI re-renders only what changed.

On Stop/Finalize:
- Signal finalize through processor + engine.
- Emit final committed text.
- Reset internal buffers (but keep model loaded unless explicitly unloaded).

## Safety Patterns (inspired by Handy)

- `catch_unwind` around long-running worker threads/tasks.
- RAII guards (`StreamWorkerGuard`, handle drops) that clean state even on panic.
- Atomic lease / worker ID scheme so an old worker cannot corrupt a newer session.
- Bounded channels + `try_*` operations for backpressure.
- Explicit reset points for resampler, VAD, and engine caches between utterances.

## Key Files

- `src-tauri/src/coordinator/coordinator.rs`
- `src-tauri/src/audio/processor.rs`
- `src-tauri/src/inference/{engine,manager,burn_engine}.rs`
- `src-tauri/src/model/manager.rs`
- `src/routes/+page.svelte` (main UI + onboarding)

## Non-Goals (for v1)

- Full multi-model / multi-language selection in the engine.
- Advanced post-processing or speaker diarization.
- Virtualized rendering for extremely long transcripts (future perf work).

See the ADRs (especially 0001–0006) for detailed rationale on each layer.
