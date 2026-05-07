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
