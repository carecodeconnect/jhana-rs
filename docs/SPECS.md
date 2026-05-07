# jhana-rs: Specifications

## Overview

jhana-rs is a Rust port of [jhana-dev](https://github.com/carecodeconnect/jhana-dev)
(Python) targeting the AI in a Box hardware (Radxa Rock 5A). The goal is a
voice-driven meditation guide that runs entirely on-device with no internet
connection.

**Scope**: STT -> Local LLM -> TTS pipeline. EEG features are excluded.

---

## 1. Target Hardware

| Spec | Value |
|------|-------|
| Board | Radxa Rock 5A |
| SoC | RK3588S |
| CPU | 4x Cortex-A55 (little) + 2x Cortex-A76 (big), up to 1.8 GHz |
| RAM | 8 GB |
| Storage | 32 GB eMMC; root partition 10.1 GB (expandable to ~29 GB) |
| GPU | Mali (not used for inference) |
| OS | Ubuntu 22.04.3 LTS, kernel 5.10.110-102-rockchip |
| Arch | aarch64 |
| Audio HW | Uctronics onboard mic + speaker; optional USB audio |
| Display | 720x1280 portrait |
| GPIO | Buttons on GPIO 63 (up), 43 (down), 139 (enter), 138 (back) |
| Serial | `/dev/ttyS6` at 115200 baud |
| Network | Ethernet only; no internet in normal operation |

### Storage Plan

The eMMC is 29.7 GB but only 10.4 GB is partitioned. Before development:
1. Expand root partition into unallocated space (`resize2fs` after `parted`)
2. Delete NLLB translation model (~1.2 GB) -- not needed for jhana-rs
3. This yields ~20+ GB free for Rust toolchain, builds, and new models

---

## 2. Phased Implementation

Each component has a **POC baseline** choice and an **upgrade** path. The POC
prioritises working software on the Rock 5A with proven dependencies. Upgrades
are evaluated once the baseline is stable.

### Phase 1: Text Pipeline (POC baseline)

stdin text prompt -> LLM streaming -> ratatui TUI display -> Piper WAV files.
No audio I/O. Validates the core loop.

| Component | POC Baseline | Upgrade |
|-----------|-------------|---------|
| **LLM** | Orca Mini 3B Q4_0 via **llama-cpp-rs** (already on device) | Qwen3-4B Q4_K_M (~2.5 GB) for better quality |
| **Display** | **ratatui** + crossterm (TUI in terminal) | Slint with `linuxkms-software` (graphical kiosk) |
| **TTS** | **Piper CLI** via `std::process::Command` (WAV to disk) | NeuTTS Air via neutts-rs (near-human quality) |

### Phase 2: Voice Input

Microphone -> VAD -> STT -> feeds into Phase 1 text pipeline.

| Component | POC Baseline | Upgrade |
|-----------|-------------|---------|
| **Audio capture** | **cpal** (ALSA backend, 16 kHz mono i16) | -- |
| **VAD** | **webrtc-vad** crate (simple, no ONNX dep) | Silero VAD via ort (ONNX Runtime) |
| **STT** | **whisper-rs** (whisper.cpp bindings, Whisper tiny.en) | whisper.apr (pure Rust) with Moonshine tiny |

### Phase 3: Voice Output

TTS audio playback through the speaker.

| Component | POC Baseline | Upgrade |
|-----------|-------------|---------|
| **Playback** | **rodio** (plays Piper WAV output) | -- |
| **TTS engine** | Piper CLI (carried from Phase 1) | NeuTTS Air Q4 GGUF (~500 MB); piper-rs bindings |
| **Feedback control** | Channel signal to mute mic during playback | -- |

### Phase 4: Hardware Integration (future)

| Component | Implementation |
|-----------|---------------|
| GPIO buttons | sysfs polling (`/sys/class/gpio/`) |
| Graphical display | Slint with `linuxkms-software` (DRM/KMS, no GPU) |
| Serial output | `/dev/ttyS6` at 115200 baud |
| Audio config | PulseAudio device setup (mirror `configure_devices.sh`) |
| Service | systemd unit (`jhana-rs.service`) |

---

## 3. Component Details

### 3.1 LLM

#### POC: Orca Mini 3B Q4_0 via llama-cpp-rs

The model is already on the device (`/home/ubuntu/ai_in_a_box/downloaded/
orca-mini-3b.ggmlv3.q4_0.bin`, 1.8 GB). This eliminates the need to download
anything for the POC.

- Context size: 2048 tokens
- Inference params: top_k=40, top_p=0.95, temp=0.25, repeat_penalty=1.1
- Streaming token output with sentence-level buffering
- Parse `[pause_duration]` markers from meditation prompts

```rust
enum LlmOutput {
    Sentence(String),
    Pause(f32),
    Done,
}
```

Prompt engineering with system prompt + few-shot examples replaces fine-tuning:

```
System: You are a meditation guide. Generate calming, guided meditation
instructions. Use [pause_duration] markers (e.g. [10]) to indicate silent
pauses. Speak in a warm, gentle tone. Keep sentences short for TTS.

Example:
"Close your eyes and take a deep breath in. [5] Now slowly exhale, releasing
any tension you feel. [3] Let your shoulders drop. [10]"
```

#### Upgrade: Qwen3-4B Q4_K_M

Best quality small model as of May 2026. Non-thinking mode for fast, fluent
generation. ~2.5 GB at Q4_K_M. Expected ~5-8 tok/s on RK3588 (faster than
spoken speech at ~2.5 words/second, so TTS pipeline keeps up).

Other candidates if Qwen3-4B is too slow:

| Model | Params | Q4_K_M Size | Expected tok/s | Notes |
|-------|--------|-------------|----------------|-------|
| Qwen3.5-2B | 2B | ~1.3 GB | ~8-12 | Newest gen; good quality for size |
| SmolLM3-3B | 3B | ~1.9 GB | ~5-8 | Fully open; strong benchmarks |
| Llama 3.2 3B | 3B | ~2.0 GB | ~5-8 | Best instruction following (IFEval 77.4) |
| Qwen3-1.7B | 1.7B | ~1.1 GB | ~8-12 | Lightest viable option |

### 3.2 STT

#### POC: whisper-rs (Whisper tiny.en)

Rust bindings to whisper.cpp. Hand-tuned ARM NEON SIMD -- proven fast on
aarch64. Whisper tiny.en model (~75 MB) is already proven on this hardware
in the AI in a Box Python setup.

#### Upgrade: whisper.apr with Moonshine tiny

Pure Rust (no C++ deps). Supports Moonshine tiny (27M params, ~27 MB) which
uses 5x less compute than Whisper tiny at comparable accuracy. Benchmarked at
237ms on RPi 5. Risk: SIMD optimised for AVX2/WASM, not ARM NEON -- may be
slower than whisper.cpp on this hardware until NEON support is added.

Also available: Moonshine standalone ONNX models via the `ort` crate, or FFI
to the C++ header-only core. Moonshine streaming models enable real-time
partial transcription.

### 3.3 TTS

#### POC: Piper CLI

Already installed and running on the Rock 5A. Model: `en_US-lessac-low`
(~61 MB ONNX). Shell out from Rust:

```bash
echo "sentence" | piper --model /path/to/model --output_file out.wav
```

Configurable speech rate via `--length_scale` (default 1.2).

#### Upgrade: NeuTTS Air

On-device TTS speech language model (Neuphonic, Apache 2.0). 748M params on
Qwen2 0.5B backbone + NeuCodec audio codec. Near-human quality with voice
cloning. Q4 GGUF ~500 MB, 400-600 MB RAM.

Rust integration via `neutts-rs` (community crate). Runs on llama.cpp for
the LM backbone + ONNX for the codec decoder. Claims RPi 5 compatibility
with RTF <0.5. Risk: community Rust port maturity; codec step latency
unbenchmarked.

### 3.4 Display

#### POC: ratatui TUI

Runs in any terminal. No X11, Wayland, or GPU driver needed.

```
┌─────────────────────────────────┐
│  jhana-rs                       │
├─────────────────────────────────┤
│                                 │
│  Close your eyes and take a     │
│  deep breath in.                │
│                                 │
│  [pause 5s]                     │
│                                 │
│  Now slowly exhale, releasing   │
│  any tension you feel.          │
│                                 │
├─────────────────────────────────┤
│  State: Generating  Model: Q3-4B│
│  Tokens: 47  Speed: 6.2 tok/s  │
└─────────────────────────────────┘
```

#### Upgrade: Slint (graphical kiosk)

Slint with `backend-linuxkms-software` renders directly to DRM/KMS dumb
buffers. No GPU driver needed. Built-in text layout, font rendering (including
CJK), and native display rotation (`SLINT_KMS_ROTATION=90`).

### 3.5 VAD

#### POC: webrtc-vad

Simple, lightweight, no ONNX dependency. Good enough for turn-based voice
detection.

#### Upgrade: Silero VAD via ort

ONNX model (~2 MB) with better accuracy. Requires building ONNX Runtime for
aarch64 (non-trivial but documented).

---

## 4. Project Structure

```
jhana-rs/
  Cargo.toml
  rustfmt.toml
  docs/
    SPECS.md              this file
  src/
    main.rs               orchestration + TUI event loop
    llm.rs                llama-cpp-rs wrapper, sentence buffering
    tts.rs                piper CLI wrapper (Phase 1); playback (Phase 3)
    stt.rs                whisper-rs wrapper (Phase 2)
    vad.rs                webrtc-vad wrapper (Phase 2)
    audio.rs              cpal capture + playback (Phase 2-3)
    ui.rs                 ratatui TUI layout and rendering
```

### Thread Model

Single process, multiple threads:

- **Main thread**: TUI event loop (ratatui) + LLM orchestration
- **LLM thread** (pinned to cores 4-7): token generation
- **Audio capture thread** (pinned to cores 0-3): mic input + VAD (Phase 2)
- **TTS/playback thread**: sentence queue -> Piper -> speaker (Phase 3)

Inter-thread communication via `crossbeam` channels.

---

## 5. Resource Budget

| Component | RAM (approx) |
|-----------|-------------|
| OS + system | 1.5 GB |
| LLM (Orca 3B Q4 / Qwen3-4B Q4) | 1.8-3.5 GB |
| Whisper tiny.en (Phase 2) | 75 MB |
| Piper TTS | 60 MB |
| VAD model | 2-10 MB |
| Rust binary + runtime | 50-100 MB |
| **Total (POC with Orca 3B)** | **~3.5 GB** |
| **Total (upgraded with Qwen3-4B)** | **~5.2 GB** |
| **Headroom** | **3-4.5 GB** |

---

## 6. Build Strategy

### POC: Native build on device

After expanding the root partition to use the full 29.7 GB eMMC:

```bash
curl --proto '=https' --tlsv1.2 -sSf https://sh.rustup.rs | sh
sudo apt install build-essential cmake pkg-config libasound2-dev
```

### Future: Cross-compile from x86_64

```bash
rustup target add aarch64-unknown-linux-gnu
sudo apt install gcc-aarch64-linux-gnu
cargo build --target aarch64-unknown-linux-gnu --release
```

Faster iteration, but C/C++ dependencies (llama.cpp, whisper.cpp) must also
be cross-compiled.

---

## 7. Risk Register

| Risk | Impact | Mitigation |
|------|--------|------------|
| llama-cpp-rs aarch64 build | Blocks LLM | candle crate; or build llama.cpp separately + FFI |
| whisper-rs aarch64 build | Blocks STT | whisper.cpp CLI subprocess; or whisper.apr |
| Disk space for toolchain | Blocks build | Expand root partition (19 GB unallocated on eMMC) |
| Qwen3-4B too slow | Poor UX | Drop to Qwen3.5-2B or Qwen3-1.7B |
| ort aarch64 build | Blocks Silero VAD | Use webrtc-vad (POC baseline) |
| neutts-rs immature | Blocks TTS upgrade | Stay on Piper CLI |
| whisper.apr NEON perf | Slow STT | Stay on whisper-rs |

---

## 8. Success Criteria

| Phase | Criterion |
|-------|-----------|
| **Phase 1** | Text prompt -> LLM streams meditation text -> ratatui displays sentences with pause markers -> Piper generates WAV files. Runs on Rock 5A. |
| **Phase 2** | Voice prompt via mic -> VAD -> STT -> feeds Phase 1 pipeline. |
| **Phase 3** | Spoken meditation guidance plays through speaker. |
| **All phases** | Peak memory under 6 GB. First displayed sentence within 3s of prompt. First spoken word within 5s of end of speech. |

---

## 9. References

### Source Projects

- `~/projects/ai_in_a_box/` -- working Python implementation on same hardware
- `~/projects/jhana-dev/src/` -- Python meditation guide (port source)
- `~/projects/rust_projects/r4ds/docs/` -- Rust crate references

### External

- [whisper.apr](https://github.com/paiml/whisper.apr) -- pure Rust Whisper + Moonshine
- [aprender](https://github.com/paiml/aprender) -- paiml Rust ML framework (Qwen GGUF)
- [Moonshine](https://github.com/usefulsensors/moonshine) -- efficient STT, ONNX models
- [NeuTTS Air](https://github.com/neuphonic/neutts) -- on-device TTS with voice cloning
- [neutts-rs](https://github.com/eugenehp/neutts-rs) -- Rust port of NeuTTS
- [Slint](https://slint.dev/) -- Rust GUI with embedded ARM + DRM/KMS support
