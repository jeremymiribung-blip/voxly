# ADR 0006: TranscriptionCoordinator and Session Lifecycle

**Status:** Accepted  
**Date:** 2026-07-14  
**Deciders:** Project team  
**Related:** ADR 0001, ADR 0002, ADR 0003

## Context

All the pieces (audio pipeline, engine, model manager, events) need a single place that owns the *session* concept: when recording starts/stops, how chunks flow to inference, how tentative vs committed text is decided, when to reset context, and how to emit UI state safely.

We wanted to avoid scattering lifecycle logic across commands, lib.rs, and individual managers.

Handy used a dedicated thread + mpsc + `catch_unwind` for exactly this reason (serialization + safety).

## Decision

### Central Coordinator
- `TranscriptionCoordinator` (or `SessionManager`) is the single source of truth for session state (`Idle` / `Listening` / `Processing`).
- Exposes a `tokio::sync::mpsc::Sender<CoordinatorCommand>` so it can be called from sync contexts (hotkeys, commands) and async.
- Main work runs in a dedicated Tokio task using `select!` / polling + bounded channels for backpressure.

### Responsibilities
- Starting/stopping `AudioCapture` + `AudioProcessor`.
- Consuming `AudioChunk`s and feeding them into `EngineManager` / `BurnVoxtralEngine`.
- Implementing boundary logic for tentative vs committed (VAD `is_final`, punctuation, stability).
- Emitting rich Tauri events (`transcription-update`, `session-status`, etc.).
- Metrics collection (RTF, audio duration, etc.).
- Graceful shutdown, cancel, and long-session `ResetContext`.

### Concurrency Model
- Audio pipeline produces via `tokio_mpsc`.
- Coordinator consumes in its task.
- Heavy engine work is either direct (stateful engine) or via the existing `EngineManager` streaming worker + guards.
- Commands are fire-and-forget or use oneshot replies where a result is needed.

### Safety
- RAII-style drops of capture/processor handles stop streams.
- Engine lease / worker guards (from earlier design) protect against panics in inference.
- `try_send` / bounded channels to prevent blocking producers.
- Optional `catch_unwind` wrappers around critical sections (inspired by Handy).

### Tentative / Committed Logic
- Engine may return both in `TranscriptionUpdate`.
- Coordinator (or a thin layer above) decides when to promote tentative → committed:
  - VAD silence boundary (`is_final` on chunk).
  - Explicit finalize command.
  - Future: punctuation, repeated stable prefix, confidence thresholds.

## Consequences

**Positive:**
- Clean ownership: audio and engine don't need to know about "sessions".
- Easy to add features like pause, context reset, metrics without touching every layer.
- Testable command → state transitions (with mocks for audio/engine).

**Trade-offs:**
- One more central component to reason about.
- Must be careful not to do heavy work directly on the coordinator task (offload to engine workers or spawn_blocking).

**Future:**
- Extract a small state machine trait or use an actor framework if complexity grows.
- Expose more commands (change VAD policy live, set delay, etc.).

## References
- Handy `TranscriptionCoordinator`
- Current `TranscriptionCoordinator` implementation and its evolution from std thread to tokio task
- EngineManager streaming design (StreamRouter + lease guards)
