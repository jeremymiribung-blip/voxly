# ADR 0001: Initial Architecture & Technology Decisions

**Status:** Accepted  
**Date:** 2026-07-13  
**Deciders:** Project lead (Grok-assisted engineering)  
**Context:** Bootstrapping Voxly — a fully local, privacy-first realtime speech-to-text desktop application.

## Context

We are building a production-grade cross-platform (macOS/Windows primary) desktop app using Tauri v2. The core requirement is realtime transcription using `mistralai/Voxtral-Mini-4B-Realtime-2602` (Voxtral Mini 4B Realtime) with aggressive latency targets (< 500-600 ms end-to-end).

The project must meet extremely high non-negotiable standards around code quality, documentation, testing, security, and maintainability from day one.

A reference codebase (`/mnt/DATA/PROJECTS/handy/Handy`) was studied for proven patterns in transcription coordination, audio streaming, VAD, engine loading, and safety.

## Decision

### 1. Core Technology Stack (Locked)

- **Desktop shell**: Tauri v2 (Rust backend + web frontend)
- **Frontend**: Svelte 5 (runes) + Vite + strict TypeScript
  - SvelteKit with static adapter is acceptable (used by the official template) because it provides excellent DX and produces a static bundle suitable for Tauri.
- **Async runtime**: Tokio (mpsc + broadcast channels)
- **Primary inference**: Burn ML framework + `voxtral-mini-realtime-rs` (Q4 GGUF weights)
- **Audio stack**: `cpal`-based (via `stream-audio` or `decibri` where appropriate) + `rubato` (resampling) + `wavekat-vad` (Silero / TEN VAD preferred)
- **Error handling**: `thiserror` for domain errors + `anyhow` (or `eyre`) for application glue
- **Logging**: `tracing` + `tracing-subscriber` (env-filter, fmt, optional json/file)
- **Model management**: On-demand Hugging Face downloads (progress, resume, integrity, versioning) cached in platform app data directory
- **State management**: Primarily Svelte 5 runes + stores. TanStack Query only if data-fetching complexity justifies it later.

### 2. Architectural Patterns (Adapted & Improved)

We deliberately studied the Handy codebase and will **adapt** (never blindly copy) the following patterns:

- **TranscriptionCoordinator** (or equivalent) — single-threaded command processor using `mpsc` to serialize all lifecycle events (recording start/stop, PTT logic, debounce, deferred release). Wrapped in `catch_unwind` + `AssertUnwindSafe` for resilience.
- **LoadedEngine** enum + dispatch — type-safe abstraction over different inference backends.
- **StreamRouter** — fast-path audio frame routing using `Arc<AtomicBool>` for zero-cost "is stream active" check before taking a `Mutex`. Commands sent via channel to a dedicated worker thread.
- **RAII Safety Guards** (`LoadingGuard`, `StreamWorkerGuard`, etc.) using atomics for worker IDs + lease tracking so that panics/unwinds cleanly release state. Poison recovery on `Mutex`.
- **VAD abstraction** — `VoiceActivityDetector` trait returning `Speech` / `Noise` frames. Support for `set_hangover_frames` (dynamic trailing silence). Separate constants for offline vs streaming hangover. Silero via appropriate crate.
- **Capability probing** + `EngineType` / `ModelSource` — pre-download inspection of model capabilities (especially important for GGUF headers).
- **Engine lease model** — allow a streaming worker to take ownership of the loaded model out of the shared mutex while still correctly reporting "model loaded" via atomic lease counters.

These patterns will be **adapted and improved** for:
- Burn + the specific Voxtral realtime crate
- Our chosen audio crate(s)
- Tokio channels instead of std mpsc where beneficial
- Stronger use of `tracing` spans

### 3. Rust Workspace Structure

- Root `Cargo.toml` as a Cargo workspace from day one.
- `src-tauri/` remains the Tauri application crate (with `voxly_lib`).
- `crates/` directory prepared for future extraction of reusable logic:
  - `crates/engine-abstraction`
  - `crates/audio`
  - `crates/vad`
  - etc.
- This enables clean separation and easier testing of core logic independent of Tauri.

### 4. Quality & Process Non-Negotiables

- Conventional commits from the first change.
- GitHub repository created with `gh`.
- Full documentation suite: README, CONTRIBUTING, LICENSE (Apache-2.0), ADRs, architecture diagrams (Mermaid).
- High test coverage using `#[cfg(test)]` and `tokio::test`.
- Strict linting: `clippy::pedantic` + `rustfmt`. Frontend equivalent (Biome or ESLint+Prettier + strict TS).
- Tauri security model: start with minimal capabilities and only expand explicitly.
- Correctness before micro-optimizations. Profile only after the pipeline works.

### 5. Latency Target

Target **< 500–600 ms** end-to-end on good hardware, explicitly accounting for the model's inherent ~480 ms algorithmic delay. This will drive buffer sizes, VAD policy, chunking strategy, and streaming usage of the Voxtral realtime model.

## Consequences

### Positive
- Strong foundation for long-term maintainability and extensibility (new engines, new platforms).
- Safety and correctness prioritized in the audio + inference hot path.
- Easy onboarding thanks to excellent docs and conventional structure.
- Future-proofing via workspace + engine abstraction.

### Negative / Trade-offs
- More upfront ceremony (workspace, ADRs, comprehensive gitignore, etc.) than a quick prototype.
- SvelteKit was accepted from the template even though a pure Vite+Svelte setup was mentioned; the DX and static output benefits outweigh the minor deviation.
- We must invest time early in proper audio + VAD abstractions instead of hacking a quick demo.

### Neutral / Future Work
- Exact choice between `stream-audio` vs `decibri` (or direct cpal + custom) will be made after prototyping and will be recorded in a follow-up ADR.
- Burn integration details and the `voxtral-mini-realtime-rs` crate API surface will be evaluated in the next phase.
- Full `clippy::pedantic` may require selective allows; these will be documented.

## References

- Original project requirements and non-negotiables (initial user prompt)
- Study of Handy transcription coordinator, managers, audio toolkit, and model capability code
- Tauri v2 documentation
- Burn + Voxtral Mini Realtime model card and related Rust crates

---

This ADR is the source of truth for core decisions. Any deviation requires a new ADR with strong justification.
