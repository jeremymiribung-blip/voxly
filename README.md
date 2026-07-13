# Voxly

> **Fully local, privacy-first, cross-platform realtime speech-to-text desktop application.**

[![License](https://img.shields.io/badge/license-Apache--2.0-blue.svg)](LICENSE)
[![Tauri](https://img.shields.io/badge/Tauri-v2-24c8db?logo=tauri)](https://tauri.app)
[![Svelte](https://img.shields.io/badge/Svelte-5-ff3e00?logo=svelte)](https://svelte.dev)
[![Rust](https://img.shields.io/badge/Rust-1.80+-orange?logo=rust)](https://www.rust-lang.org)
[![Build](https://img.shields.io/badge/build-passing-brightgreen)]()

Voxly brings **Voxtral Mini 4B Realtime** (Mistral AI) to your desktop with sub-second latency, running 100% locally using the Burn ML framework. No cloud, no data leaving your machine.

## ✨ Key Features (Planned)

- **Realtime STT** — Low-latency streaming transcription (< 600 ms target end-to-end)
- **Fully Local & Private** — Models run on-device (Q4 GGUF via Burn + voxtral-mini-realtime-rs)
- **Cross-platform** — Primary targets: macOS & Windows (Linux supported)
- **Engine Abstraction** — Clean separation so additional backends (CrispASR, etc.) can be added easily
- **Professional UX** — Clean Svelte 5 UI with runes, push-to-talk / always-on modes, visual feedback
- **Model Management** — On-demand download from Hugging Face with progress, resume, and local cache

## 🛠 Tech Stack

| Layer              | Technology                                      |
|--------------------|-------------------------------------------------|
| Desktop Framework  | Tauri v2                                        |
| Backend            | Rust + Tokio                                    |
| Inference          | Burn + TrevorS/voxtral-mini-realtime-rs (Q4)    |
| Audio Pipeline     | cpal / decibri / stream-audio + rubato + wavekat-vad (Silero preferred) |
| Frontend           | Svelte 5 (runes) + Vite + TypeScript (strict)  |
| State & Reactivity | Svelte runes + stores                           |
| Error Handling     | `thiserror` + `anyhow`                          |
| Logging            | `tracing` + `tracing-subscriber`                |
| Model Downloads    | Hugging Face (hf-hub style with progress)       |

## 🚀 Quick Start

### Prerequisites

- Rust (latest stable)
- Node.js 20+ + pnpm (recommended) or npm
- For macOS: Xcode command line tools
- For Windows: Visual Studio Build Tools + WebView2

### Development

```bash
# Clone
git clone https://github.com/jeremymiribung-blip/voxly.git
cd voxly

# Install frontend deps
pnpm install

# Run in development (hot reload)
pnpm tauri dev
```

### Build

```bash
pnpm tauri build
```

The production binary will be in `src-tauri/target/release/bundle/`.

## 📁 Project Structure

```
voxly/
├── src/                  # Svelte 5 frontend (SvelteKit + Vite)
├── src-tauri/            # Rust Tauri backend
│   ├── src/
│   │   ├── main.rs
│   │   └── lib.rs
│   ├── Cargo.toml
│   └── tauri.conf.json
├── crates/               # Future shared Rust crates (engine abstraction, audio, VAD...)
├── docs/
│   └── adr/              # Architecture Decision Records
├── .github/              # CI, issue templates, etc.
├── Cargo.toml            # Workspace root
└── package.json
```

See `docs/adr/0001-initial-architecture.md` for the core decisions and rationale.

## 🧠 Architecture Highlights

- **Engine Abstraction Layer** — `Engine` trait + concrete implementations (starting with Voxtral Realtime). Inspired by proven patterns (e.g. coordinator + router + RAII guards).
- **Audio Pipeline** — Lock-free(ish) streaming via atomics + channels. VAD with dynamic trailing silence.
- **Safety** — `Arc<Mutex<...>>` + `catch_unwind` + RAII guards around long-running workers.
- **Model Lifecycle** — Download, verify, cache, lazy load/unload, versioned.
- **Frontend** — Svelte 5 runes for fine-grained reactivity. Minimal global state.

Full architecture diagrams and component breakdown will live in the README and `/docs` as the project matures.

## 🛡️ Security & Privacy

- Least-privilege Tauri capabilities (only what is needed)
- No network calls except explicit model downloads (user-initiated)
- All transcription happens locally
- No telemetry by default

## 📜 License

Licensed under the [Apache License 2.0](LICENSE).

## 🤝 Contributing

See [CONTRIBUTING.md](CONTRIBUTING.md).

## 📝 Status

**Early initialization.** Core scaffolding complete. Next milestones will include:

- Audio capture + VAD pipeline
- Engine abstraction + first Voxtral integration
- Basic realtime transcription loop
- Model manager with HF downloads
- Polished UI

Watch this space.

---

Built with ❤️ for privacy and performance.
