# 04: Speech-to-Text Integration

## Use Case

The user speaks a natural-language meditation request:
- "Guide me in a breath awareness meditation"
- "I am feeling stressed"
- "How do I be more compassionate"

The LLM receives the transcribed text and selects an appropriate meditation
type from the available prompts (flower garden, lotus, sun, fountain, soup,
lake). No wake word needed — the user presses ENTER, speaks, and the system
listens until silence.

**Pipeline:** `[ENTER] -> [Mic capture] -> [VAD] -> [STT] -> [LLM prompt]`

---

## STT Engine Comparison

Evaluated offline (no internet) STT options compatible with sherpa-onnx on
Rock 5A (RK3588S, Cortex-A76/A55, 8 GB RAM, aarch64). All models run via
`OfflineRecognizer` in the sherpa-onnx Rust crate (already a dependency
for TTS).

### Candidates

#### 1. Whisper tiny.en (OpenAI)

- Format: ONNX (encoder + decoder split)
- Size: **~117 MB** (int8: encoder 12 MB + decoder 105 MB)
- Full-precision: **~221 MB** (encoder 36 MB + decoder 185 MB)
- Language: English only
- Architecture: Fixed 30s encoder window, zero-pads short audio
- Download: `sherpa-onnx-whisper-tiny.en.tar.bz2` from sherpa-onnx releases
- Sherpa-onnx config: `OfflineWhisperModelConfig { encoder, decoder, language, task }`

**Pros:** Well-known, battle-tested, good accuracy for its size.
**Cons:** Fixed 30s encoder window wastes compute on short utterances
(meditation requests are typically 2-5 seconds). Zero-padding overhead
means it processes 30s of audio even for a 3-second phrase.

#### 2. Moonshine tiny (Useful Sensors)

