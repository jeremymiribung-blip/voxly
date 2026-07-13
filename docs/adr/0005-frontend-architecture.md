# ADR 0005: Frontend Architecture (Svelte 5)

**Status:** Accepted  
**Date:** 2026-07-14  
**Deciders:** Project team  
**Related:** ADR 0001

## Context

The desktop UI must feel instant and responsive while handling high-frequency updates from the realtime inference engine (tentative tokens arriving every few tens of ms).

Requirements:
- Clear distinction between tentative (provisional) and committed text.
- Smooth auto-scroll with user control.
- Low-jank updates even on long transcripts.
- Settings, device selection, download progress, status.
- Keyboard + global hotkey support.
- SPA mode (no SSR) because of Tauri.

We used the official `create-tauri-app` Svelte + TS + Vite template, which produced a SvelteKit + adapter-static setup.

## Decision

### Framework Choice
- **Svelte 5 with runes** (`$state`, `$derived`, `$effect`) for fine-grained reactivity.
- SvelteKit + `@sveltejs/adapter-static` (fallback: `index.html`) — standard and recommended for Tauri desktop apps. Gives us routing/file structure for free while producing a pure static bundle.

### Reactivity & Performance
- Main transcription state (`committed`, `tentative`) as top-level `$state`.
- Display composition via `$derived` (or direct template) to avoid unnecessary work.
- Event listeners (`listen` from `@tauri-apps/api/event`) update only the relevant runes.
- Scroll behavior uses `requestAnimationFrame` + conditional logic instead of constant DOM writes.
- Avoided heavy frameworks or large component libraries; used Tailwind (v3) for styling.

### Key UI Patterns
- Large dedicated transcription pane (monospace for readability).
- Tentative text rendered with distinct classes (`italic`, lower opacity, muted color).
- Simple inline editing for committed segments (textarea on demand).
- Prominent controls + settings drawer.
- Conditional onboarding/download screen driven by `modelReady` state (populated via `is_model_downloaded` + events).
- Event-driven updates: `transcription-update`, `session-status`, `model-download-progress`, etc.

### Tauri Integration
- Commands via `invoke` (typed wrappers where possible).
- Events via `listen<T>` with matching TypeScript interfaces.
- Global shortcuts registered on the Rust side; frontend provides local keyboard fallbacks and hints.
- All cross-boundary data is versioned via the event/command payloads.

### Styling
- Tailwind CSS for rapid, consistent, accessible design.
- Dark-first theme suitable for long listening sessions.
- Minimal custom CSS focused on the transcription area and scroll behavior.

## Consequences

**Positive:**
- Extremely responsive updates — only changed text nodes re-render thanks to runes.
- Easy to keep the "always-focused" transcription experience.
- Onboarding flow integrates cleanly with the model download events.
- Type safety across the Tauri boundary reduces runtime surprises.

**Trade-offs:**
- SvelteKit adds a small amount of boilerplate vs pure Vite+Svelte for a pure SPA.
- Must be careful with large strings in Svelte; current approach (two main strings + spans) works well but may evolve to segment arrays for advanced editing.
- No built-in virtual scrolling yet (future optimization if transcripts become extremely long).

**Future Considerations:**
- Virtualized list for very long sessions.
- Richer editing (per-segment, undo, speaker labels).
- Theming / accessibility improvements (high contrast, reduced motion).
- Possible migration to a small component lib (e.g. shadcn-svelte) if complexity grows.

## References
- Svelte 5 runes documentation
- Tauri + SvelteKit official guide
- Previous frontend implementation in this project
- Whispering and similar realtime Tauri STT UIs (patterns for live text + controls)
