# Performance Notes

## Targets

- End-to-end latency: < 600 ms on good hardware (model has ~480 ms inherent delay).
- RTF (Real-Time Factor) < 1.0 for comfortable real-time use.

## Current Implementation

- Audio capture runs on a dedicated thread.
- Resampling + VAD + chunking runs on a worker thread (not the main Tokio runtime).
- Inference uses Burn with wgpu backend (GPU preferred) or falls back.
- Stateful KV cache carry-over in the Voxtral engine for low-latency streaming.

## Benchmarking

A simple benchmarking harness exists in the test suite:

```bash
cargo test -p voxly --lib -- --nocapture audio::processor::tests::benchmark_simple_rtf_and_wer
```

It measures VAD + chunking RTF on synthetic audio and includes a stub WER calculation.

For full end-to-end:

- Use real audio files + reference transcripts.
- Profile with `cargo flamegraph` or `perf` / Instruments / Windows Performance Analyzer.
- Monitor VRAM with `nvidia-smi`, `asitop` (Apple), or equivalent.

## Profiling Tips

```bash
# Install cargo tools
cargo install flamegraph

# Record
cargo flamegraph --bin voxly -- --features burn-voxtral  # when running with real model
```

## Hardware Recommendations

- **Minimum**: Modern multi-core CPU, 8 GB RAM, integrated graphics.
- **Recommended for good experience**: Dedicated GPU (Apple Silicon, NVIDIA/AMD with Vulkan/Metal), 16 GB+ RAM.
- Q4 model ~2.5 GB on disk, significantly less in RAM when using quantized inference.

See the benchmarking code in `src-tauri/src/audio/processor.rs` and integration points in the coordinator for current measurement points.
