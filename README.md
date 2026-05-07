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

| Stage | POC Baseline | Upgrade Path |
|-------|-------------|-------------|
| **LLM** | Orca Mini 3B Q4 via llama-cpp-rs | Qwen3-4B Q4_K_M |
| **STT** | Whisper tiny.en via whisper-rs | Moonshine tiny via whisper.apr |
| **TTS** | Piper CLI (text first, voice later) | NeuTTS Air via neutts-rs |
| **Display** | ratatui TUI | Slint graphical kiosk |
| **VAD** | webrtc-vad | Silero VAD via ort |

## Phases

1. **Text pipeline** -- stdin prompt -> LLM streaming -> ratatui TUI + Piper WAV files
2. **Voice input** -- microphone -> VAD -> STT -> text pipeline
3. **Voice output** -- TTS audio playback through speaker
4. **Hardware integration** -- GPIO buttons, graphical display, systemd service

## Prerequisites

### Rock 5A (build target)

OS-level packages:
```bash
sudo apt install build-essential cmake pkg-config libasound2-dev rsync console-setup libclang-dev protobuf-compiler
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

### Package summary

| Package | Where | Purpose |
|---------|-------|---------|
| `build-essential` | Rock | C/C++ compiler for native deps (llama.cpp, whisper.cpp) |
| `cmake` | Rock | Build system for C/C++ deps |
| `pkg-config` | Rock | Library discovery |
| `libasound2-dev` | Rock | ALSA audio headers (cpal) |
| `rsync` | Both | Incremental file sync between x61s and Rock |
| `console-setup` | Rock | Console font configuration for 720x1280 display |
| `libclang-dev` | Rock | bindgen FFI generation for llama-cpp-2 (llama.cpp) |
| `protobuf-compiler` | Rock | protobuf compilation for llama-gguf (ONNX proto) |
| `dnsmasq` | X61s | DHCP server for direct ethernet link to Rock |
| `sshpass` | X61s | Non-interactive SSH password for scripts |

## Building

All builds happen on the Rock 5A (the x61s is too slow and wrong arch):

```bash
cargo check    # verify before building
cargo build
cargo test
cargo doc --no-deps
```

## Tooling

- `rustfmt` for formatting
- `clippy` for linting (pedantic, warnings as errors)
- Rust 2024 edition
- Pre-commit hooks: `cp scripts/pre-commit .git/hooks/pre-commit`

## Specifications

See [docs/SPECS.md](docs/SPECS.md) for full technical specifications including
hardware details, library assessments, model selection, and risk register.

## License

MIT
