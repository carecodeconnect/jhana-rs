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

### Agent harness — pi attempt (historical)

> Pi was evaluated as the agent harness and **pivoted away from** on
> 2026-05-15 in favour of a bespoke Rust agent loop. See
> [docs/10_SPECS.md § Outcome](docs/10_SPECS.md#outcome-2026-05-15-pivoted-to-bespoke-rust-agent-loop)
> for the reasoning (built-in coding tools we couldn't disable; TUI
> mismatch; content-shape quirks). The install instructions below are
> preserved so anyone reviewing that decision can reproduce the setup,
> and because the `jhana-llm-server` HTTP shim is still useful for
> external clients. Skip this section for normal use.

Node 20 + npm via NodeSource (the apt-shipped `nodejs` is too old for pi):

```bash
curl -fsSL https://deb.nodesource.com/setup_20.x | sudo -E bash -
sudo apt-get install -y nodejs
node --version    # → v20.x
npm --version     # → 10.x
```

Pi via npm with a **user-writable global prefix** (avoids the `/usr/lib/node_modules`
EACCES that pi's curl-installer trips into, and keeps `sudo` out of global npm):

```bash
mkdir -p ~/.npm-global
npm config set prefix ~/.npm-global
grep -q '.npm-global/bin' ~/.bashrc || echo 'export PATH=$HOME/.npm-global/bin:$PATH' >> ~/.bashrc
npm install -g @earendil-works/pi-coding-agent
~/.npm-global/bin/pi --version
```

After this, `pi` is on `$PATH` in new shells. The pi.dev curl-installer (`curl
-fsSL https://pi.dev/install.sh | sh`) does the same thing but defaults to
`sudo npm install -g`, which works but taints `/usr/lib/node_modules/` with
root permissions and creates EACCES issues for every subsequent global npm
install. The user-prefix approach above is what `pi_sandbox/docs/06-troubleshooting.md`
recommends.

See [docs/15_INTERACTION.md](docs/15_INTERACTION.md) for the agent harness
design and tool catalog, and [docs/10_SPECS.md § pi as agent harness](docs/10_SPECS.md)
for why pi vs alternatives.

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
| `nodejs` (NodeSource 20.x) | Rock | Runtime for the `pi` agent harness — **pi-port branch only** |
| `@earendil-works/pi-coding-agent` | Rock (npm global, user prefix) | The `pi` CLI itself — **pi-port branch only** |

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
