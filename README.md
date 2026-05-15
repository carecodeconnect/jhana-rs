# jhana-rs

A Rust port of [jhana-dev](https://github.com/carecodeconnect/jhana-dev) --
a voice-driven meditation guide running entirely on-device.

## Target Hardware

[Radxa Rock 5A](https://radxa.com/products/rock5/5a/) (RK3588S, 8 GB RAM,
aarch64). No internet required.

## Architecture

```
[Mic] -> [VAD] -> [STT] -> [LLM] -> [TUI Display]
                                 \-> [TTS] -> [Speaker]
```

| Stage | Current | NPU Upgrade |
|-------|---------|-------------|
| **LLM** | mistral.rs + Ministral 3B (CPU, 3.89 tok/s) | rkllm-rs + Llama-3.2-3B on NPU |
| **STT** | sensevoice-rs + SenseVoice (RKNN NPU) | — (already on NPU) |
| **TTS** | Piper CLI (CPU, RTF ~0.31) | qwen_tts voice cloning / piper-rknn-rs on NPU |
| **Display** | ratatui TUI | Slint graphical kiosk |
| **VAD** | sensevoice-rs built-in FSMN-VAD | — (already on NPU) |

## Phases

1. **Text pipeline** -- stdin prompt -> LLM streaming -> ratatui TUI + Piper WAV files
2. **Voice input** -- microphone -> VAD -> STT -> text pipeline
3. **Voice output** -- TTS audio playback through speaker
4. **Hardware integration** -- GPIO buttons, graphical display, systemd service

## Prerequisites

### Rock 5A (build target)

OS: Armbian 26.2.1, Ubuntu 24.04 LTS Noble, kernel 6.1.115-vendor-rk35xx
RKNPU driver: v0.9.8

OS-level packages:
```bash
sudo apt install build-essential cmake pkg-config libasound2-dev libssl-dev rsync console-setup libclang-dev ffmpeg
```

Rust toolchain:
```bash
curl --proto '=https' --tlsv1.2 -sSf https://sh.rustup.rs | sh
```

### X61s (dev machine)

OS-level packages:
```bash
sudo apt install dnsmasq sshpass rsync
```

Rust toolchain (for `cargo check`, `cargo clippy`, `cargo fmt` only -- no
builds on x61s):
```bash
curl --proto '=https' --tlsv1.2 -sSf https://sh.rustup.rs | sh
```

### NPU acceleration

The RK3588S has a 6 TOPS NPU with two runtime libraries:

**librknnrt.so** (RKNN — for STT, VAD, TTS, vision):
```bash
wget -O /tmp/librknnrt.so \
  "https://github.com/airockchip/rknn-toolkit2/raw/v2.2.0/rknpu2/runtime/Linux/librknn_api/aarch64/librknnrt.so"
sudo cp /tmp/librknnrt.so /usr/lib/librknnrt.so
sudo ldconfig

# C headers (for sherpa-onnx RKNN build)
for h in rknn_api.h rknn_matmul_api.h rknn_custom_op.h; do
  wget -O /tmp/$h \
    "https://github.com/airockchip/rknn-toolkit2/raw/v2.2.0/rknpu2/runtime/Linux/librknn_api/include/$h"
done
sudo cp /tmp/rknn_*.h /usr/include/
```

**librkllmrt.so** (RKLLM — for LLM inference):
```bash
wget -O /tmp/librkllmrt.so \
  "https://raw.githubusercontent.com/airockchip/rknn-llm/release-v1.2.3/rkllm-runtime/Linux/librkllm_api/aarch64/librkllmrt.so"
sudo cp /tmp/librkllmrt.so /usr/lib/librkllmrt.so
sudo ldconfig
```

See [docs/05_NPU.md](docs/05_NPU.md) for full NPU setup, model downloads,
and build steps.

### Package summary

| Package | Where | Purpose |
|---------|-------|---------|
| `build-essential` | Rock | C/C++ compiler for native deps (llama.cpp, whisper.cpp) |
| `cmake` | Rock | Build system for C/C++ deps |
| `pkg-config` | Rock | Library discovery |
| `libasound2-dev` | Rock | ALSA audio headers (cpal) |
| `rsync` | Both | Incremental file sync between x61s and Rock |
| `console-setup` | Rock | Console font configuration for 720x1280 display |
| `libssl-dev` | Rock | OpenSSL headers (hf-hub model downloads in sensevoice-rs) |
| `libclang-dev` | Rock | bindgen FFI generation (sensevoice-rs, rkllm-rs) |
| `ffmpeg` | Rock | Demo recording (framebuffer + audio capture) |
| `dnsmasq` | X61s | DHCP server for direct ethernet link to Rock |
| `sshpass` | X61s | Non-interactive SSH password for scripts |
| `librknnrt.so` | Rock | RKNN NPU runtime (v2.2.0, from rknn-toolkit2) |
| `rknn_api.h` | Rock | RKNN C headers for sherpa-onnx RKNN build |

## Building

All builds happen on the Rock 5A (the x61s is too slow and wrong arch).

The `+fp16` target feature is **required** — the RK3588S supports FP16 but
Rust's default aarch64 target doesn't enable it. Without this flag, the
`gemm-f16` crate (Candle/sensevoice-rs dependency) fails to compile.

```bash
RUSTFLAGS="-C target-feature=+fp16" cargo build
RUSTFLAGS="-C target-feature=+fp16" cargo test
cargo doc --no-deps
```

Or set it permanently on the Rock in `~/.cargo/config.toml`:
```toml
[target.aarch64-unknown-linux-gnu]
rustflags = ["-C", "target-feature=+fp16"]
```

## Tooling

- `rustfmt` for formatting
- `clippy` for linting (pedantic, warnings as errors)
- Rust 2024 edition
- Pre-commit hooks: `cp scripts/pre-commit .git/hooks/pre-commit`

## Specifications

See [docs/10_SPECS.md](docs/10_SPECS.md) for full technical specifications including
hardware details, library assessments, model selection, and risk register.

## License

MIT
