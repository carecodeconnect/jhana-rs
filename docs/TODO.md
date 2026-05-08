# TODO: jhana-rs POC/MVP

Tracks all steps from initial hardware setup through Phase 1 POC completion.
See [SPECS.md](SPECS.md) for full technical specifications.

**POC success criterion (Phase 1):** Text prompt -> LLM streams meditation
text -> ratatui displays sentences with pause markers -> Piper generates WAV
files. Runs on Rock 5A. Peak memory under 6 GB. First displayed sentence
within 3s of prompt.

---

## 0. Hardware & Dev Environment Setup

- [x] SSH into Rock 5A via direct ethernet link
- [x] Stop and disable AI in a Box captioning service
- [x] Expand eMMC root partition (10.1 GB -> 29 GB)
- [x] Delete unused NLLB translation model (~1.2 GB freed)
- [x] Set up NAT forwarding (X61s wifi -> Rock internet)
- [x] Install Rust toolchain on Rock (1.95.0 aarch64)
- [x] Verify build deps on Rock (build-essential, cmake, pkg-config, libasound2-dev)
- [x] Install rsync on Rock
- [x] Install console-setup, configure TerminusBold 32x16 font
- [x] Suppress kernel DMA console messages (dmesg --console-off)
- [x] Set up pre-commit hooks (rustfmt + clippy)
- [x] Create dev scripts (rock-ssh, rock-sync, rock-build, rock-run, rock-stop, rock-log, rock-deploy)
- [x] Document all setup steps in 01_DEV_SETUP.md

## 1. TUI Display (ratatui)

- [x] Basic ratatui TUI with header, body, footer
- [x] Retro phosphor green/amber palette
- [x] Meditation text display with styled pause markers
- [x] Scrollable body (UP/DOWN buttons + arrow keys)
- [x] File logging (jhana-rs.log) for remote debugging
- [x] Signal handling (SIGTERM/SIGINT) for clean remote shutdown
- [x] Clean exit to tty1 login prompt with large font
- [x] GPIO button input (←quit →start ↑up ↓down)
- [x] Button labels in footer matching physical layout
- [x] Rustdoc comments on all public items
- [x] Integration tests for TUI rendering
- [x] TUI state transitions (Idle -> Generating -> Paused -> Done)
- [x] Live token count and speed display in footer
- [x] Sentence-by-sentence reveal (not all at once)
- [ ] Light/outdoor theme (white bg, dark text, retro style — match x61s i3 look)
- [ ] Pause marker countdown timer in TUI (scaffolded, activates with LLM)

### TUI visual upgrade — retro sci-fi aesthetic (Tron / Matrix / WarGames)

Inspiration: retro CRT terminal that generates meditations — an ancient AI
meditation machine. Cyan glow lines (Tron), digital rain (Matrix), typewriter
reveal (WarGames).

- [ ] Typewriter text reveal — characters appear one at a time with cursor blink
      (WarGames WOPR terminal style), synced to TTS voice playback speed
- [ ] Matrix digital rain — falling character columns as idle/pause animation
- [ ] Tron glow borders — cyan or amber glowing line borders (`Color::Rgb`)
- [ ] CRT scanline effect — alternating dim/bright rows for phosphor CRT look
- [ ] Pulsing `●` dot animation during `[pause]` sections, synced to TTS silence
- [ ] Centered narrow text column (book-style) — more readable, more zen
- [ ] Hide chrome during active meditation — footer/status hidden, reveal on keypress
- [ ] Full-screen fade-to-black during pause periods
- [ ] HSL-based palette — warm amber/gold/cyan tones, harmonious color construction
- [ ] TachyonFX integration — coalesce (text materializes from noise), dissolve,
      fade in/out effects for state transitions (Idle→Generating→Paused→Done)
- [ ] Visual effects synced to voice — text reveal speed matches TTS speech rate,
      animations pulse with audio cadence, pauses trigger visual transitions
- [ ] Breathing circle animation — expanding/contracting Unicode art during pauses
- [ ] Startup sequence — WarGames-style boot text ("SHALL WE MEDITATE?"),
      system diagnostics scrolling before meditation begins

