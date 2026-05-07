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
- [ ] Pause marker countdown timer in TUI (scaffolded, activates with LLM)

## 2. LLM Integration (llama-cpp-rs)

Model on device: `/home/ubuntu/ai_in_a_box/downloaded/orca-mini-3b.ggmlv3.q4_0.bin` (1.8 GB)

- [ ] Add llama-cpp-rs dependency to Cargo.toml
- [ ] Verify llama-cpp-rs builds on aarch64 (Risk: C++ cross-dep)
- [ ] Create `src/llm.rs` module
- [ ] Load Orca Mini 3B Q4_0 model
- [ ] Configure inference params (top_k=40, top_p=0.95, temp=0.25, repeat_penalty=1.1)
- [ ] System prompt for meditation guide with [pause] markers
- [ ] Streaming token output via crossbeam channel
- [ ] Sentence-level buffering (accumulate tokens until sentence boundary)
- [ ] Parse [pause_duration] markers from LLM output
- [ ] Define LlmOutput enum (Sentence, Pause, Done)
- [ ] Pin LLM thread to Cortex-A76 cores (cores 4-7) for performance
- [ ] Wire LLM output to TUI display (live streaming)
- [ ] Text prompt input via stdin or TUI
- [ ] Measure tokens/second on Rock, verify >2.5 tok/s
- [ ] Memory profiling: verify LLM stays within budget (~1.8 GB)
- [ ] Tests for sentence buffering and pause marker parsing
- [ ] Rustdoc for llm module

## 3. TTS Integration (Piper CLI)

Piper installed: `/usr/local/bin/piper`
Model on device: `/home/ubuntu/ai_in_a_box/downloaded/en_US-lessac-low.onnx` (61 MB)

- [ ] Create `src/tts.rs` module
- [ ] Shell out to Piper via std::process::Command
- [ ] Generate WAV file per sentence
- [ ] Configure speech rate via --length_scale (default 1.2)
- [ ] Output WAV files to a temp directory
- [ ] Wire to LLM pipeline: each Sentence -> Piper -> WAV
- [ ] Skip TTS for Pause markers (just wait)
- [ ] Error handling for Piper subprocess failures
- [ ] Measure TTS latency per sentence on Rock
- [ ] Tests for TTS command construction
- [ ] Rustdoc for tts module

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

## Phase 2: Voice Input (future)

- [ ] Add cpal dependency (ALSA backend)
- [ ] Create `src/audio.rs` — mic capture (16 kHz mono i16)
- [ ] Create `src/vad.rs` — webrtc-vad for voice activity detection
- [ ] Create `src/stt.rs` — whisper-rs (Whisper tiny.en, ~75 MB)
- [ ] Verify whisper-rs builds on aarch64
- [ ] Wire: mic -> VAD -> STT -> text prompt -> Phase 1 pipeline
- [ ] Pin audio capture to Cortex-A55 cores (0-3)

## Phase 3: Voice Output (future)

- [ ] Add rodio dependency for audio playback
- [ ] Play Piper WAV files through speaker
- [ ] Sentence queue: LLM -> TTS -> playback in order
- [ ] Mute mic during playback (feedback suppression)
- [ ] Configure PulseAudio devices (mirror configure_devices.sh)

## Phase 4: Hardware Integration (future)

- [ ] Slint graphical display (DRM/KMS, no GPU)
- [ ] Serial output to /dev/ttyS6 at 115200 baud
- [ ] systemd service (jhana-rs.service)
- [ ] Boot-to-app: auto-start on power on

---

## Models on device

| Model | Path | Size | Used in |
|-------|------|------|---------|
| Orca Mini 3B Q4_0 | `/home/ubuntu/ai_in_a_box/downloaded/orca-mini-3b.ggmlv3.q4_0.bin` | 1.8 GB | Phase 1 LLM |
| Piper en_US-lessac-low | `/home/ubuntu/ai_in_a_box/downloaded/en_US-lessac-low.onnx` | 61 MB | Phase 1 TTS |
| Silero VAD | `/home/ubuntu/ai_in_a_box/downloaded/snakers4_silero-vad_master/` | ~2 MB | Phase 2 VAD |
| Whisper tiny.en | (needs download or copy from Python env) | ~75 MB | Phase 2 STT |
