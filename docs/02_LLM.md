# 02: LLM Integration

## Model Format: GGUF

All models must be in **GGUF** format. The original Orca Mini 3B on the
device (`orca-mini-3b.ggmlv3.q4_0.bin`) is in the deprecated **GGML v3**
format, which modern llama.cpp no longer supports. It must be replaced with
a GGUF version.

GGUF is the standard format for llama.cpp since August 2023. All target
models (Qwen3-4B, Qwen3.5-2B, SmolLM3-3B, Llama 3.2 3B) are available
in GGUF on HuggingFace.

---

## Inference Engine Benchmark

Systematic comparison of Rust LLM inference engines on Rock 5A (RK3588S,
Cortex-A76/A55, 8 GB RAM, aarch64). Minimum viable: >2.5 tok/s (faster
than spoken speech for TTS pipeline).

### Test protocol

All tests use:
- **Model**: Orca Mini 3B Q4_0 GGUF (`q4_0-orca-mini-3b.gguf`, 1.9 GB)
- **Prompt**: Meditation guide prompt (37 tokens)
- **Max tokens**: 100
- **Hardware**: Rock 5A (RK3588S), debug build
- **Sampling**: temp=0.25, top_k=40, top_p=0.95

### Candidates

#### 1. llama-cpp-2 (C++ wrapper) — BASELINE

