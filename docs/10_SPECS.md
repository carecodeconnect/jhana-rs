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

### Existing Rust voice-agent frameworks (discovered 2026-05-15)

After web search for our use-case shape (offline Rust voice agent
with STT + LLM + TTS on embedded Linux), three existing projects
turn out to overlap heavily with what we'd otherwise hand-build:

| Project | Stack | Pipeline | Fit for jhana-rs |
|---------|-------|----------|------------------|
| **[Vox](https://github.com/mrtozner/vox)** | Pure Rust | Mic → Silero VAD → Whisper/Sherpa STT → speaker ID → user code → Kokoro/Piper/Qwen3 TTS → speaker | **Closest direct match.** Pipeline shape is exactly our target. Pluggable STT/TTS backends. Worth reading even if we don't adopt — the trait surface is a model for our own. |
| **[Feros](https://dev.to/loopbreaker111/we-open-sourced-our-production-voice-ai-stack-rust-runtime-sub-second-latency-3gb9)** | Rust + Tokio | VAD → STT → LLM → TTS, hot path entirely Rust, sub-second latency target | "Voice Agent OS"; orchestration is in Rust on `tokio`. The async-streaming architecture we sketched above is essentially their production stack. |
| **[EchoKit](https://github.com/second-state/echokit_box)** | Rust + ESP32 | VAD, ASR, LLM orchestration, TTS for embedded MCUs | Targets ESP32 not RK3588, but the ergonomics of "voice toolkit for a tiny embedded device" overlap with our cyberbox aesthetic. |
| **[Kalosm](https://github.com/floneum/floneum)** (kalosm-0.4, Feb 2025) | Rust + Candle | LLM + Whisper STT + async streaming; **no built-in TTS, no clear tool-call story** | Less direct fit; more of a "local-first ML primitives" library than an agent framework. Useful for understanding how `Candle`-based async streaming is shaped, but doesn't replace our pipeline. |
| **[Goose](https://github.com/block/goose)** (Block / Square) | Rust + TypeScript | Agent + tool-calling via Model Context Protocol (MCP); 15+ LLM providers incl. **Ollama** for local | Production AI agent with desktop apps. If we want a generic agent that speaks MCP we plug rkllm-rs behind an Ollama-compatible endpoint and reuse Goose's 70+ MCP extensions. Heavier than what we need, but the MCP-as-tool-protocol pattern is portable. |

Implication for the architecture:

- We do **not** need to handroll the agent runtime if we don't want
  to — **Vox or Feros could be the spine**, with `rkllm-rs` plugged
  in as the LLM provider and `sensevoice-rs` + `espeak-ng` →
  `piper-rs` as the STT/TTS implementations. That gets us a
  tokio-async pipeline for ~free.
- If we still want a Pi-style higher-level agent on top
  (tool calls, meditation-template retrieval, repair sequences from
  NCF), Goose-via-MCP is a single-binary alternative to a separate
  Node `pi-mono` install.
- A **bespoke ratatui + tokio** build (option in this section's
  header) remains viable too — Vox's traits are a good crib sheet
  for what to expose, and we already have ratatui + rkllm-rs +
  sensevoice-rs working.

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

### Streaming pipeline (STT → LLM → TTS) and async architecture

The current jhana-rs pipeline is **per-utterance batched** with
`std::thread + mpsc::channel`:

1. `arecord` → `/tmp/jhana_stt.wav` (5 s native S32_LE 48 kHz)
2. `ffmpeg` resample → `/tmp/jhana_stt_16k.wav`
3. `svs.infer_file()` → full transcript
4. `LlmStreamingThread` streams sentences over `mpsc::Sender<LlmOutput>`
5. Each sentence → `espeak-ng` → `/tmp/jhana_tts.wav` → `aplay`

This works but has two costs:

- **STT is blocking**: no captions are visible while the user is
  speaking; the screen is empty until the full 5 s clip is recorded
  *and* SenseVoice returns.
- **No backpressure**: if `aplay` lags the LLM, sentences pile up in
  the channel buffer.

#### Target: tokio + `Stream`-based pipeline

```
mic-stream  ▶  STT-stream  ▶  text-stream  ▶  LLM-stream  ▶  sentence-stream  ▶  TTS-stream  ▶  audio-stream  ▶  aplay
```

Each stage is a `futures::Stream`; downstream backpressure flows
backwards via the stream contract. Implementation switches from
`std::thread` to `tokio::task::spawn`, and from `mpsc::channel` to
`tokio::sync::mpsc` + `tokio_stream::wrappers::ReceiverStream`.

This shape matches:

- **[Rig](https://www.rig.rs)** — Rust, sync trait `Stream` based.
  Each agent step exposes `Stream<Item = T>`. STT yields partial
  transcripts, LLM yields tokens, TTS yields audio chunks.
  Single-binary fit; natural pair with `rkllm-rs` for the LLM stage.
- **[LangGraph](https://github.com/langchain-ai/langgraph)** —
  Python directed graph of async-iterator nodes (`astream_events`).
  Battle-tested; brings Python.
- **[pi-agent-core](https://github.com/earendil-works/pi-mono/tree/main/packages/agent)**
  — TS/Node, async generator tools via Node Streams API. Matches
  the Pi harness path documented below.

For an offline cyberbox we prefer **Rig** (Rust, single binary,
zero daemon) for the in-Rust streaming layer, while keeping the
door open for `pi-agent-core` as the on-top harness if/when we
adopt the Node agent runtime.

#### Captioning during STT (streaming ASR)

The current SenseVoiceSmall encoder is non-causal — it needs the
entire utterance before emitting tokens, so live captions aren't
possible with it. Two viable swaps for streaming STT on the
RK3588 NPU:

- **[Moonshine streaming](https://github.com/usefulsensors/moonshine)**
  — Useful Sensors' efficient streaming ASR. Tiny (~27 M params),
  much faster than Whisper-tiny at the same accuracy. RKNN
  conversions are available; quality is benchmark-comparable.
  Pairs naturally with the streaming-pipeline target above.
- **online-zipformer** via RKNN — referenced in
  [05_NPU.md](05_NPU.md) under sherpa-onnx; needs `.rknn` model
  conversion. Lower-level option if Moonshine doesn't fit.

User-visible win: a live caption strip appears under the mic icon
during recording, both as a UX improvement and as a debugging
surface (the user can see when the model mis-transcribes them and
re-press immediately, rather than wait the 5 + 7 s for SenseVoice
batch output).

Plan:

1. Land the tokio + `Stream` refactor while keeping SenseVoice as
   the final-pass ASR (so we keep current accuracy).
2. Add Moonshine streaming as a second ASR stage that emits partial
   captions to the TUI's caption strip.
3. When the user releases the button (or VAD detects silence),
   commit the final SenseVoice result as the canonical transcript
   and feed it to the LLM.

### Pi adaptation plan (2026-05-15, after looking at the actual repos)

`pi` lives locally at `/mnt/data/projects/pi`
([earendil-works/pi-mono](https://github.com/earendil-works/pi-mono)). It
is a TypeScript/Node monorepo with several useful packages:

- `@earendil-works/pi-agent-core` — agent runtime with tool calling and
  state management
- `@earendil-works/pi-coding-agent` — interactive CLI built on it
- `@earendil-works/pi-ai` — unified multi-provider LLM API (OpenAI,
  Anthropic, Google, anything OpenAI-compatible)
- `@earendil-works/pi-tui` — differential-rendering terminal UI library
  with bracketed paste, image rendering, autocomplete

`/mnt/data/projects/pi_sandbox` is a tested setup that runs pi against a
local OpenAI-compatible server (llama.cpp) on Apple M1 Max with
Qwen3-Coder 30B / 80B and gpt-oss-20b. Decode at 50–60 tok/s on M1 Max.

#### Proposed Rock architecture

```
┌─────────────────────────────────────────────────────────────────┐
│                          Rock 5A                                │
│                                                                 │
│  ┌────────────────┐    HTTP /v1/...    ┌────────────────────┐   │
│  │ pi (Node TUI)  │ ─────────────────▶ │ jhana-rs-server    │   │
│  │ on tty1        │ ◀───────────────── │ (Rust, single bin) │   │
│  │ - agent loop   │   tool calls       │ - rkllm-rs (NPU)   │   │
│  │ - skills/tools │                    │ - sensevoice-rs    │   │
│  │ - meditation   │                    │ - piper-rs (later) │   │
│  │   templates    │                    │ - ALSA capture/    │   │
│  └────────────────┘                    │   playback         │   │
│         ▲                              └────────────────────┘   │
│         │ DSI display                            ▲              │
│         │ keyboard / hardware buttons            │ I2S          │
│         ▼                                        ▼              │
│   ┌─────────────┐                       ┌────────────────┐      │
│   │ tty1 + xx16 │                       │ Uctronics      │      │
│   │ Terminus    │                       │ mic + speaker  │      │
│   │ font        │                       │ (codec card 1) │      │
│   └─────────────┘                       └────────────────┘      │
└─────────────────────────────────────────────────────────────────┘
```

The Rust binary becomes an **HTTP service** instead of a TUI app:
- `POST /v1/chat/completions` — OpenAI-compatible, backed by rkllm-rs
  on the NPU. This is what `pi` already speaks.
- `POST /v1/audio/transcriptions` — OpenAI-shape STT endpoint. Pi calls
  it as a tool; server records from `plughw:1,0` S32_LE 48 kHz,
  resamples to 16 kHz S16 for SenseVoice, returns the transcript.
- `POST /v1/audio/speech` — OpenAI-shape TTS endpoint. Body is text;
  server synthesises with espeak-ng (baseline) → piper-rs (next) →
  piper-rknn-rs (NPU); plays through `plughw:1,0`.
- Custom skill endpoints / templates served from a directory the
  agent can read via tool calls (e.g. `GET /v1/templates/{type}`).

Pi's frontend uses `pi-tui` to render the meditation session on tty1,
swap to a different chapter via slash commands, etc. The agent's tool
catalog includes `transcribe`, `speak`, `list_templates`, `select
template`, `run_meditation_segment`, `wait(seconds)`.

#### Concrete next steps

1. **Replace Ratatui main loop with an HTTP server** in jhana-rs.
   Reuse `src/llm.rs` (rkllm-rs), `src/stt.rs`, `src/tts.rs` behind
   axum / hyper routes. Keep the binary single-file.
2. **Install Node 20 on the Rock** (~50 MB), clone pi-mono, build the
   coding-agent — or write a small jhana-specific harness on top of
   `pi-agent-core` + `pi-tui` that knows about meditation templates.
3. **Pick a smaller LLM** in `.rkllm` format that fits 8 GB RAM with
   headroom for the Node process: Llama-3.2-3B (already on device,
   4.35 GB) or Qwen3-1.7B (~1.1 GB) for faster decode. The
   M1-Max-tested Qwen3-Coder 30B will not fit.
4. **Author 3–5 jhana templates** as on-device markdown/YAML the
   agent retrieves through a tool call. Mirror the existing
   `src/prompts/*.md` corpus.
5. Move `src/main.rs` from a Ratatui TUI to an HTTP entry point;
   keep the old TUI binary available as `bin/jhana-rs-tui` for
   offline/diagnostic use and to test the Rust pipeline in isolation
   without Node.

#### Trade-offs

| Question                                  | Answer                                                                                                     |
|-------------------------------------------|------------------------------------------------------------------------------------------------------------|
| Does this match "Rust on the Rock"?       | Inference + audio stay Rust. The agent/UI layer becomes Node — a deliberate exception for the harness only.|
| Cyberbox / single-binary aesthetic?       | Compromised — Node adds ~50 MB and a process. The Rust HTTP service is still single-binary; pi is the second process. |
| Latency cost vs current direct-link Rust? | One extra HTTP hop per tool call (~ms locally). Negligible vs LLM tok/s.                                   |
| What if pi has unfixable bugs?            | We keep the `bin/jhana-rs-tui` Ratatui binary as the always-working fallback.                              |

This is a meaningful detour. Worth doing once the Rust pipeline
(STT + LLM + TTS + display + hardware buttons) is fully working and
stable end-to-end so the pi adaptation is a layer, not a rewrite.

### Interaction design: Moore's Natural Conversation Framework

The conversational UX for jhana-rs is designed against **Robert J.
Moore's Natural Conversation Framework (NCF)** from IBM Research,
which applies findings from **Conversation Analysis (CA)** — the
sociolinguistic study of how humans actually take turns, repair
misunderstandings, and structure dialogue — to the design of
conversational AI systems.

Whereas typical chatbots flatten dialogue into a request → response
loop, NCF treats the unit of interaction as a **sequence** built from
CA primitives:

- **Adjacency pairs** — first-pair-parts (e.g. *question, request,
  invitation*) project a constrained set of second-pair-parts
  (*answer, grant/decline, accept/refuse*). The meditation guide
  should respect what kind of response its previous utterance has
  made expectable.
- **Preference organisation** — within a pair, certain second-pair-
  parts are preferred (an *acceptance* after an invitation) while
  others are dispreferred (a *refusal*); dispreferred turns are
  typically delayed, mitigated, and accounted for. Important for
  asking the user to commit to a longer meditation than they wanted.
- **Pre-sequences** — turns that test the ground before a main action
  ("are you ready?" before "close your eyes"). NCF makes these
  first-class so the system warms the user up rather than launching.
- **Repair sequences** — self- and other-initiated repair when the
  user mishears, the STT mis-transcribes, or the LLM produces an
  off-topic instruction. The agent must recognise repair-initiations
  ("what?", "sorry, again?") and re-do the last turn rather than
  charging on.
- **Closing sequences** — proper closings ("we'll stop here. how was
  that?") instead of abrupt stops, important for meditation where
  reorientation matters.

NCF then layers a **dialogue-act vocabulary** on top of these (e.g.
*request_open_session*, *acknowledge*, *offer_instruction*,
*confirm_understanding*, *initiate_closing*) and an authoring model
where designers script sequences rather than individual responses.
For jhana-rs the practical implication is that the LLM is not the
whole conversational logic: there is a sequence layer above it that
tracks the projected next-action set, gates the LLM's output, and
routes user input to either the meditation flow or to repair.

Primary references:

- Moore, An, Ahmed & Gala, **"NC-Bench: An LLM Benchmark for
  Evaluating Conversational Competence"** (arxiv 2601.06426, 2026) —
  uses the Natural Conversation Framework as the spine of a
  benchmark for LLM dialogue competence across three suites (basic
  interaction patterns, information retrieval scenarios, complex
  multi-turn exchanges). The paper reports that current models do
  well on straightforward turns but **struggle specifically with
  repair, closings, and complex requests** — the exact failure
  modes jhana-rs has to handle gracefully in a meditation context.
  <https://arxiv.org/abs/2601.06426>
- Moore & Arar, *Conversational UX Design: A Practitioner's Guide to
  the Natural Conversation Framework* (ACM Books, 2019) — the
  canonical book-length treatment of NCF.
- Moore, "A Natural Conversation Framework for Conversational AI
  Systems" and related IBM Research write-ups on
  <https://research.ibm.com> describing the framework's dialogue-act
  vocabulary and sequence-authoring model.
- Sacks, Schegloff & Jefferson, "A Simplest Systematics for the
  Organization of Turn-Taking for Conversation" (*Language*, 50:4,
  1974) — the founding CA paper, gives us the turn-taking model NCF
  inherits.
- Schegloff, *Sequence Organization in Interaction* (Cambridge,
  2007) — definitive treatment of sequences, adjacency pairs,
  preference, pre-sequences and repair.
- Liddicoat, *An Introduction to Conversation Analysis* (2nd ed.,
  2011) — readable CA textbook if Schegloff is too dense.

NC-Bench's failure-mode list (repair, closings, complex requests) is
a useful **internal eval target** for jhana-rs: we can sample
relevant test prompts from the public NC-Bench suite and run them
against whichever on-device model we settle on, to validate that
the meditation guide behaves correctly when the user interrupts,
asks for a repeat, or ends the session mid-sequence.



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

### Model selection for the Rock (2026-05-15, derived from pi_sandbox)

The pi_sandbox project tested coding-agent-grade models on a 64 GB M1
Max (`/mnt/data/projects/pi_sandbox/docs/02-models.md`). Key lessons
that transfer to the Rock:

1. **MoE with low active params wins on memory-bandwidth-bound
   hardware.** Apple Silicon and the RK3588 both have one shared memory
   bus; "active params" sets the decode-tok/s ceiling more than total
   params. The Rock's bandwidth ceiling is even tighter than the M1.
2. **Dense ≥ ~20 B is bandwidth-death.** On M1 Max, Devstral-Small-24B
   ran at 11 tok/s (vs Qwen-30B-A3B at 51). For the Rock the equivalent
   threshold is much lower — anything dense above ~4 B is going to
   crawl.
3. **Structured tool-call training is non-negotiable** for the agent
   harness path (Pi calls tools via `<tool_call>` envelopes; older
   models like DeepSeek-Coder-V2-Lite, CodeLlama, StarCoder-2 silently
   fail). Verify the model's chat-template emits `<tool_call>` blocks
   before adopting it. Anything trained pre-mid-2024 is suspect.
4. **Choose by active params, then by tool-call quality.** Quant
   choices are secondary; for the Rock, w8a8 (the RKLLM default) is
   the only practical format anyway.

#### Direct port of pi_sandbox's tier list to the Rock

The Rock's working set is ~6 GB after OS + audio pipeline + (eventual)
Node frontend. RKLLM only supports a small, hand-converted set of
models. The candidates that survive:

| Model                            | Active / Total | Quant     | On-device size | Status on Rock                                                                                              |
|----------------------------------|---------------:|-----------|---------------:|-------------------------------------------------------------------------------------------------------------|
| **Llama-3.2-3B-Instruct**        |     3 B (dense) | w8a8 g128 |      **4.35 GB** | Already on device at `~/models/Llama-3.2-3B-Instruct_w8a8_g128_rk3588.rkllm`. Tool-call trained.            |
| **Llama-3.2-1B-Instruct**        |     1 B (dense) | w8a8      |      ~1.0 GB    | Has RKLLM conversions on HF (jamescallander/c01zaut). Worth testing for decode-speed gains.                 |
| **Qwen2.5-3B-Instruct**          |     3 B (dense) | w8a8      |      ~3.5 GB    | Pre-converted by [jamescallander](https://huggingface.co/jamescallander) per `docs/14_TODO.md`. Tool-call OK.  |
| **Qwen3-1.7B**                   |     1.7 B (dense) | w8a8     |      ~1.7 GB    | Newest Qwen3 small model. RKLLM conversion availability TBD; if conversion exists this is the top pick.     |
| **Gemma-3-4B**                   |     4 B (dense) | w8a8      |      ~4.5 GB    | Pre-converted, mentioned in 14_TODO.md. Gemma 3 has tool-call training.                                        |
| **Phi-3-mini-3.8B**              |     3.8 B (dense) | w8a8     |      ~4 GB     | Mature, well-supported. RKLLM conversion availability TBD.                                                  |

Rejected for the Rock (following pi_sandbox's logic, downsized):

- **gpt-oss-20b** (11 GB MXFP4) — too big; would saturate RAM and the
  NPU doesn't speak MXFP4 anyway.
- **Qwen3-Coder-30B / 80B**, **GLM-4.5-Air** — 21+ GB, completely out.
- **DeepSeek-Coder-V2-Lite** — even if converted, fails the tool-call
  template check (see pi_sandbox doc).
- **Anything dense ≥ ~6 B** — Devstral-style bandwidth death on RK3588;
  decode would be unusable for spoken meditation pacing.

#### Why no MoE in the Rock list

MoE models pass through *all* expert weights via memory even though only
a small fraction is computed per token. On the M1 Max with 64 GB RAM
this is fine. On the Rock with 8 GB and the RKLLM runtime (which
currently lacks production MoE support), there is no MoE candidate
small enough to fit. The Rock list is therefore all dense, capped at
~4 B to stay within bandwidth budget.

#### Recommended trial order

1. **Baseline what we have:** keep Llama-3.2-3B as the production model
   for now. Generate one meditation, measure decode tok/s and time-to-
   first-sentence. Decide if we need to go smaller for latency or
   bigger for quality.
2. **Test Llama-3.2-1B** for decode-speed wins; quality may be too thin
   for natural meditation guidance but worth measuring.
3. **Run pi_sandbox's `tool-call-test` equivalent** against the chosen
   model — emit `<tool_call>` to a stub `wait`, `speak_segment`,
   `transition_phase` tool catalog. If the model can't speak tool
   calls cleanly, we won't get the agent harness to work; stay on
   hand-rolled state machine.
4. **Watch for Qwen3-1.7B / Phi-3-mini RKLLM conversions** —
   small + modern + tool-call-trained = our top of the upgrade
   curve. The pi_sandbox "tested and rejected" list is the model
   to consult before pulling in any new conversion.

#### Sources beyond pi_sandbox

- **[Rockchip RKLLM model zoo](https://huggingface.co/c01zaut)** —
  c01zaut's HF page is the de-facto repo for RK3588-quantised LLMs in
  `.rkllm` format. Sort by recent uploads to find current tool-call
  trained variants.
- **[jamescallander RK3588 collection](https://huggingface.co/collections/jamescallander/rk3588-rkllm-models)**
  — also maintains a Qwen2.5 + Llama 3.2 conversion set; mentioned
  in `docs/14_TODO.md`.
- **[Are We Learning Yet?](https://www.arewelearningyet.com/)** — Rust
  ML ecosystem tracker; useful when picking a Rust-side inference
  crate to pair with the model.

### Mistral / Ministral / Mixtral fit (2026-05-15)

The Mistral family is the user's preferred lineage; the question is
which member fits the Rock and speaks pi's tool-call envelope.

| Model                             | Params           | Format     | Tool calls?                              | Fits 8 GB Rock? | Inference path                              |
|-----------------------------------|------------------|------------|------------------------------------------|-----------------|---------------------------------------------|
| **Ministral-3B-Instruct**         |    3 B (dense)   | GGUF / .rkllm if converted | Yes — Ministral series is post-2024, tool-call trained | Yes — Q4_K_M ~2 GB on disk | **Best fit.** Already on device as `~/models/Ministral-3B-Instruct-Q4_K_M.gguf`. Currently runnable via mistral.rs (CPU). Watch HF for an `.rkllm` conversion to move to the NPU. |
| **Ministral-8B-Instruct**         |    8 B (dense)   | GGUF       | Yes                                      | Q4 ~5 GB — borderline | CPU only on Rock; dense 8 B will be bandwidth-bound. Skip unless we ever cross to a larger SoC. |
| **Mistral-7B-Instruct-v0.3**      |    7 B (dense)   | GGUF       | Yes (v0.3 has function-calling)          | Q4 ~4 GB — fits | CPU only; ~2 tok/s expected on Rock. Pre-Ministral, slightly older tuning. |
| **Mistral-7B-Instruct v0.1/v0.2** |    7 B (dense)   | GGUF       | **No** — predates structured tool calls   | Fits             | Rejected; same failure mode as DeepSeek-Coder-V2-Lite in pi_sandbox. |
| **Mixtral 8×7B**                  |   12.9 B active / 47 B total | GGUF | Yes                                | **No** — ~26 GB at Q4 | Out of range for an 8 GB Rock. |
| **Mixtral 8×22B**                 |   39 B active    | GGUF       | Yes                                      | **No** — ~80 GB at Q4 | Out of range. |
| **Mistral-Small / Mistral-Nemo-12B** | 12 B dense   | GGUF       | Yes                                      | **No** — Q4 ~7 GB + KV cache | Borderline; CPU only would be unusably slow. |

#### Inference engine choice for the Mistral family

Mistral models ship primarily as **GGUF** (llama.cpp ecosystem). There
is no native `.rkllm` conversion for Mistral/Ministral as of 2026-05
— RKLLM's converter targets a small allowlist (Llama, Qwen, Gemma,
Phi, RWKV, etc.). To use a Mistral model on the Rock the realistic
paths are:

1. **`mistralrs` (Rust)** — pure Rust LLM inference engine, supports
   GGUF, runs on CPU. The current jhana-rs `src/llm.rs` already used
   `mistral.rs` once (HTTP variant); we ripped that out for `rkllm-rs`
   NPU inference. Re-adding mistralrs as an alternative backend is
   the easiest Rust-native way to run Ministral-3B. Expect ~3–5 tok/s
   on RK3588 A76 cores for a 3 B Q4 model.
2. **`llama.cpp` via `llama-server`** — pi-sandbox's tested stack.
   Mature, OpenAI-API-compatible HTTP, supports all GGUF Mistral
   variants. Speed on RK3588: similar to mistralrs (~3–5 tok/s for
   3 B Q4). Bigger codebase, C++ build, but it's the most
   battle-tested. Useful if we move to the pi harness front-end
   anyway since pi already talks to llama-server.
3. **`mistral.rs` via HTTP** — Rust-native, OpenAI-compatible. Same
   bind point for pi as llama-server.
4. **Wait for RKLLM Mistral support** — Rockchip's converter would
   need to add the Mistral architecture. No firm timeline.

#### Recommendation

- **Short term:** keep Llama-3.2-3B on the NPU via rkllm-rs as the
  default — it's the fastest path on the Rock (NPU >> CPU for LLM
  decode). Tool-call quality is comparable to Ministral.
- **If/when we adopt the pi harness:** stand up `llama-server` on the
  Rock with **Ministral-3B-Instruct-Q4_K_M.gguf** (already on disk)
  as a second LLM backend. Pi can target either — `llama-server` for
  Ministral on CPU, or our Rust HTTP shim around rkllm-rs for the
  NPU Llama. Measure tok/s and tool-call cleanness; switch defaults
  per the result.
- **Long term:** push for an `.rkllm` conversion of Ministral-3B (or
  a successor) so we can have user-preferred lineage + NPU speed in
  one package.

#### Tool-call compatibility with pi (gating requirement)

Pi expects the standard OpenAI `<tool_call>` envelope in the model's
chat-template. The pi_sandbox doc's pre-flight checks
(`tool-call-test`) catch failures cheaply. Before adopting *any*
model on the Rock:

1. Boot `llama-server` (or our shim) with the candidate model.
2. Issue an OpenAI `tools=[...]` request that demands a tool call
   (e.g. "what time is it?" with a `get_time` tool).
3. Verify the response contains a structured `tool_calls` array,
   not freeform text that *describes* calling a tool.

This screening is non-negotiable per pi_sandbox's experience with
DeepSeek-Coder-V2-Lite, CodeLlama, and similar models that look
right on paper but emit text-only suggestions when asked to call a
tool.

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
    10_SPECS.md              this file
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