References:
- [Ratatui examples](https://github.com/ratatui/ratatui/tree/main/examples) — Colors-RGB, Constraint Explorer, Modifiers
- [TachyonFX](https://github.com/ratatui/tachyonfx) — 50+ effects: coalesce, dissolve, fade, slide-in
- [Awesome ratatui](https://github.com/ratatui/awesome-ratatui) — curated list of apps and widgets
- [Ratatui showcase](https://ratatui.rs/showcase/apps/) — Yazi, Gitui, real-world examples
- [Throbber widgets](https://github.com/arkbig/throbber-widgets-tui) — spinner animations
- [Braille bar](https://github.com/penso/ratatui-braille-bar) — elegant progress bars with braille chars
- [Ratatui garnish](https://github.com/franklaranja/ratatui-garnish) — composable decorations, shadows

## 2. LLM Integration

**Current: mistral.rs + Ministral 3B on CPU** (2026-05-07). See `docs/02_LLM.md`.
mistral.rs runs as a local HTTP server (OpenAI-compatible API, port 8321).
jhana-rs connects via raw TCP, parses SSE streaming tokens.

**Next: rkllm-rs + 3B/4B model on NPU** — pure Rust LLM inference via
[rkllm-rs](https://github.com/darkautism/rkllm-rs) (same author as
sensevoice-rs). Loads `.rkllm` models directly on RK3588 NPU. Expected
~5-8 tok/s (3B) or ~4-6 tok/s (4B) on NPU, freeing CPU for TTS+STT.
No HTTP server, no Python, no Go — direct Rust FFI to `librkllmrt.so`.

- [x] Create `src/llm.rs` module with ChunkParser and LlmOutput types
- [x] Implement bracket state machine for pause marker parsing
- [x] Unit tests for pause parsing
- [x] Build llama-cpp-2 on Rock (C++ wrapper — needs libclang-dev)
- [x] Build llama-gguf on Rock (pure Rust — needs protobuf-compiler)
- [x] Download Orca Mini 3B GGUF (1.9 GB) to Rock
- [x] Benchmark llama-cpp-2: **5.8 tok/s** on Orca Mini 3B Q4_0 (target was >2.5)
- [x] Benchmark llama-gguf: **~0.25 tok/s** — 23x slower, no ARM NEON
- [x] Decision: **mistral.rs + Ministral 3B** (3.89 tok/s, native [N] pauses)
- [x] Download Ministral 3B GGUF (2.0 GB) to Rock
- [x] Load GGUF model via mistral.rs server, verify it loads
- [x] Benchmark mistral.rs: **3.89 tok/s** (exceeds 2.5 target)
- [x] Clean meditation examples into system/user prompts (`prompts/`)
- [x] System prompt for meditation guide with [N] markers
- [x] 6 meditation types: flower garden, lotus, sun, fountain, soup, lake
- [x] Streaming token output via mpsc channel (SSE -> ChunkParser -> TUI)
- [x] Sentence-level buffering (ChunkParser splits on `.` boundary)
- [x] Parse [N] pause markers from LLM output
- [x] LlmOutput enum (Sentence, Pause, Done, Error)
- [x] Wire LLM output to TUI display (live streaming)
- [x] Rustdoc for llm module
- [x] SSE parser tests (canned data via Cursor)
- [ ] Pin LLM thread to Cortex-A76 cores (cores 4-7) for performance
- [ ] Meditation type selection menu in TUI
- [ ] Measure tokens/second on Rock with live TUI, verify >2.5 tok/s
- [ ] Memory profiling: verify LLM stays within budget (~2 GB)

### LLM on NPU: RKLLama / rkllm-rs comparison

Compare NPU-accelerated LLM inference vs current CPU mistral.rs approach.
TTS and STT stay on CPU — NPU is dedicated to LLM. Goal: faster token
generation while freeing A76 cores for audio processing.

Two runtime formats on RK3588:
- `.rknn` = general NN (STT, VAD, TTS, vision) via `librknnrt.so`
- `.rkllm` = LLM-specific via `librkllmrt.so` (w8a8 quantization only)

#### Option A: RKLLama (HTTP server, backup option)

[RKLLama](https://github.com/NotPunchnox/rkllama) — Ollama-compatible
server wrapping Rockchip's RKLLM runtime. OpenAI-compatible
`/v1/chat/completions` endpoint — same API as mistral.rs, minimal code
changes (`SERVER_ADDR` swap in `src/llm.rs`).

- [ ] Install RKLLama on Rock (`pip install rkllama`, needs Python)
- [ ] Download pre-converted 3B model from
      [jamescallander](https://huggingface.co/collections/jamescallander/rk3588-rkllm-models):
      Qwen2.5-3B-Instruct or Llama-3.2-3B-Instruct (.rkllm, ~3.5-5 GB RAM)
- [ ] Download pre-converted 4B model: Gemma-3-4B (.rkllm, ~4.5-6 GB RAM)
- [ ] Start RKLLama server: `rkllama serve` (port 8080)
- [ ] Test `/v1/chat/completions` endpoint with meditation system prompt
- [ ] Benchmark 3B model: tok/s on NPU (expect ~5-8 tok/s)
- [ ] Benchmark 4B model: tok/s on NPU (expect ~4-6 tok/s)
- [ ] Compare meditation text quality: 3B vs 4B vs Ministral 3B (CPU)
- [ ] Measure RAM: verify LLM + TTS + STT fit in 8 GB
- [ ] Test simultaneous NPU LLM + CPU TTS + CPU STT (no contention)
- [ ] If viable: update `SERVER_ADDR` in `src/llm.rs` to `127.0.0.1:8080`

#### Option B: rkllm-rs (pure Rust, no HTTP server) — SELECTED

[rkllm-rs](https://github.com/darkautism/rkllm-rs) v0.1.14 — Rust FFI
bindings for `librkllmrt.so`. Same author as sensevoice-rs. Loads `.rkllm`
models directly, callback-based streaming. No Go, no Python, no HTTP
overhead. Replaces `src/llm.rs` HTTP client with direct NPU calls.

##### Setup (on Rock)

- [ ] Download `librkllmrt.so` v1.2.3 from
      [airockchip/rknn-llm](https://github.com/airockchip/rknn-llm/tree/main/rkllm-runtime/Linux/librkllm/aarch64)
- [ ] Install: `sudo cp librkllmrt.so /usr/lib/ && sudo ldconfig`
- [ ] Download 3B .rkllm model from
      [jamescallander](https://huggingface.co/collections/jamescallander/rk3588-rkllm-models):
      Qwen2.5-3B-Instruct or Llama-3.2-3B-Instruct (~3.5-5 GB RAM)
- [ ] Place model at `/home/ubuntu/models/<model-name>.rkllm`

##### Test binary (on X61s: edit, on Rock: build+run)

- [ ] Add `rkllm-rs = "0.1.14"` to Cargo.toml
- [ ] Create `src/bin/test_rkllm.rs`:
      - Load model via `rkllm_rs::RkllmModel::new(path, params)`
      - Set system prompt (meditation guide)
      - Send user prompt ("guide me through a breathing meditation")
      - Stream tokens via callback, print each token + timing
      - Measure: load time, first token latency, tok/s, total RAM
- [ ] `cargo check` on X61s (verify types compile)
- [ ] Sync to Rock, `cargo build --bin test_rkllm`
- [ ] Run test_rkllm, verify tokens stream from NPU
- [ ] Benchmark: tok/s (expect ~5-8 for 3B), RAM usage, first token latency

##### Integrate into TUI (rewrite src/llm.rs)

- [ ] Study rkllm-rs callback API — map to mpsc sender pattern:
      callback receives token → sends `LlmOutput::Sentence` via tx
- [ ] Rewrite `src/llm.rs`:
      - Remove all HTTP/TCP/SSE code
      - Add `rkllm_rs` imports
      - Load model once at thread start (like TTS/STT pattern)
      - `start_streaming()` spawns thread, runs inference with callback
      - Callback feeds tokens through `ChunkParser` → `LlmOutput` via mpsc
      - Keep `ChunkParser` (bracket state machine for `[N]` pause markers)
      - Keep `load_prompts()` for system/user prompt loading
- [ ] Wire into main.rs — same `llm_tx`/`llm_rx` channels, no API change
- [ ] Test end-to-end: → button → STT → LLM (NPU) → TTS → display
- [ ] Compare meditation text quality: rkllm-rs 3B vs Ministral 3B (CPU)
- [ ] Measure RAM: LLM + STT + TTS must fit in 8 GB
- [ ] Test 4B model (Gemma-3-4B) if 3B RAM allows headroom
- [ ] Remove mistral.rs from Rock (no HTTP server needed)

##### Pre-converted .rkllm model sources

- [jamescallander collection](https://huggingface.co/collections/jamescallander/rk3588-rkllm-models) — 25+ models
- [Pelochus/ezrkllm-collection](https://huggingface.co/Pelochus/ezrkllm-collection) — with RAM estimates
- Custom conversion via [rknn-llm toolkit](https://github.com/airockchip/rknn-llm) (x86_64 only)

## 3. Audio & TTS Integration

### Speaker test (after LLM works)

4 audio devices on Rock (no Bluetooth):
- Card 2: Uctronics onboard speaker + mic (AI in a Box hardware)
- Card 1: 3.5mm headphone/line out (rockchip-es8316)
- Cards 0,3: HDMI audio out

- [x] Test onboard speaker: `aplay -D plughw:2,0 test.wav` (needs plughw for mono->stereo)
- [x] Test 3.5mm jack: `aplay -D plughw:1,0 test.wav`
- [x] Verify Piper TTS outputs playable WAV (16-bit mono 44100 Hz)
- [x] Volume control: `amixer -c 2 sset DAC N` (0-4, default 3=75%, 1=25% good)
- [x] Speech rate: `--length_scale 1.5` for calm meditation pace

### TTS integration

**sherpa-onnx removed** (2026-05-07) — protobuf-lite symbol clash with
sensevoice-rs's sentencepiece-sys. See `docs/05_NPU.md` for details.

**Current: Piper CLI** (stopgap) — shell out to `/usr/local/bin/piper`.
Same model, same output, no Rust dep conflicts.

Model on device: `/home/ubuntu/models/vits-piper-en_US-lessac-medium/`

- [x] Create `src/tts.rs` module (rewrite: Piper CLI, not sherpa-onnx)
- [ ] Shell out to Piper via std::process::Command (text stdin → WAV stdout)
- [ ] Generate WAV file per sentence
- [ ] Configure speech rate via --length_scale (default 1.3)
- [ ] Wire to LLM pipeline: each Sentence → Piper → aplay
- [ ] Skip TTS for Pause markers (just wait)
- [ ] Error handling for Piper subprocess failures
- [ ] Measure TTS latency per sentence on Rock

**Goal: pure Rust TTS on NPU** — build a Piper VITS inference crate
using the same candle + rknn-rs stack as sensevoice-rs.

#### Option A: piper-rs (Rust, CPU only)

[piper-rs](https://github.com/thewh1teagle/piper-rs) v0.2 — pure Rust
Piper TTS bindings using onnxruntime (not sherpa-onnx). 55 stars, works
on aarch64. No NPU support but no protobuf conflict either (uses its
own onnxruntime, not sherpa-onnx's vendored copy).

- [ ] Test `piper-rs` on Rock — verify no symbol clashes with sensevoice-rs
- [ ] Benchmark vs Piper CLI (should be identical, both use onnxruntime)
- [ ] If clean: replace Piper CLI shell-out with piper-rs API calls

#### Option B: Rust Piper VITS on NPU (candle + rknn-rs)

Build a `piper-rknn-rs` crate following the sensevoice-rs architecture.
Same author ecosystem (darkautism's candle + rknn-rs pattern). This is
the pure Rust NPU path for TTS.

Piper VITS pipeline:
1. Text → phonemes (espeak-ng FFI or lookup table)
2. Phonemes → encoder (small, CPU via candle)
3. Encoder → VITS decoder (heavy part → NPU via rknn-rs)
4. Decoder → WAV samples

Pre-converted RKNN decoders exist:
[danielferr85/piper-checkpoints-rknn](https://huggingface.co/danielferr85/piper-checkpoints-rknn)

- [ ] Study Piper VITS architecture (encoder/decoder split)
- [ ] Study sensevoice-rs source for candle + rknn-rs integration pattern
- [ ] Prototype: load Piper RKNN decoder via rknn-rs, run inference
- [ ] Add phoneme preprocessing (espeak-ng or text-to-phoneme crate)
- [ ] Wire encoder (candle, CPU) → decoder (rknn-rs, NPU)
- [ ] Benchmark: expect RTF ~0.15 (4.3x faster than CPU RTF ~0.31)
- [ ] Package as reusable crate (contribute to ecosystem)

#### Option C: Paroli HTTP sidecar (C++, NPU, backup)

[Paroli](https://github.com/marty1885/paroli) — C++ streaming Piper TTS
with RKNN NPU decoder. HTTP API at `POST :8848/api/v1/synthesise`.
Proven 4.3x speedup. Call from Rust via HTTP. Backup option if pure
Rust NPU TTS is too complex.

- [ ] Build Paroli on Rock (`cmake .. -DUSE_RKNN=ON`)
- [ ] Test HTTP endpoint from Rust
- [ ] Benchmark vs Piper CLI

#### Option D: RKLLama TTS endpoint (Go, NPU, backup)

[RKLLama](https://github.com/NotPunchnox/rkllama) has `/v1/audio/speech`
(OpenAI-compatible). Runs Piper ONNX encoder + RKNN decoder. Backup
option — useful if already running RKLLama for LLM.

### Vision: pure Rust NPU pipeline

The end state for jhana-rs is a fully Rust, fully NPU pipeline:

| Module | Crate | NPU Runtime | Status |
|--------|-------|-------------|--------|
| **STT** | sensevoice-rs | `librknnrt.so` (RKNN) | Working (2026-05-07) |
| **LLM** | rkllm-rs | `librkllmrt.so` (RKLLM) | Next to test |
| **TTS** | piper-rknn-rs (fork) | `librknnrt.so` (RKNN) | To build |

All three use darkautism's candle + rknn-rs stack. No Python, no Go,
no HTTP servers, no C++ wrappers. Single Rust binary, three NPU
workloads.

### Build order (2026-05-07)

**Step 1: STT — sensevoice-rs on NPU** *(in progress)*
- [x] sensevoice-rs compiles and links on Rock (cargo check + build)
- [x] SenseVoice RKNN model downloaded to Rock (462 MB encoder)
- [x] test_stt transcribes Piper-generated speech correctly (3.2s inference)
- [x] src/stt.rs module created, wired into TUI main loop
- [ ] Drop sherpa-onnx from Cargo.toml (protobuf clash resolved)
- [ ] Rewrite src/tts.rs to use Piper CLI (stopgap, no sherpa-onnx)
- [ ] Rebuild on Rock without sherpa-onnx — verify clean build
- [ ] Test STT end-to-end on TUI: → button → mic → transcription displayed
- [ ] Benchmark STT latency with live mic (target: <5s for short utterance)

**Step 2: LLM — rkllm-rs on NPU**
- [x] Add `rkllm-rs = "0.1.14"` to Cargo.toml (2026-05-08)
- [x] Install `librkllmrt.so` v1.2.3 on Rock (2026-05-08)
- [x] Download Llama-3.2-3B-Instruct .rkllm to Rock (2026-05-08, 4.35 GB, jamescallander w8a8_g128)
- [x] Create `src/bin/test_rkllm.rs` — load model, stream tokens (2026-05-08)
- [x] Document .rkllm model options in `docs/05_NPU.md` (2026-05-08)
- [x] Build `test_rkllm` on Rock with `RUSTFLAGS="-C target-feature=+fp16"` (2026-05-08)
- [x] Test RKLLM on Rock — **BLOCKED by RKNPU driver v0.8.2** (2026-05-08)
      - 3B model: fails to allocate 3.2 GB NPU memory
      - 270M model: loads (1.24s, 66 tok/s) but matmul w8a8 produces garbage
      - NPU hardware works — driver too old for correct w8a8 computation
- [x] **Flash Armbian 26.2.1** — Ubuntu 24.04 Noble, kernel 6.1.115,
      RKNPU v0.9.8 (2026-05-08). See `docs/07_IMAGE.md`.
- [x] Re-run `test_rkllm` on new image — **SUCCESS** (2026-05-08)
      - Llama-3.2-3B-Instruct: **4.49 tok/s**, 2.06s first token, 130s load
      - Coherent meditation text with proper [N] pause markers
      - Faster than CPU baseline (mistral.rs 3.89 tok/s) + frees CPU for TTS/STT
- [ ] **Rewrite `src/llm.rs`** — replace HTTP/SSE client with rkllm-rs direct calls
      - Remove all TCP/HTTP/SSE code
      - Load model once at thread start (like TTS/STT pattern)
      - `start_streaming()` spawns thread with `RkllmCallbackHandler`
      - Callback feeds tokens through `ChunkParser` → `LlmOutput` via mpsc
      - Keep `ChunkParser` (bracket state machine for `[N]` pause markers)
      - Keep `load_prompts()` for system/user prompt loading
      - Use Llama 3 chat template (already working in test_rkllm.rs)
- [ ] Wire into main.rs — same `llm_tx`/`llm_rx` channels, no API change
- [ ] Test end-to-end: → button → STT → LLM (NPU) → TTS → display
- [ ] Measure RAM: verify LLM (~4 GB) + STT (~1 GB) + TTS fit in 8 GB
- [ ] Remove mistral.rs from Rock (no HTTP server needed)
- [ ] Reduce model load time (130s is slow — investigate prompt caching,
      or keep model loaded across meditation sessions)
- [ ] Download Gemma-3-4B-IT .rkllm as alternative (5.62 GB, if RAM allows)
- [ ] Convert Ministral 3B to .rkllm via rkllm-toolkit on X61s (no pre-built exists)
- [ ] Benchmark Ministral 3B .rkllm vs Llama 3.2 3B .rkllm (meditation quality + tok/s)
- [ ] Compare meditation text quality: Llama 3.2 3B (NPU) vs Ministral 3B (CPU)
- [ ] Set up console font and dmesg suppression on new Armbian image
      (see `docs/01_DEV_SETUP.md` — setfont, dmesg -n 1)
- [ ] Install Piper TTS CLI on new image (`/usr/local/bin/piper`)
- [ ] Download Piper model to new image (`~/models/vits-piper-en_US-lessac-medium/`)
- [ ] Download SenseVoice RKNN model to new image (`~/models/sensevoice/`)
- [ ] Test full pipeline on new image: STT (NPU) → LLM (NPU) → TTS (CPU) → display
- [x] **Fix DSI display on Armbian image** (2026-05-08) — see `docs/08_DISPLAY.md`
      Panel IC is ILI9881C. Forked `panel-ilitek-ili9881c.ko` with correct
      init sequence (200 DCS commands) extracted from baseline image kernel
      via disassembly. Installed as `panel-radxa-display-8hd.ko` replacement.
      Uses stock `rock-5a-radxa-display-8hd` overlay unchanged.
- [ ] **Fix Uctronics audio codec on Armbian** — the onboard mic+speaker
      use a custom `uctronics,uctronics-codec` driver (not the es8316 headphone
      jack). Old image has `CONFIG_SND_SOC_UCTRONICS_CODEC=y` built into kernel
      plus `audio-codec-0` and `uctronics-sound` device tree nodes. Armbian
      doesn't have this driver. Needs: extract kernel module from baseline image
      (same approach as display fix), create DT overlay for the codec+sound nodes.
      GPIO pins: sdmode=GPIO3_A5, gainsel_1=GPIO3_A3, gainsel_2=GPIO3_A5,
      gainsel_3=GPIO3_A2. I2S bus: phandle 0x16f (find which i2s controller).
      **Workaround:** Use es8316 headphone jack (card 0, `plughw:0,0`) with
      external mic for testing until uctronics codec is fixed.
- [x] **Get TUI running on Armbian image** (2026-05-08)
      Rust toolchain installed, jhana-rs built, TUI running on DSI display.
      Piper TTS installed (`/usr/local/bin/piper` + espeak-ng-data symlink).
      SenseVoice STT model downloaded and loading. Console font set to
      TerminusBold 32x16. FB rotation (90 CW), dmesg suppression, and font
      persist via `/etc/rc.local`. Llama 3.2 3B `.rkllm` already on device.

**Step 3: TTS — fork piper-rs with candle + rknn-rs**
- [ ] Study piper-rs source (github.com/thewh1teagle/piper-rs)
- [ ] Fork piper-rs → piper-rknn-rs
- [ ] Rip out onnxruntime backend
- [ ] Add candle for CPU tensor ops (encoder, phoneme embedding)
- [ ] Add rknn-rs for NPU inference (VITS decoder)
- [ ] Download RKNN decoder from danielferr85/piper-checkpoints-rknn
- [ ] Wire: text → espeak-ng phonemes → encoder (CPU) → decoder (NPU) → WAV
- [ ] Benchmark RTF on NPU (expect ~0.15 vs CPU ~0.31)
- [ ] Rewrite `src/tts.rs` — replace Piper CLI with piper-rknn-rs
- [ ] Test end-to-end: STT (NPU) → LLM (NPU) → TTS (NPU)
- [ ] Publish crate (contribute to Rust RKNN ecosystem)

## Known bugs

- [ ] **TUI text races ahead of TTS audio — they appear out of sync.**
      Root cause: the LLM streams sentences via mpsc and `main.rs` dispatches
      each `Sentence` to both TUI (`app.push_sentence`) and TTS
      (`TtsCommand::Speak`) immediately (lines 190-191). The TUI displays
      sentences the instant they arrive, but TTS processes them sequentially
      (Piper synth ~0.8s + aplay ~2.5s per sentence). At ~4 tok/s the LLM
      produces sentences far faster than TTS can speak them, so by the time
      the user reads sentence 5 on screen the speaker is still on sentence 1.
      They are the **same meditation** but completely desynchronized.
      **Fix:** Gate TUI display on TTS playback. Options:
      1. **TTS-paced display:** Don't call `app.push_sentence` until TTS
         confirms the sentence has started (or finished) playing. Add a
         `TtsEvent::Playing(sentence)` feedback channel from TTS → main loop.
      2. **Buffer and reveal:** Buffer LLM sentences in a queue, reveal each
         one on TUI only when TTS is ready to play it. This naturally paces
         the visual display to match the spoken cadence.
      3. **Typewriter sync (future):** The planned typewriter text reveal
         effect (TODO item in TUI visual upgrade) should sync character
         reveal speed to TTS speech rate, solving both display pacing and
         the visual aesthetic goal at once.

- [ ] **Audio clicks/pops after every spoken sentence.** Each `aplay` call
      in `src/tts.rs` opens and closes the ALSA device, causing an audible
      click/pop on the Uctronics speaker when the device powers down between
      sentences. This breaks the calm meditation experience.
      **Fix options:**
      1. **Keep ALSA open:** Instead of spawning `aplay` per sentence, open
         the ALSA device once when the TUI starts (e.g. via `rodio` or raw
         ALSA fd) and stream all WAV data through it. Close only on TUI exit.
         Silence between sentences keeps the device active without pops.
      2. **Play silence padding:** Append a short silence (50-100ms) to each
         WAV before closing, or play a looping silent stream in the background
         while the TUI is active.
      3. **Use `aplay --process-id-file`** with a persistent ALSA connection
         that stays open across sentences.

## 4. Orchestration (Phase 1 complete pipeline)

- [ ] Wire stdin prompt -> LLM -> TUI + TTS pipeline
- [ ] ENTER button (→) triggers meditation from predefined prompt
- [ ] BACK button (←) cancels current generation
- [ ] Crossbeam channels between LLM thread and main/TTS
- [ ] Graceful shutdown: stop LLM generation on quit
- [ ] End-to-end test: prompt -> displayed text + WAV files on disk
- [ ] Measure end-to-end latency (target: first sentence <3s)
- [ ] Measure peak memory (target: <6 GB total)
- [ ] Release build and benchmark on Rock

---

## Phase 2: Voice Input

### 2a. Rebuild sherpa-onnx with RK3588 NPU support

sherpa-onnx-sys downloads CPU-only prebuilt libs. We need to build from
source with `-DSHERPA_ONNX_ENABLE_RKNN=ON` so VAD/STT/TTS can use the
6 TOPS NPU. See `docs/04_STT.md` for engine decisions.

- [x] Install librknnrt.so v2.2.0 to `/usr/lib` on Rock (2026-05-07)
- [x] Install RKNN C headers (`rknn_api.h` etc.) to `/usr/include` (2026-05-07)
- [x] Clone sherpa-onnx v1.13.0 on Rock (`/home/ubuntu/sherpa-onnx-rknn`)
- [x] Build sherpa-onnx with RKNN support (static libs, release) (2026-05-07)
- [x] Verify `.a` static libs produced in `build/install/lib/` (2026-05-07)
- [ ] Update `rock-build.sh` to set `SHERPA_ONNX_LIB_DIR=/home/ubuntu/sherpa-onnx-rknn/build/install/lib`
- [ ] Rebuild jhana-rs with RKNN-enabled libs, verify it links
- [x] Set `provider: Some("rknpu".into())` in TTS config (2026-05-07)
  - **Finding:** `rknpu` is not a valid provider string — sherpa-onnx
    RKNN uses a separate code path with `.rknn` models, not the
    `provider` field. Reverted to `cpu`. See `docs/05_NPU.md`.
- [x] Benchmark TTS latency: CPU vs NPU on same sentence (2026-05-07)
  - Result: both identical (RTF ~0.31), `rknpu` falls back to CPU
  - CPU is adequate: 3x real-time for meditation TTS
- [x] Rebuild jhana-rs with RKNN-enabled libs, verify it links (2026-05-07)
  - Needs: `RUSTFLAGS='-L /usr/lib -l rknnrt'` and
    `SHERPA_ONNX_LIB_DIR=/home/ubuntu/sherpa-onnx-rknn/build/install/lib`
- [ ] Update `rock-build.sh` to set `SHERPA_ONNX_LIB_DIR` and `RUSTFLAGS`

Script: `scripts/rock-build-sherpa-rknn.sh`

### 2a-alt. NPU acceleration: next steps

sherpa-onnx RKNN works via `.rknn` format models (not the `provider`
field). Pre-converted models exist for STT/VAD. TTS and LLM need
separate solutions. See `docs/05_NPU.md` for full details.

#### Option A: Download pre-converted RKNN models for STT/VAD

Pre-converted `.rknn` models from [k2-fsa/sherpa-onnx releases](https://github.com/k2-fsa/sherpa-onnx/releases/tag/asr-models):

- [ ] Download [SenseVoice RKNN](https://github.com/k2-fsa/sherpa-onnx/releases/tag/asr-models)
  (`sherpa-onnx-rk3588-20-seconds-sense-voice-zh-en-ja-ko-yue-2025-09-09`)
  to `/home/ubuntu/models/` on Rock
- [ ] Download Silero VAD RKNN model (may be bundled with SenseVoice)
- [ ] Test STT with RKNN model path in sherpa-onnx Rust config
- [ ] Benchmark STT latency: CPU (.onnx) vs NPU (.rknn) on same audio

#### Option B: Convert ONNX models to RKNN on X61s

[rknn-toolkit2](https://github.com/airockchip/rknn-toolkit2) converts
ONNX -> RKNN format. **x86_64 only** — runs on X61s, not Rock.

- [ ] Install `rknn-toolkit2` Python package on X61s:
  `pip install rknn-toolkit2` (requires Python 3.8-3.10, Ubuntu 22.04)
- [ ] Convert Piper VITS decoder ONNX -> RKNN for TTS NPU
  (or download pre-converted from [danielferr85/piper-checkpoints-rknn](https://huggingface.co/danielferr85/piper-checkpoints-rknn))
- [ ] Convert Moonshine tiny ONNX -> RKNN for STT NPU
- [ ] SCP converted `.rknn` models to Rock `/home/ubuntu/models/`

#### Option C: RKLLama — LLM on NPU

[RKLLama](https://github.com/NotPunchnox/rkllama) is an Ollama-compatible
server wrapping Rockchip's RKLLM runtime. OpenAI-compatible HTTP API —
drop-in replacement for mistral.rs in `src/llm.rs`.

- [ ] Install RKLLama on Rock
- [ ] Download a supported `.rkllm` model
  ([HuggingFace RKLLM model zoo](https://huggingface.co/models?search=rkllm)):
  Qwen2 0.5B, TinyLlama 1.1B, or DeepSeek-R1-Distill-Qwen-1.5B
- [ ] Test via `localhost:port/v1/chat/completions`
- [ ] Benchmark: tokens/sec vs mistral.rs CPU (~3.89 tok/s for 3B)
- [ ] If viable: update `SERVER_ADDR` in `src/llm.rs`

Expected: ~10-15 tok/s for 1.1B on NPU. Smaller model but faster.

#### Option D: Paroli — streaming TTS on NPU

[Paroli](https://github.com/marty1885/paroli) runs the Piper VITS decoder
on the RK3588 NPU. **4.3x speedup** over CPU (RTF ~0.15 vs ~0.31).

- [ ] Build Paroli on Rock with `-DUSE_RKNN=ON`
- [ ] Get RKNN decoder model (convert on X61s or download from
  [danielferr85/piper-checkpoints-rknn](https://huggingface.co/danielferr85/piper-checkpoints-rknn))
- [ ] Benchmark vs sherpa-onnx TTS CPU
- [ ] If faster: integrate as TTS backend (CLI pipe or FFI)

#### Option E: sensevoice-rs — pure Rust STT+VAD with RKNN

[sensevoice-rs](https://crates.io/crates/sensevoice-rs) (v0.1.7) is a
pure Rust SenseVoice implementation using Candle ML framework. Native
RKNN backend via `rknn-rs`. Built-in FSMN-VAD. Could replace sherpa-onnx
for STT+VAD entirely.

**API:** `SenseVoiceSmall::init_with_config(config, vad_config)` ->
`svs.infer_file(path)` or `svs.infer_vec(samples, rate)` ->
`Vec<VoiceText>` with language, emotion, event, content fields.

**Model:** [`happyme531/SenseVoiceSmall-RKNN2`](https://huggingface.co/happyme531/SenseVoiceSmall-RKNN2)
(~973 MB: encoder.rknn + tokenizer + VAD + embeddings).

**Deps:** candle-core 0.9.2, candle-nn, hf-hub, sentencepiece, ndarray,
rknn-rs 0.2.4 (heavy dependency tree).

- [x] Add `sensevoice-rs` to `Cargo.toml` with `rknpu` feature (2026-05-07)
- [x] Create `src/bin/test_stt.rs` test binary (2026-05-07)
- [ ] `cargo check` on X61s — verify type-checking passes
- [ ] Download RKNN model to Rock (`/home/ubuntu/models/sensevoice/`)
- [ ] Build and run `test_stt` on Rock
- [ ] Benchmark STT latency: CPU (Candle) vs NPU (RKNN) on same audio
- [ ] Compare API ergonomics vs sherpa-onnx Rust bindings

#### Option F: rknpu2 crate — direct RKNN Runtime from Rust

[rknpu2](https://crates.io/crates/rknpu2) (v0.0.9) provides Rust
bindings for `librknnrt.so`. Load any `.rknn` model and run inference
directly — no C++ wrapper needed. Could be used to build custom
TTS/STT/LLM inference.

- [ ] Evaluate API for loading and running `.rknn` models
- [ ] Test with a simple model (e.g. Silero VAD)

### 2b. STT + VAD integration

Decision (2026-05-07): sherpa-onnx Silero VAD + Moonshine tiny int8.
See `docs/04_STT.md`. **Update:** SenseVoice RKNN may be better —
pre-converted for RK3588 NPU, includes English, tested by k2-fsa.

- [ ] Download SenseVoice RKNN model to Rock (see Option A above)
- [ ] Download Moonshine tiny int8 as CPU fallback
- [ ] Verify Silero VAD model on Rock (`snakers4_silero-vad_master/`)
- [ ] Create `src/stt.rs` — VAD + STT via sherpa-onnx (same thread+mpsc pattern)
- [ ] Mic capture: cpal (ALSA, 16 kHz mono) or arecord shell-out
- [ ] Wire: ENTER -> mic -> VAD -> STT -> transcribed text -> LLM user prompt
- [ ] LLM selects meditation type from natural language request
- [ ] Pin audio capture to Cortex-A55 cores (0-3)
- [ ] Benchmark STT latency on Rock (target: <2s for 5s utterance)

## Phase 3: Voice Output (future)

- [ ] Add rodio dependency for audio playback
- [ ] Play Piper WAV files through speaker
- [ ] Sentence queue: LLM -> TTS -> playback in order
- [ ] Mute mic during playback (feedback suppression)
- [ ] Configure PulseAudio devices (mirror configure_devices.sh)

### 3a. Voice cloning TTS

Replace the robotic Piper lessac voice with a cloned voice using the
reference WAV files in `audio/voice-cloning/`:

| File | Duration | Source |
|------|----------|--------|
| `voice-to-clone.wav` | 1.7 MB, 16-bit stereo 44100 Hz | Primary reference voice |
| `voice-to-clone-batman.wav` | 6.3 MB, 16-bit stereo 48000 Hz | Batman character voice |
| `voice-to-clone-tom-waits.wav` | 21 MB, 16-bit stereo 48000 Hz | Tom Waits style voice |
| `voice-to-clone-morgan-freeman.wav` | 16 MB, 16-bit stereo 48000 Hz | Morgan Freeman style voice |

These were used with Coqui XTTS v2 in the Python prototype (`jhana-dev`).

#### Rust voice cloning TTS options (surveyed 2026-05-08)

Reference voice WAV files are in `audio/voice-cloning/` (4 voices from
the Python prototype). "Zero-shot" = no training needed, just a reference
WAV at inference time (misleading term — it means zero *training* steps,
not zero audio samples).

**Top candidates for RK3588 aarch64 (8 GB RAM):**

| Crate | Voice cloning | Backend | aarch64 | Stars | Notes |
|-------|--------------|---------|---------|-------|-------|
| [`qwen_tts`](https://github.com/TrevorS/qwen3-tts-rs) v0.4.0 | Yes (x-vector + ICL) | Candle (no Python) | Confirmed (Docker CI) | 126 | **Top pick.** 0.6B/1.7B models. Pure Rust. |
| [`qwen3_tts`](https://github.com/second-state/qwen3_tts_rs) v0.2.2 | Yes (ICL mode) | libtorch (~800 MB) | Confirmed (ARM64 bins) | 208 | Most mature. `voice_clone` CLI. |
| [`fish-speech.rs`](https://github.com/EndlessReform/fish-speech.rs) | Yes (ref audio) | Candle | CPU fallback | 110 | ~15 MB binary. OpenAI API. |
| [`vox`](https://github.com/mrtozner/vox) v0.6.0 | Yes (Chatterbox) | sherpa-onnx/ONNX | RPi4+ supported | 32 | Multi-engine (Kokoro, Qwen3, Piper). |
| [`rwkv-tts-rs`](https://github.com/cgisky1980/rwkv-tts-rs) v0.2.5 | Yes (ref audio) | wgpu (Vulkan) | Mali-G610 Vulkan | 26 | RWKV arch. Under-documented. |
| [`voxtral-tts-rs`](https://github.com/second-state/voxtral_tts_rs) v0.1.1 | Yes (ref audio) | libtorch | ARM64 bins | 7 | Mistral 4B — tight on 8 GB RAM. |

**Not suitable for voice cloning:**

| Crate | Why not |
|-------|---------|
| `kitten_tts_rs` (288 stars) | Fixed voices only, no cloning. Great for edge TTS though. |
| `kokoros` / `kokoroxide` | Preset voice styles, not ref-audio cloning. |
| `piper-rs` / Piper CLI | Fixed voices. Cloning requires full model retraining. |
| `coqui-tts` (Rust) | PyO3 wrapper, stale (2023), Coqui AI shut down 2024. |
| F5-TTS RKNN | 39x real-time on NPU — too slow. Python only. |

#### Option G: qwen_tts — Qwen3-TTS voice cloning in Candle (PREFERRED)

[qwen_tts](https://crates.io/crates/qwen_tts) v0.4.0 — pure Rust,
Candle-based, no Python, aarch64 confirmed. ICL mode takes a 3-second
reference WAV + transcript and clones the voice. x-vector mode is faster
but lower quality. Models: 0.6B (lighter) or 1.7B (better quality).

- [ ] Add `qwen_tts` to Cargo.toml
- [ ] Create `src/bin/test_voice_clone.rs` — clone from reference WAV
- [ ] Test with `audio/voice-cloning/voice-to-clone.wav` (primary voice)
- [ ] Test with Morgan Freeman and Batman voices for variety
- [ ] Benchmark inference speed on Rock 5A (CPU) with 0.6B model
- [ ] Test 1.7B model if RAM allows alongside STT + LLM
- [ ] Compare voice quality vs Piper lessac (robotic baseline)
- [ ] If quality is good: replace Piper in `src/tts.rs` with qwen_tts
- [ ] Investigate ONNX export path for RKNN NPU acceleration

#### Option H: fish-speech.rs — Fish Speech voice cloning in Candle

[fish-speech.rs](https://github.com/EndlessReform/fish-speech.rs) —
Candle-based, ~15 MB binary, OpenAI-compatible API, supports Fish Speech
1.2/1.4/1.5. Voice cloning via reference audio encoding.

- [ ] Clone and build fish-speech.rs on Rock
- [ ] Test voice cloning with reference WAVs
- [ ] Benchmark latency vs qwen_tts

#### Option I: vox — multi-engine TTS framework

[vox](https://crates.io/crates/vox) v0.6.0 — framework supporting
Chatterbox (voice cloning, 350 MB model), Kokoro, Qwen3, Piper, Pocket
TTS. RPi4+ supported. Heavier but covers many backends.

- [ ] Evaluate vox Chatterbox backend for voice cloning quality
- [ ] Benchmark on Rock 5A

#### Option J: Piper fine-tuning with custom voice

Train a custom Piper VITS voice using the reference WAV files. Keeps the
existing Piper pipeline (fast, lightweight) but with a personalized voice.
Not instant cloning — requires training.

- [ ] Research Piper voice training workflow (requires aligned text+audio)
- [ ] Create training dataset from reference WAVs (transcribe + align)
- [ ] Fine-tune Piper VITS model on X61s or cloud GPU
- [ ] Export fine-tuned model as ONNX, deploy to Rock
- [ ] Optionally convert to RKNN for NPU inference

#### Option K: rknpu2 + custom RKNN TTS model

Use the [`rknpu2`](https://crates.io/crates/rknpu2) crate (v0.0.9,
`rk35xx` feature) to load a pre-converted `.rknn` TTS decoder model
directly. No voice cloning — fixed Piper voices only, but with NPU
acceleration (4.3x speedup).

- [ ] Add `rknpu2 = { version = "0.0.9", features = ["rk35xx"] }` to Cargo.toml
- [ ] Download Piper RKNN decoder from [danielferr85](https://huggingface.co/danielferr85/piper-checkpoints-rknn)
- [ ] Load `.rknn` model via `rknpu2` API, run inference on a test input
- [ ] Benchmark TTS latency: rknpu2 direct vs Piper CLI CPU (RTF ~0.31)
- [ ] If viable: integrate as alternative TTS backend in `src/tts.rs`

## Demo Recording

Capture TUI screen + audio (TTS output / mic input) for project demos.

### Tools

| Tool | Purpose | Output |
|------|---------|--------|
| [VHS](https://github.com/charmbracelet/vhs) | Scripted terminal recording via `.tape` files | GIF, MP4, WebM |
| [asciinema](https://asciinema.org/) | Terminal session recorder (Rust-based v3) | `.cast` files |
| [agg](https://github.com/asciinema/agg) | Convert `.cast` -> optimized GIF (gifski) | GIF |

### Recommended approach

1. **TUI screen**: Use **VHS** with a `.tape` script — supports interactive
   TUI apps (arrow keys, timing, Wait for render). Produces GIF/MP4 for
   GitHub README. Scriptable and reproducible.

2. **Audio capture**: VHS/asciinema capture terminal output only, not audio.
   For audio, record separately with `arecord` (ALSA) or `ffmpeg` and sync
   in post with video editing. Or use `ffmpeg` to capture both screen
   (framebuffer) and audio simultaneously:
   ```bash
   ffmpeg -f fbdev -i /dev/fb0 -f alsa -i hw:2,0 -t 60 demo.mp4
   ```

3. **Combined demo**: Record TUI via VHS (`.tape` -> MP4), record audio via
   ALSA (`arecord -D hw:2,0 -f cd -d 60 audio.wav`), merge with ffmpeg:
   ```bash
   ffmpeg -i tui.mp4 -i audio.wav -c:v copy -c:a aac demo.mp4
   ```

### Scripts

- `scripts/rock-demo-record.sh [duration]` — start recording + TUI
- `scripts/rock-demo-stop.sh` — stop recording, list demos

### TODO

- [x] Install ffmpeg on Rock
- [x] Create `rock-demo-record.sh` (framebuffer + ALSA -> MP4)
- [x] Create `rock-demo-stop.sh` (clean stop + list demos)
- [ ] Test framebuffer capture with `ffmpeg -f fbdev`
- [ ] Test audio capture from Uctronics speaker (card 2)
- [ ] Produce first demo MP4 with audio
- [ ] Extract GIF from MP4 for README (`ffmpeg -i demo.mp4 -vf fps=10 demo.gif`)
- [ ] Produce polished demo for project showcase

## Packaging

### Crates.io publication

Prepare jhana-rs for publication as a Rust crate on crates.io.

- [ ] Review and update `Cargo.toml` metadata (description, license, repository,
  keywords, categories, readme, homepage)
- [ ] Add `license-file` or ensure `LICENSE` file exists at repo root
- [ ] Audit `[dependencies]` — ensure all are published on crates.io
- [ ] Add `exclude` patterns in `Cargo.toml` (prompts/, models/, scripts/, docs/)
- [ ] Run `cargo publish --dry-run` to validate
- [ ] Update `cargo doc` output — ensure public API docs are clean
- [ ] Publish initial version (0.1.0)

---

## Phase 4: Hardware Integration (future)

- [ ] Slint graphical display (DRM/KMS, no GPU) — larger meditation text font
- [ ] Serial output to /dev/ttyS6 at 115200 baud
- [ ] systemd service (jhana-rs.service)
- [ ] Boot-to-app: auto-start on power on
- [ ] **Add eMMC module to Rock 5A** — currently booting from microSD (slower).
      Research compatible eMMC modules for the Rock 5A socket (mmc0, sdhci-dwcmshc
      fe2e0000). The Rock 5A supports eMMC 5.1. Options:
      - Radxa official eMMC modules (16/32/64/128 GB) from radxa.com
      - Generic eMMC 5.1 modules with B2B connector (check pinout compatibility)
      - Expected speed improvement: eMMC ~300 MB/s sequential vs microSD ~100 MB/s
      - Faster model loading (4.35 GB Llama model), faster boot, more reliable
      - Can boot from eMMC and use microSD for extra storage or backup

---

## Models on device

| Model | Path | Size | Used in |
|-------|------|------|---------|
| Orca Mini 3B Q4_0 | `/home/ubuntu/ai_in_a_box/downloaded/orca-mini-3b.ggmlv3.q4_0.bin` | 1.8 GB | Phase 1 LLM |
| Piper en_US-lessac-low | `/home/ubuntu/ai_in_a_box/downloaded/en_US-lessac-low.onnx` | 61 MB | Phase 1 TTS |
| Piper en_US-lessac-medium | `/home/ubuntu/models/vits-piper-en_US-lessac-medium/` | ~60 MB | TTS (current) |
| Ministral 3B Q4_K_M | `/home/ubuntu/models/Ministral-3B-Instruct-Q4_K_M.gguf` | ~2 GB | LLM CPU (current) |
| SenseVoice RKNN | `/home/ubuntu/models/sensevoice/` | ~973 MB | STT NPU |
| Llama-3.2-3B-Instruct | `/home/ubuntu/models/Llama-3.2-3B-Instruct_w8a8_g128_rk3588.rkllm` | 4.35 GB | LLM NPU (testing) |
| Silero VAD | `/home/ubuntu/ai_in_a_box/downloaded/snakers4_silero-vad_master/` | ~2 MB | Phase 2 VAD |

---

## Documentation

- [ ] **Explain the "cyberbox" concept in README or docs** — the Uctronics
      AI in a Box hardware is a "cyberbox": a self-contained, offline,
      single-purpose AI device. Cite the source of the cyberbox idea and
      how jhana-rs transforms it from a captioning device into a meditation
      guide. Document the vision: a dedicated physical object for meditation,
      not an app on a phone/laptop.

## Cross-project documentation

- [ ] Review ALL Rust crates evaluated in this project (sensevoice-rs, rkllm-rs,
      rknn-rs, rknpu2, piper-rs, qwen_tts, fish-speech.rs, rwkv-tts-rs, vox,
      kitten_tts_rs, kokoros, sherpa-onnx, any-tts, voxtral-tts-rs, etc.) and
      add any missing ones to `~/projects/r4ds` in the appropriate docs/resources
      sections (ML/AI, TTS, STT, LLM inference, NPU/embedded).
- [ ] Add key Rust crate/library discovery sources to `~/projects/r4ds` docs:
      - [crates.io](https://crates.io) — official Rust package registry
      - [crates.io/keywords/text-to-speech](https://crates.io/keywords/text-to-speech)
      - [crates.io/keywords/speech-recognition](https://crates.io/keywords/speech-recognition)
      - [crates.io/keywords/machine-learning](https://crates.io/keywords/machine-learning)
      - [crates.io/keywords/llm](https://crates.io/keywords/llm)
      - [crates.io/keywords/neural-network](https://crates.io/keywords/neural-network)
      - [lib.rs](https://lib.rs) — alternative frontend, better search/categorization
      - [lib.rs/search?q=tts](https://lib.rs/search?q=tts)
      - [lib.rs/search?q=voice+cloning](https://lib.rs/search?q=voice+cloning)
      - [docs.rs](https://docs.rs) — auto-generated API docs for every crate
      - [Are We Learning Yet?](https://www.arewelearningyet.com/) — Rust ML ecosystem tracker
      - [HuggingFace Candle](https://github.com/huggingface/candle) — Rust ML framework, includes TTS/STT examples
