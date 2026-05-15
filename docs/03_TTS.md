# 03: Text-to-Speech Integration

## TTS Engine Benchmark

Systematic comparison of Rust TTS options on Rock 5A (RK3588S,
Cortex-A76/A55, 8 GB RAM, aarch64). Target: natural-sounding speech
with controllable pace for meditation guidance.

### Candidates

#### 1. piper-rs (Pure Rust, Piper ONNX bindings)

- Crate: [piper-rs](https://crates.io/crates/piper-rs) v0.1.9
- Approach: Rust bindings to Piper TTS via ort (ONNX Runtime)
- Dependencies: espeak-rs, ort, ndarray
- **Status: BROKEN** — `ndarray` version conflict (0.16 vs 0.17) between
  piper-rs and ort. Does not compile. Upstream issue, no fix available.

#### 2. any-tts (Pure Rust, Candle backends)

- Crate: [any-tts](https://crates.io/crates/any-tts) v0.1.1
- Approach: Pure Rust ML framework (Candle), supports Kokoro/Qwen3-TTS
- Dependencies: 298 packages (tokio, reqwest, hyper, tokenizers, lindera)
- **Status: TOO HEAVY** — pulls in full async runtime, HTTP stack, Japanese
  dictionary (lindera), and tokenizer library. Impractical for embedded.
  Compiles but excessive for an 8 GB ARM device.

#### 3. sherpa-onnx (C++ FFI, ONNX Runtime)

- Crate: [sherpa-onnx](https://crates.io/crates/sherpa-onnx) v1.13.0
- Approach: Rust FFI bindings to sherpa-onnx C++ library
- Supported models: VITS (Piper), Matcha, Kokoro, Pocket TTS, others
- Dependencies: sherpa-onnx-sys (static C++ lib), ureq, bzip2
- ARM NEON: Yes (via ONNX Runtime)
- **Status: WORKING** — tested on Rock 5A with Piper medium model

#### 4. coqui-tts (Python bindings)

- Crate: [coqui-tts](https://crates.io/crates/coqui-tts) v0.2.0
- Approach: PyO3 bindings to Coqui TTS Python library
- **Status: NOT SUITABLE** — requires Python runtime. The original
  jhana-dev Python project used Coqui TTS (`TTS.api`) with the Jenny
  model on desktop, but this doesn't translate to a pure Rust binary.

#### 5. Piper CLI (shell out)

- Binary: `/usr/local/bin/piper` (already installed on Rock)
- Approach: `std::process::Command` to shell out to Piper CLI
- **Status: FALLBACK** — works but CLI approach has overhead and less
  control. The `--length_scale` flag works but the `low` model produces
  unnaturally fast speech even at high scale values.

### Results

| Engine | Type | Status | Load time | Synth time | Quality | Notes |
|--------|------|--------|-----------|------------|---------|-------|
| **sherpa-onnx** | C++ FFI | **Working** | **1.59s** | **0.75s** | **Medium (22050 Hz)** | **Selected** |
| piper-rs | Pure Rust | Broken | — | — | — | ndarray version conflict |
| any-tts | Pure Rust | Compiles | — | — | — | 298 deps, too heavy |
| coqui-tts | Python | Skip | — | — | — | Requires Python runtime |
| Piper CLI | Shell out | Works | — | — | Low (44100 Hz) | Unnaturally fast |

### Decision update: Piper CLI (2026-05-07)

sherpa-onnx was removed due to a protobuf-lite symbol clash with
sensevoice-rs. See `docs/05_NPU.md` for details. Current TTS is Piper
CLI via `std::process::Command` (stopgap). The voice is robotic — voice
cloning is the planned fix (see below).

---

## Voice cloning TTS — Rust options (surveyed 2026-05-08)

The Piper lessac voice is too robotic for meditation guidance. Reference
WAV files for voice cloning are in `audio/voice-cloning/` (4 voices from
the Python prototype's Coqui XTTS v2 integration).

"Zero-shot" cloning = no training step needed, just a 3-30 second
reference WAV at inference time. The model clones the voice characteristics
on the fly. (Misleading term — "zero-shot" refers to zero *training*
iterations, not zero audio input.)

### Recommended options (aarch64, pure Rust or near-Rust)

| Crate | Voice cloning | Backend | aarch64 | Stars | Notes |
|-------|--------------|---------|---------|-------|-------|
| [`qwen_tts`](https://github.com/TrevorS/qwen3-tts-rs) v0.4.0 | Yes (x-vector + ICL) | Candle (no Python) | Confirmed (Docker CI) | 126 | **Top pick.** 0.6B/1.7B models. Same framework as sensevoice-rs. |
| [`qwen3_tts`](https://github.com/second-state/qwen3_tts_rs) v0.2.2 | Yes (ICL mode) | libtorch (~800 MB) | Confirmed (ARM64 bins) | 208 | Most mature. `voice_clone` CLI included. |
| [`fish-speech.rs`](https://github.com/EndlessReform/fish-speech.rs) | Yes (ref audio) | Candle | CPU fallback | 110 | ~15 MB binary. OpenAI-compatible API. Fish Speech 1.2/1.4/1.5. |
| [`vox`](https://github.com/mrtozner/vox) v0.6.0 | Yes (Chatterbox, 350 MB) | sherpa-onnx/ONNX | RPi4+ confirmed | 32 | Multi-engine (Kokoro, Qwen3, Piper, Pocket TTS). |
| [`rwkv-tts-rs`](https://github.com/cgisky1980/rwkv-tts-rs) v0.2.5 | Yes (ref audio) | wgpu (Vulkan) | Mali-G610 Vulkan | 26 | RWKV arch. Under-documented. |
| [`voxtral-tts-rs`](https://github.com/second-state/voxtral_tts_rs) v0.1.1 | Yes (ref audio) | libtorch | ARM64 bins | 7 | Mistral 4B — tight on 8 GB RAM. State-of-the-art quality. |

### Fixed-voice TTS (no cloning, but fast/lightweight)

| Crate | Backend | aarch64 | Stars | Notes |
|-------|---------|---------|-------|-------|
| [`kitten_tts_rs`](https://github.com/second-state/kitten_tts_rs) v0.2.2 | ONNX Runtime | Confirmed | 288 | 8 voices, 8-9x RT, ~10 MB binary. Best edge fixed-voice TTS. |
| [`kokoros`](https://github.com/lucasjinreal/Kokoros) | ONNX | — | 769 | StyleTTS2, preset voice styles, OpenAI API. |
| [`kokoroxide`](https://crates.io/crates/kokoroxide) | ONNX Runtime | — | — | espeak-ng phonemizer. |
| Piper CLI / piper-rs | ONNX / C++ | Native binary | — | Current stopgap. Fixed voices, no cloning. |

### Not viable

| Option | Why |
|--------|-----|
| `coqui-tts` (Rust, v0.2.0) | PyO3 wrapper, stale (2023), Coqui AI shut down 2024. |
| F5-TTS RKNN (happyme531) | 39x real-time on NPU — too slow. Python only. |
| CosyVoice | Python only, no Rust crate. C++ port exists (cosyvoice.cpp/GGML). |
| OpenVoice | Python only, no Rust bindings. |
| Bark TTS | Python/PyTorch only, no Rust crate. |
| MetaVoice-1B | Candle example only, not a published crate. |
| Parler-TTS | Description-based (not ref-audio cloning). Candle example. |

### Other relevant projects

| Project | Type | Notes |
|---------|------|-------|
| [`pocket-tts`](https://github.com/kyutai-labs/pocket-tts) (Kyutai) | Python + community Candle port | 1.6B, CPU-first, voice cloning via Mimi encoder. Community Rust/Candle ports exist but not on crates.io. |
| [`any-tts`](https://github.com/TM9657/any-tts) v0.1.1 | Candle | Multi-backend (Kokoro, OmniVoice, Qwen3, VibeVoice). Cloning only via VibeVoice-1.5B. Very new (Apr 2026). |
| [`voirs`](https://crates.io/crates/voirs) v0.1.0-alpha.2 | Pure Rust | 9 languages, 54 voices. Alpha. No cloning. |
| [`gpt-sovits`](https://crates.io/crates/gpt-sovits) | Rust | Few-shot cloning (1 min audio). Sparse docs. |

### NPU acceleration for TTS on RK3588

| Approach | Tool | Cloning | NPU | Speed |
|----------|------|---------|-----|-------|
| Piper + RKNN decoder | [Paroli](https://github.com/marty1885/paroli) (C++) | No | Yes | ~4.3x vs CPU |
| Piper RKNN decoders | [danielferr85](https://huggingface.co/danielferr85/piper-checkpoints-rknn) | No | Yes | Pre-converted |
| MMS-TTS + RKNN | [RKLLama](https://github.com/NotPunchnox/rkllama) | No | Yes | Good |
| F5-TTS RKNN | [happyme531](https://huggingface.co/happyme531/F5-TTS-RKNN2) | Yes | Partial | 39x RT (too slow) |
| Any ONNX via `ort` | [ort](https://crates.io/crates/ort) crate | Depends | CPU only | No RKNN routing |

No Rust crate currently wraps `librknnrt.so` for TTS inference directly.
The `ort` crate runs ONNX on aarch64 CPU but does **not** route to the
RK3588 NPU. Custom RKNN TTS would require `rknpu2` or `rknn-rs` crates
with a converted `.rknn` decoder model.

### Recommendation for jhana-rs

**Start with `qwen_tts`** (TrevorS/qwen3-tts-rs):
- Pure Candle — same ML framework as sensevoice-rs (already a dependency)
- aarch64 confirmed with Docker CI
- 0.6B model should fit alongside LLM (~4 GB) + STT (~1 GB) in 8 GB
- ICL mode: 3 seconds of reference WAV + transcript → cloned voice
- x-vector mode: faster, lower quality, no transcript needed
- No Python, no libtorch, no heavy runtime dependencies

**Fallback:** `fish-speech.rs` if Qwen3-TTS quality is insufficient.
**Future:** Once voice cloning model is working, investigate ONNX export
→ RKNN conversion for NPU acceleration.

### Crate discovery sources

- [crates.io/keywords/text-to-speech](https://crates.io/keywords/text-to-speech)
- [lib.rs/search?q=tts](https://lib.rs/search?q=tts)
- [lib.rs/search?q=voice+cloning](https://lib.rs/search?q=voice+cloning)
- [docs.rs](https://docs.rs) — auto-generated API docs for every published crate

---

### Decision update: Moonshine Voice evaluated, reverted to paroli (2026-05-15)

Tested [Moonshine Voice](https://github.com/usefulsensors/moonshine) (Useful
Sensors, Feb 2026 release) as an alternative TTS engine alongside the
existing paroli + RKNN path. Selected `kokoro_am_michael` for a natural male
voice; integrated as a persistent Python worker (`scripts/moonshine_tts_worker.py`)
spawned by `src/tts.rs` with a JSON `{text, out}` request/`{ok}` reply
protocol over stdin/stdout. Kept paroli, SenseVoice, espeak-ng all
selectable via `config/jhana.json` (`tts.engine`).

**Outcome: reverted to paroli + RKNN NPU.**

| Engine | RTF | TTFT (typical sentence) | Voice quality | Notes |
|--------|-----|-------------------------|---------------|-------|
| paroli (RKNN decoder) | **0.29** | ~0.3 s | Good female (ljspeech) | NPU-accelerated, current default |
| Moonshine (CPU, kokoro_am_michael) | ~1.5 | ~2.5 s | Excellent male | No NPU path; ONNX Runtime CPU only |

Speed dominated the decision — paroli's NPU path is ~5× faster TTFT, which
matters for the "Speak now" cue (cached) and meditation pacing alike.
Moonshine's voice quality is higher and the male voice is a better fit
for the intended meditation persona, but at current Rock 5A CPU speeds
the TTFT regression was perceptible compared to paroli NPU.

Moonshine support is kept in the codebase and config; flip `tts.engine`
to `"moonshine"` to re-enable. Future direction: investigate whether
Kokoro ONNX decoder can be routed through RKNN (similar trick to
paroli's Piper-decoder conversion) for an NPU-backed male voice.

---

### Decision: sherpa-onnx (2026-05-07)

**Engine: [sherpa-onnx](https://github.com/k2-fsa/sherpa-onnx)** (C++ FFI via Rust bindings)
**Model: vits-piper-en_US-lessac-medium** (63 MB, 22050 Hz)

Selected because:
- 0.75s synthesis for a full sentence (faster than real-time)
- 1.59s model load (acceptable for always-on device)
- Natural speech quality at medium resolution
- `length_scale` control for meditation pacing (1.3 = calm pace)
- Built-in WAV output and progress callbacks
- Supports multiple model families (VITS, Matcha, Kokoro) for future upgrades
- Static linking — no runtime dependencies beyond the binary
- Actively maintained by k2-fsa (speech processing research group)

**Trade-off:** Not pure Rust (C++ FFI), but the Rust API is safe and
ergonomic. Pure Rust TTS is not viable today due to broken or bloated
crates. sherpa-onnx is the same approach as llama-cpp-2 (C++ core,
Rust bindings) and is battle-tested on ARM.

---

## Audio Hardware

### Playback devices (Rock 5A)

| Card | Device | Type | Notes |
|------|--------|------|-------|
| 0 | rockchip-hdmi1 | HDMI | SPDIF output |
| 1 | rockchip-es8316 | 3.5mm jack | Headphone/line out |
| 2 | uctronics-codec | Onboard speaker | AI in a Box hardware |
| 3 | rockchip-hdmi0 | HDMI | I2S output |

Both card 1 (3.5mm) and card 2 (onboard speaker) tested and working.
Use `plughw:N,0` (not `hw:N,0`) for automatic channel conversion
(Piper outputs mono, some devices require stereo).

### Volume control

```bash
# Uctronics onboard speaker (card 2)
amixer -c 2 sset DAC N    # N = 0-4 (0=min, 3=75%, 4=max)

# Check current volume
amixer -c 2 sget DAC
```

DAC 1 (25%) is comfortable for indoor use. Default DAC 3 (75%) is very loud.

### Playback command

```bash
aplay -D plughw:2,0 /tmp/test.wav    # onboard speaker
aplay -D plughw:1,0 /tmp/test.wav    # 3.5mm headphone jack
```

---

## Model on device

| Model | Path | Size | Sample rate | Quality |
|-------|------|------|-------------|---------|
| Piper en_US-lessac-low | `/home/ubuntu/ai_in_a_box/downloaded/en_US-lessac-low.onnx` | 61 MB | 44100 Hz | Low — too fast |
| **Piper en_US-lessac-medium** | `/home/ubuntu/models/vits-piper-en_US-lessac-medium/` | 63 MB | 22050 Hz | **Medium — good** |

The medium model includes bundled `tokens.txt` and `espeak-ng-data`
directory required by sherpa-onnx.

**Note:** The original Piper ONNX models (from piper.readthedocs.io) lack
`sample_rate` metadata in the ONNX file, which sherpa-onnx requires.
Use sherpa-onnx's pre-packaged Piper models from their
[GitHub releases](https://github.com/k2-fsa/sherpa-onnx/releases/tag/tts-models)
which include the correct metadata.

---

## Original Python implementation

### jhana-dev (desktop)

Used Coqui TTS (`from TTS.api import TTS`) with:
- Model: `tts_models/en/jenny/jenny`
- Playback: `simpleaudio`
- Post-processing: `audiostretchy` with ratio 1.25x to slow down speech
- Device: desktop with GPU (CUDA optional)

### ai_in_a_box (Rock 5A)

Used Piper as a Python library (`import piper`):
- Model: `en_US-lessac-low` via `piper.PiperVoice.load()`
- Streaming: `synthesize_stream_raw()` for real-time audio
- Playback: `sounddevice` (`sd.OutputStream`) at 16000 Hz
- Length scale: 1.2 default, dynamically adjusted based on queue size
- Volume: software-based (multiply samples by volume factor)
- Sample rate: 16000 Hz (not 44100 — model config says 44100 but
  the Python code forced 16000)
