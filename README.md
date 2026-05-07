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

## Building

Native build on the Rock 5A (after expanding the eMMC root partition):

```bash
curl --proto '=https' --tlsv1.2 -sSf https://sh.rustup.rs | sh
sudo apt install build-essential cmake pkg-config libasound2-dev
cargo build --release
```

## Tooling

- `rustfmt` for formatting
- `clippy` for linting (pedantic)
- Rust 2024 edition

## Specifications

See [docs/SPECS.md](docs/SPECS.md) for full technical specifications including
hardware details, library assessments, model selection, and risk register.

## License

MIT