- Format: ONNX (preprocessor + encoder + uncached/cached decoders)
- Size: **~30 MB** total
- Language: English only
- Architecture: Variable-length encoder (no zero-padding)
- Download: `sherpa-onnx-moonshine-tiny-en-int8.tar.bz2` from sherpa-onnx releases
- Sherpa-onnx config: `OfflineMoonshineModelConfig { preprocessor, encoder, uncached_decoder, cached_decoder }`
- Paper: [Moonshine: Speech Recognition for Live Transcription and Voice Commands](https://arxiv.org/html/2410.15608v1)

**Pros:** 5x less compute than Whisper tiny for 10s audio at same WER.
Variable-length encoder scales with actual audio length — ideal for short
meditation requests. ~30 MB is 4x smaller than Whisper tiny int8.
**Cons:** Newer model, less battle-tested. English only (fine for this use case).

#### 3. SenseVoice (FunAudioLLM / Alibaba)

- Format: ONNX
- Size: **~220 MB**
- Languages: Chinese, Cantonese, English, Korean, Japanese
- Architecture: Non-autoregressive, extremely fast (70ms for 10s audio)
- Sherpa-onnx config: `OfflineSenseVoiceModelConfig { model, language, use_itn }`

**Pros:** 15x faster than Whisper-Large, multilingual, includes ITN
(inverse text normalization). Fastest inference of all candidates.
**Cons:** Larger model (220 MB). Multilingual is unnecessary — English
only needed. Overkill for short command-style utterances.

#### 4. Paraformer (FunASR / Alibaba)

- Format: ONNX
- Languages: Chinese, English, Cantonese
- Sherpa-onnx config: `OfflineParaformerModelConfig { model }`
- Note: Has RK NPU support in sherpa-onnx (relevant for RK3588S)

**Pros:** Could potentially use RK3588S NPU for acceleration.
**Cons:** Primarily optimized for Chinese. English is secondary.
NPU support is C++ only, not yet exposed in Rust bindings.

#### 5. whisper-rs (whisper.cpp Rust bindings)

- Crate: [whisper-rs](https://crates.io/crates/whisper-rs)
- Approach: Rust bindings wrapping whisper.cpp C++ library
- **Status: NOT EVALUATED** — adds a second C++ dependency alongside
  sherpa-onnx. Since sherpa-onnx already provides Whisper support via
  its own ONNX runtime, there's no reason to maintain two C++ speech
  libraries.

#### 6. whisper-burn (Pure Rust)

- Crate: [whisper-burn](https://crates.io/crates/whisper-burn)
- Approach: Pure Rust via Burn ML framework
- **Status: NOT EVALUATED** — Burn's ARM NEON support is less mature
  than ONNX Runtime. Pure Rust is appealing but not worth the risk
  on embedded aarch64.

### Results

| Model | Size (int8) | Fixed window | Compute (10s audio) | English accuracy | Notes |
|-------|-------------|--------------|---------------------|------------------|-------|
| **Moonshine tiny** | **~30 MB** | **No (variable)** | **~1x** | **Same as Whisper tiny** | **Best fit** |
| Whisper tiny.en | ~117 MB | Yes (30s) | ~5x | Baseline | Wastes compute on short audio |
| SenseVoice | ~220 MB | No | <1x | Good | Overkill, multilingual |
| Paraformer | ~200 MB | No | ~1x | Secondary | Chinese-first |

### Decision: sherpa-onnx + Moonshine tiny (2026-05-07)

**Engine: [sherpa-onnx](https://github.com/k2-fsa/sherpa-onnx)** (already a dependency for TTS)
**Model: Moonshine tiny int8** (~30 MB, English only)

Selected because:
- 5x less compute than Whisper tiny at same word error rate
- Variable-length encoder — no wasted compute on short utterances (2-5s)
- ~30 MB total — fits comfortably in 8 GB RAM alongside LLM + TTS
- Sherpa-onnx already loaded for TTS — zero new dependencies
- Designed for "live transcription and voice commands" — exactly our use case
- int8 quantization for ARM efficiency

**Fallback:** Whisper tiny.en (int8) if Moonshine has issues on aarch64.
The download is `sherpa-onnx-whisper-tiny.en.tar.bz2` and the config
swap is trivial (change `OfflineModelConfig.moonshine` to `.whisper`).

---

## VAD Engine

### Decision: sherpa-onnx Silero VAD (2026-05-07)

Using sherpa-onnx's built-in Silero VAD — already bundled, no new
dependencies. The Silero VAD model (~2 MB) is already on the Rock at
`/home/ubuntu/ai_in_a_box/downloaded/snakers4_silero-vad_master/`.

**Sherpa-onnx VAD API:**
```rust
let config = VadModelConfig {
    silero_vad: SileroVadModelConfig {
        model: Some("/path/to/silero_vad.onnx".into()),
        threshold: 0.5,
        min_silence_duration: 0.5,   // end-of-utterance after 500ms silence
        min_speech_duration: 0.25,
        window_size: 512,            // 16 kHz -> 512 samples per window
        max_speech_duration: 15.0,   // cap at 15s for meditation requests
    },
    sample_rate: 16000,
    num_threads: 1,
    ..Default::default()
};
let vad = VoiceActivityDetector::create(&config, 30.0);
```

**Flow:**
1. Feed 512-sample windows from mic via `vad.accept_waveform()`
2. `vad.detected()` returns true when speech starts
3. When speech ends (silence > 500ms), `vad.front()` returns `SpeechSegment`
4. Pass segment samples directly to `OfflineRecognizer` for STT

### Alternatives considered

| VAD | Type | Notes |
|-----|------|-------|
| **sherpa-onnx Silero VAD** | Built-in | **Selected** — zero new deps |
| webrtc-vad | C FFI (libfvad) | Lightweight, no ML model, more false positives |
| silero-vad-rust | Pure Rust (ort) | Would add second ONNX runtime |
| voice_activity_detector | Pure Rust (ort) | Silero V5, but redundant with sherpa-onnx |
| voice-stream | Pure Rust | Higher-level (capture + VAD), too opinionated |

---

## Mic Capture

### Hardware

Uctronics onboard mic (Card 2) — same ALSA device as the speaker.

```bash
# Test mic capture
arecord -D hw:2,0 -f S16_LE -r 16000 -c 1 -d 5 /tmp/test_mic.wav
aplay -D plughw:2,0 /tmp/test_mic.wav
```

### Rust approach

Use **cpal** crate (ALSA backend) for mic capture:
- 16 kHz sample rate, mono, i16 (convert to f32 for sherpa-onnx)
- Non-blocking callback feeds samples to VAD on audio thread
- VAD segments forwarded to STT via mpsc channel

Alternative: shell out to `arecord` like we do `aplay` for TTS. Simpler
but less control over buffering and harder to integrate with VAD.

---

## Model Download

Models to download to Rock 5A:

```bash
# Moonshine tiny int8 (primary)
cd /home/ubuntu/models
wget https://github.com/k2-fsa/sherpa-onnx/releases/download/asr-models/sherpa-onnx-moonshine-tiny-en-int8.tar.bz2
tar xvf sherpa-onnx-moonshine-tiny-en-int8.tar.bz2

# Whisper tiny.en int8 (fallback)
wget https://github.com/k2-fsa/sherpa-onnx/releases/download/asr-models/sherpa-onnx-whisper-tiny.en.tar.bz2
tar xvf sherpa-onnx-whisper-tiny.en.tar.bz2

# Silero VAD (if not already present)
# Check: ls /home/ubuntu/ai_in_a_box/downloaded/snakers4_silero-vad_master/
# Or download from sherpa-onnx releases
```

---

## Architecture

```
                         mpsc
  [cpal mic] -> [VAD thread] -----> [STT] -> [LLM prompt]
   16kHz mono    Silero VAD          Moonshine tiny
   512-sample    SpeechSegment       OfflineRecognizer
   windows       with audio data     -> transcribed text
```

Reuses the same thread + mpsc pattern as TTS and GPIO modules.

---

## Original Python implementation

### ai_in_a_box (Rock 5A)

Used Whisper via `faster-whisper` Python library:
- Model: `tiny.en` (CTranslate2 format, not ONNX)
- VAD: Silero VAD via `silero-vad` Python package
- Capture: `sounddevice` (`sd.InputStream`) at 16 kHz mono
- Flow: continuous listen -> VAD -> Whisper -> text

### jhana-dev (desktop)

Used `speech_recognition` Python library:
- Backend: Google Speech Recognition (online) or Whisper (offline)
- VAD: energy-based (simple threshold in `speech_recognition`)
- Not suitable for offline embedded use
