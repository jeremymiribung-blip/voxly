# ADR 0004: Model Management, Downloads, and Caching

**Status:** Accepted  
**Date:** 2026-07-14  
**Deciders:** Project team  
**Related:** ADR 0001, ADR 0002

## Context

Voxly must deliver a fully local experience. The primary model (Voxtral Mini 4B Realtime, Q4 GGUF) is ~2.5 GB. Users must be able to:

- Download on first launch or when missing.
- Resume interrupted downloads.
- Pause/resume for UX.
- See progress, speed, ETA.
- Choose quantization levels (Q4 default; higher if hardware allows).
- Manage storage (delete, see size).
- Have reliable cache with basic versioning/validation.

Models must live in the platform app data directory for proper packaging/sandboxing.

Previous implementation used `hf-hub` directly with limited progress and no explicit resume control.

## Decision

### Storage Location
- Use `tauri::Manager::path().app_data_dir().join("models")`.
- Per-quant sub-organization or filename encoding (e.g., `voxtral-q4.gguf`).
- Sidecar metadata (size, etag/revision, downloaded_at) for validation.

### Download Implementation
- Primary: Custom resumable downloader using `reqwest` + HTTP `Range` headers (full control).
- Support for partial files: on resume, issue `Range: bytes={current_size}-`.
- HEAD request to obtain `Content-Length` for progress/ETA.
- Throttled progress events including `speed_bps` and `eta_seconds`.
- Control via atomic flags (`download_paused`, `download_cancelled`) checked inside the chunk loop.
- Fallback / future: `hf-hub` for non-GGUF or when its resume semantics improve.

### Model Selection & Quantization
- `PrimaryModel` struct extended with `quant`, `repo_id`, `filename`.
- Initial support for Q4 GGUF from `TrevorJS/voxtral-mini-realtime-gguf`.
- Future: registry of available quants + UI selector. Loading path chosen at runtime.
- EngineManager receives the concrete path after download succeeds.

### Loading & Lifecycle
- `ensure_primary_model()` returns the path only after a complete (or already-complete) download.
- Engine load (`EngineManager::load_burn_voxtral`) happens **only after** download completes.
- `is_primary_model_downloaded()` does size > threshold check + optional future hash validation.
- Delete model, get size exposed via commands for settings UI.

### First-Run / Onboarding
- Background ensure on launch.
- Frontend detects `!modelReady` via `is_model_downloaded` + events and shows dedicated download screen.
- Progress, pause/resume/cancel wired through Tauri events + commands.
- Once ready, engine is loaded and main UI activates.

### Versioning & Validation
- Filename + revision in the model key.
- Optional etag or size in sidecar metadata.
- On startup or before load: basic existence + size sanity check.
- Cache invalidation on quant change or explicit delete.

### Error Handling
- Network, partial write, low disk space, cancellation all map to `VoxlyError::ModelDownload` / `Storage`.
- User-friendly messages surfaced via events/commands.
- Graceful degradation: app still launches; user is guided to download.

## Consequences

**Positive:**
- Excellent UX for large downloads (resume is critical for 2.5 GB files).
- Clear separation: ModelManager owns files/download; EngineManager owns loaded inference state.
- Storage hygiene and user control.
- Ready for multiple quants and future model versions.

**Trade-offs:**
- Custom downloader code vs relying entirely on `hf-hub`.
- Must handle platform differences in app data paths and permissions.
- Large files mean careful memory use during streaming copy.

**Open Items:**
- SHA256 / integrity verification of downloaded GGUF.
- Background vs foreground download policy.
- Automatic updates / delta downloads for future model versions.

## References
- HF Hub direct resolve URLs + Range header support
- Previous ModelManager implementation and its limitations
- Tauri app data path best practices
- TrevorJS quantized GGUF releases