- Crate: [llama-cpp-2](https://crates.io/crates/llama-cpp-2) v0.1.146
- Approach: Rust FFI bindings to llama.cpp C/C++
- ARM NEON: Yes (hand-tuned assembly in llama.cpp)
- GGUF: Yes (all formats)
- Model support: All llama.cpp architectures
- Build deps: cmake, build-essential, libclang-dev

#### 2. llama-gguf (pure Rust)

- Crate: [llama-gguf](https://crates.io/crates/llama-gguf) v0.14.0
- Approach: Pure Rust reimplementation of llama.cpp
- ARM NEON: Claims support but not effective in benchmark
- GGUF: Yes (all formats)
- Build deps: protobuf-compiler

#### 3. OxiLLaMa (pure Rust, COOLJAPAN ecosystem)

- Source: [github.com/cool-japan/oxillama](https://github.com/cool-japan/oxillama)
- Approach: Pure Rust reimpl built on SciRS2/OxiBLAS/OxiFFT
- ARM NEON: Via OxiBLAS (Rust BLAS implementation)
- GGUF: Yes — all formats including Q4_0, K-quants, Q1_0_G128
- Model support: Llama, Qwen3, Mistral, Gemma, Phi, Command-R
- Build deps: None (pure Rust). Not on crates.io — git dependency.

#### 4. OxiBonsai (pure Rust, 1-bit specialist)

- Source: [OxiBonsai](https://kitasanio.medium.com/oxibonsai-the-worlds-first-pure-rust-1-bit-llm-inference-engine-4c15abf53fce) (Apr 2026)
- Approach: Pure Rust, zero-FFI, targets 80-85% of llama.cpp throughput
- ARM NEON: Yes (core::arch intrinsics, runtime CPU detection)
- GGUF: Q1_0_G128 focus (1-bit quantization)
- Notes: 1-bit only — may not support Q4_0. Skip if Q4_0 unsupported.

#### 5. Candle (HuggingFace, pure Rust ML framework)

- Crate: [candle-core](https://crates.io/crates/candle-core)
- Source: [github.com/huggingface/candle](https://github.com/huggingface/candle)
- Approach: General ML framework, not llama.cpp specific
- ARM NEON: Via candle's SIMD backend
- GGUF: Via candle-transformers quantized model support
- Notes: Requires model-specific Rust code. More complex integration
  but very active (HuggingFace maintained).

### Results

| Engine | tok/s | First token | Model load | Quality | Build deps | Status |
|--------|-------|-------------|------------|---------|------------|--------|
| **llama-cpp-2** | **5.8** | 1ms | 23.85s | Good | cmake, libclang-dev | **Tested** |
| llama-gguf v0.14 | ~0.25 | — | 1.46s | Poor | protobuf-compiler | Tested |
| OxiLLaMa v0.1.3 | **CRASH** | — | 0.01s | — | None | OOM: 13.3 GB alloc on 8 GB |
| OxiBonsai | **SKIP** | — | — | — | None | Q1_0 only — no Q4_0 support |
| Candle v0.10.2 | **3.03** | — | 0.09s | Gibberish | libssl-dev | NEON works, model compat issue |

### Analysis so far

llama-cpp-2 is 23x faster than llama-gguf on this hardware due to
hand-tuned ARM NEON SIMD in the C++ llama.cpp library. The llama-gguf
output quality difference (academic vs meditation text) is likely due
to sampler implementation, not the model.

Key question for remaining candidates: do any pure Rust engines match
llama.cpp's ARM NEON performance? OxiLLaMa and OxiBonsai claim NEON
support via Rust intrinsics — need to verify on this hardware.

**OxiLLaMa (v0.1.3) crashed immediately** — tried to allocate 13.3 GB for
a 1.9 GB model on 8 GB hardware. Does not use mmap like llama.cpp. This
is a disqualifying issue for memory-constrained devices. Installed as
release build via `cargo install oxillama-cli`.

---

## Test Plan

### Step 1: Verify build on Rock (DONE)

```bash
# Add llama-cpp-2 to Cargo.toml, sync, build
scripts/rock-sync.sh
scripts/rock-build.sh
```

Build both llama-cpp-2 and llama-gguf. Benchmark tokens/sec to decide.

### Step 2: Download a tiny GGUF test model

The Orca Mini 3B in GGUF is ~2 GB. For a quick build/load test, use a
smaller model first:

```bash
# On Rock (with internet via NAT forwarding)
curl -L -o /home/ubuntu/test-model.gguf \
  "https://huggingface.co/Aryanne/Orca-Mini-3B-gguf/resolve/main/q4_0-orca-mini-3b.gguf"
```

### Step 3: Load model and verify inference

- Load GGUF model via the chosen crate
- Run a simple prompt, verify tokens stream
- Measure tokens/sec on Rock (target: >2.5 tok/s for Orca 3B)

### Step 4: Wire to TUI

- Streaming tokens -> ChunkParser -> LlmOutput channel -> TUI display
- Sentence-by-sentence reveal with live token count

---

## Pause Marker Parsing

The `ChunkParser` in `src/llm.rs` implements the bracket state machine
from the Python `jhana-dev/src/meditation_guide.py`:

```
Text: "Close your eyes. [5] Breathe out. [3]"
       ^^^^^^^^^^^^^^^^  ^  ^^^^^^^^^^^^  ^
       Sentence("...")   |  Sentence(...)  |
                     Pause(5.0)        Pause(3.0)
```

Rules:
- `[` enters pause mode, `]` exits it
- Content between brackets parsed as float seconds
- Leading pauses (before first sentence) are skipped
- Sentences split on `.` boundary

---

## Model Inventory

### On device (Rock 5A)

| Model | Path | Size | Format | Status |
|-------|------|------|--------|--------|
| Orca Mini 3B Q4_0 | `/home/ubuntu/ai_in_a_box/downloaded/orca-mini-3b.ggmlv3.q4_0.bin` | 1.8 GB | GGML v3 | **Deprecated — needs GGUF replacement** |

### Target models (GGUF, to download)

| Model | HuggingFace | Size | Notes |
|-------|-------------|------|-------|
| Orca Mini 3B Q4_0 | [Aryanne/Orca-Mini-3B-gguf](https://huggingface.co/Aryanne/Orca-Mini-3B-gguf) | ~2.0 GB | Direct GGUF replacement for POC |
| Qwen3-4B Q4_K_M | TBD | ~2.5 GB | Upgrade: best quality small model |
| Qwen3.5-2B Q4_K_M | TBD | ~1.3 GB | Fallback if Qwen3-4B too slow |
| Qwen3-1.7B Q4_K_M | TBD | ~1.1 GB | Lightest viable option |

---

## Original Python Implementation

The jhana-dev Python app used:
- **Model**: [carecodeconnect/jhana-mistral-GGUF](https://huggingface.co/carecodeconnect/jhana-mistral-GGUF) (fine-tuned Mistral 7B)
- **Inference**: LangChain + Ollama (local HTTP streaming)
- **Pause markers**: Baked into fine-tuned model output as `[N]` format
- **Parsing**: Bracket state machine in `meditation_guide.py`

For the Rust POC, we use prompt engineering (system prompt + few-shot
examples) to get pause markers from a generic model. Fine-tuning a small
model is the upgrade path.
