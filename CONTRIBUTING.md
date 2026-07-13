# Contributing to Voxly

Thank you for your interest in contributing to Voxly! We aim to build a production-grade, privacy-first desktop application with exceptionally high code quality.

## Code of Conduct

Be respectful, constructive, and inclusive. We follow standard open source community norms.

## Development Setup

See the [README.md](README.md) Quick Start section.

### Required Tooling

- `rustup` + stable Rust (clippy + rustfmt enabled)
- `pnpm` (preferred) or npm
- `cargo fmt` and `cargo clippy -- -D warnings` must pass before PR
- Frontend: TypeScript strict mode + Svelte checks

### Running Checks Locally

```bash
# Rust
cargo fmt -- --check
cargo clippy --all-targets -- -D warnings

# Frontend
pnpm check

# Full dev
pnpm tauri dev
```

## Git & Commit Standards

We use **Conventional Commits** strictly:

- `feat:` new feature
- `fix:` bug fix
- `refactor:`
- `docs:`
- `test:`
- `chore:`
- `perf:`
- `ci:`

Example: `feat(audio): implement lock-free VAD frame router`

- Keep commits focused and atomic.
- Write clear, imperative commit messages.
- After major milestones, commit + push.

## Architecture & Quality Standards

Voxly enforces the following non-negotiables (see ADR 0001):

- Clean modular architecture with clear boundaries
- Engine abstraction (easy to swap inference backends)
- `thiserror` + `anyhow` (or `eyre`) for errors
- `tracing` for structured logging (console + rotating file)
- Comprehensive tests (`#[cfg(test)]`, `tokio::test`)
- `clippy::pedantic` + rustfmt
- Strict TypeScript + frontend linting
- Tauri capabilities using least privilege
- Safety patterns: RAII guards, `catch_unwind`, atomic fast-paths for audio routing (inspired by proven designs but adapted)

**Do not** copy large verbatim blocks from other projects. Study, understand, and improve.

## Pull Requests

1. Create a feature branch from `main`
2. Make your changes + add/update tests + docs
3. Ensure all checks pass locally (`cargo fmt`, `clippy -D warnings`, `pnpm check`)
4. Update relevant ADRs or documentation
5. Open PR with a clear description referencing issues
6. Be responsive to review feedback

## Reporting Issues

- Use the GitHub issue templates
- Include reproduction steps, OS, Rust version, logs
- For security issues, please email privately first (see security policy when added)

## Areas Where Help is Welcome (Early Stage)

- Audio device enumeration & robust capture
- VAD tuning and dynamic trailing silence
- Burn + voxtral-mini-realtime-rs integration
- Model download UX (progress, resume, verification)
- UI/UX polish with Svelte 5 runes
- Cross-platform testing (especially Windows)

## Questions?

Open a discussion or issue. We prefer well-reasoned technical discussion.

Thank you for helping make private, local, high-quality speech technology accessible.
