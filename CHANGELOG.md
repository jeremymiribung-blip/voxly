# Changelog

All notable changes to this project will be documented in this file.

The format is based on [Keep a Changelog](https://keepachangelog.com/en/1.0.0/),
and this project adheres to [Semantic Versioning](https://semver.org/spec/v2.0.0.html).

## [0.1.0] - 2026-07-14

### Added
- Initial Tauri v2 + Svelte 5 application skeleton
- Audio pipeline: cpal capture, rubato resampling, wavekat-vad with dynamic policy
- Engine abstraction with Burn + voxtral-mini-realtime-rs (Q4 GGUF) integration (feature-gated)
- Stateful streaming with KV cache management
- Resumable Hugging Face model downloads with progress, pause/resume
- TranscriptionCoordinator with tentative/committed text logic
- Svelte 5 runes UI with live updates, onboarding, settings
- Comprehensive error handling and safety patterns (inspired by Handy)
- Expanded test coverage and simple benchmarking harness
- CI workflows for Linux/macOS/Windows
- Multiple Architecture Decision Records (ADRs)

### Notes
- First public release. Core realtime STT loop is functional.
- Model download (~2.5 GB Q4) is required on first use.
- GPU acceleration recommended for best performance.
