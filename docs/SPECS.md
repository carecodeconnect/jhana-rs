# jhana-rs: Specifications

## Overview

jhana-rs is a Rust port of [jhana-dev](https://github.com/carecodeconnect/jhana-dev)
(Python) targeting the AI in a Box hardware (Radxa Rock 5A). The goal is a
voice-driven meditation guide that runs entirely on-device with no internet
connection.

**Scope**: STT -> Local LLM -> TTS pipeline. EEG features are excluded.

---

## Aim (long-term vision, 2026-05-15)

jhana-rs is a self-contained Rust backend on the Rock 5A that delivers
**guided, personalised Jhana meditations** with no cloud dependency:

- **VAD / STT / TTS** in pure Rust on CPU (sensevoice-rs for STT today;
  webrtc-vad or Silero on the input side; Piper for TTS now, NeuTTS Air
  or `piper-rknn-rs` next).
- **LLM inference on the RK3588 NPU** (currently Llama 3.2 3B via
  rkllm-rs; soon a meditation-tuned model in the same `.rkllm` format).
  The NPU is dedicated to the LLM so the A76 cores stay free for the
  audio pipeline.
- The product is a **cyberbox** — a single-purpose, offline,
  battery/AC-powered physical object, not an app on a phone.

The current TUI (Ratatui) renders sentences and pause markers in a
fixed loop. The next architectural shift is to swap Ratatui for an
**agent harness** — likely the local `/mnt/projects/pi` runtime — so
that the meditation flow is driven by a tool-using language model
rather than a hand-coded state machine. The harness lets the model:

- **call skills / function tools** on the device (start TTS, ring a
  bell, wait N seconds, transition phase, ask a clarifying question)
- **read meditation literature** from a local repository archive of
  jhana templates (suttas, modern teachers, jhana-mistral training
  data) via a retrieval/tool call
- **compose a personalised session** by selecting and adapting a
  template to the user's stated goal, experience level, time budget,
  and post-session feedback — instead of running a fixed script

The harness path is currently aspirational; Ratatui stays in place
until the agent + skills runtime is built. None of the changes break
the offline-by-default rule: all literature, templates, and tools
must live on the device's storage.

### Candidate agent harnesses

The choice of harness drives whether we can stay in a single Rust
binary or have to add a Python / Node sidecar on the device. Selection
criteria, in priority order: **(a) runs fully offline on aarch64 with
no cloud calls**, (b) speaks to a local OpenAI-compatible endpoint
(or directly to rkllm-rs), (c) embeds as a library rather than a
separate service, (d) tool/function-call routing for skills,
(e) memory / RAG over the meditation-template archive, (f) low
overhead — the meditation experience can't tolerate seconds of
agent-side latency on top of LLM tok/s.

| Harness                                                            | Language | Embed model      | Tool calls         | RAG/memory        | aarch64-ready | Fit for jhana-rs                                                                 |
|--------------------------------------------------------------------|----------|------------------|--------------------|-------------------|---------------|----------------------------------------------------------------------------------|
| **`/mnt/projects/pi`** (local internal harness)                    | TBD      | unknown          | yes (skills)       | TBD               | likely        | Top contender if it stays small and Rust-callable; primary integration target.   |
| **Rig** — <https://www.rig.rs/>                                    | Rust     | library          | yes                | vector stores     | yes           | Strong fit: pure-Rust crate, OpenAI-compatible, can target any local server.     |
| **Flue** — <https://github.com/withastro/flue>                     | JS/TS    | Node runtime     | yes                | varies            | yes (Node)    | Adds Node to the device; OK for a sidecar but not single-binary.                 |
| **LangChain** / **LangGraph** (langchain-ai)                       | Python   | library/runtime  | mature             | mature            | yes           | Powerful but pulls Python + heavy deps; only worth it if other Python is on-box. |
| **swiftide** — <https://github.com/bosun-ai/swiftide>              | Rust     | library          | yes                | vector + indexers | yes           | Tilted toward RAG/indexing pipelines; useful for the template archive.           |
| **Direct rkllm-rs + custom tool router** (status quo + skills)     | Rust     | library          | hand-rolled        | hand-rolled       | yes           | Lowest dependency surface; most code to write ourselves.                         |

Notes:

- The **status-quo path** is to keep `src/llm.rs` (rkllm-rs) and add a
  small Rust skills router (a function-calling layer + a template
  retriever over a local jhana corpus). This is the simplest delta
  from where jhana-rs is today.
- **Rig** is the most natural step up — it's a Rust crate, hits any
  OpenAI-compatible endpoint (we can stand up `rkllama` or wrap
  rkllm-rs behind a thin local HTTP shim), and brings a tool-call
  abstraction without giving up the single-binary deployment.
- **Flue** and **LangGraph** are powerful, but bring an additional
  runtime (Node or Python) to the cyberbox. Acceptable as a sidecar
  if `/mnt/projects/pi` itself ends up being JS/TS based and we
  already need that runtime on device.
- **swiftide** is interesting specifically for indexing the meditation
  literature archive (Pali Canon excerpts, modern Jhana teachers,
  jhana-mistral training data) into a local vector store the agent
  can retrieve from — we may use it under whichever harness we pick.

Decision is deferred until the `/mnt/projects/pi` interface is more
concrete; until then jhana-rs continues to be implemented as a
hand-rolled Rust pipeline.



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
| Audio HW | See audio devices table below |
| Bluetooth | None |
| Display | 720x1280 portrait |
| GPIO | Buttons on GPIO 63 (up), 43 (down), 139 (enter), 138 (back) |
| Serial | `/dev/ttyS6` at 115200 baud |
| Network | Ethernet only; no internet in normal operation |

### Audio Devices (verified 2026-05-07)

| Card | ALSA Name | Type | Use |
|------|-----------|------|-----|
| 0 | `rockchip-hdmi1` | HDMI SPDIF out | External speaker via HDMI |
| 1 | `rockchip-es8316` | Onboard codec | 3.5mm headphone/line out + mic |
| 2 | `uctronics-codec` | Uctronics board | Onboard speaker + mic (AI in a Box) |
| 3 | `rockchip-hdmi0` | HDMI I2S out | External speaker via HDMI |

No Bluetooth. For TTS playback, use card 2 (Uctronics onboard speaker) or
card 1 (3.5mm jack for external speaker). USB audio devices also work.

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
- Parse `[pause_duration]` markers from meditation output (see below)

```rust
enum LlmOutput {
    Sentence(String),
    Pause(f32),
    Done,
}
```

#### Pause marker strategy

The Python jhana-dev used a **fine-tuned Mistral 7B**
([carecodeconnect/jhana-mistral-GGUF](https://huggingface.co/carecodeconnect/jhana-mistral-GGUF))
that was trained to emit `[duration]` markers (e.g. `[5]`, `[3.5]`) inline
with meditation text. The training dataset baked these markers into the
output, so the model produced them naturally.

For the Rust POC, we use Orca Mini 3B (untrained for pause markers). Two
options:

1. **Prompt engineering** (POC): System prompt + few-shot examples instruct
   the model to emit `[N]` markers. Less reliable than fine-tuning but
   works for demonstration.
2. **Fine-tune** (upgrade): Train a small model (Qwen3-4B or similar) on
   meditation texts with embedded pause markers, similar to the original
   jhana-mistral approach.

The pause parsing logic is a simple bracket state machine (ported from
`jhana-dev/src/meditation_guide.py`):
- Text outside `[]` accumulates in a sentence buffer
- `[` enters pause mode, `]` exits it
- Content between brackets is parsed as `float` seconds
- Pauses only fire after at least one sentence has been spoken (skip
  leading pauses)

#### POC prompt

```
System: You are a meditation guide. Generate calming, guided meditation
instructions. Use [N] markers (e.g. [10]) to indicate silent pauses of
N seconds. Speak in a warm, gentle tone. Keep sentences short for TTS.

Example:
"Close your eyes and take a deep breath in. [5] Now slowly exhale,
releasing any tension you feel. [3] Let your shoulders drop. [10]"
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
