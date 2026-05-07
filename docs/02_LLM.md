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

#### Orca Mini 3B Q4_0 (1.9 GB, old model)

| Engine | tok/s | First token | Model load | Quality | Status |
|--------|-------|-------------|------------|---------|--------|
| **llama-cpp-2** | **5.8** | 1ms | 23.85s | Good | Tested |
| llama-gguf v0.14 | ~0.25 | — | 1.46s | Poor | 23x slower |
| OxiLLaMa v0.1.3 | CRASH | — | — | — | OOM 13.3 GB |
| OxiBonsai | SKIP | — | — | — | Q1_0 only |
| Candle v0.10.2 | 3.03 | — | 0.09s | Gibberish | Model compat |

#### Qwen3-1.7B Q4_K_M (1.1 GB, modern model)

| Engine | tok/s | First token | Model load | Quality | Status |
|--------|-------|-------------|------------|---------|--------|
| **llama-cpp-2** | **10.8** | 2ms | 0.84s | **Excellent** | Tested |
| Candle v0.10.2 | — | — | 0.43s | — | Unsupported arch |

#### SmolLM2-1.7B-Instruct Q4_K_M (1.0 GB, Candle-compatible)

| Engine | tok/s | First token | Model load | Quality | Status |
|--------|-------|-------------|------------|---------|--------|
| **llama-cpp-2** | **11.8** | 1ms | 1.61s | Good | Tested |
| **Candle v0.10.2** | **8.72** | — | 0.14s | Good | Tested, NEON works |

#### Ministral 3B Instruct Q4_K_M (2.0 GB, Mistral family)

| Engine | tok/s | First token | Model load | Quality | Status |
|--------|-------|-------------|------------|---------|--------|
| llama-cpp-2 | 1.0 | 5ms | 5.01s | **Excellent** | Tested |
| **mistral.rs** | **3.89** | — | ~40s | **Excellent** | **Best candidate** |

Ministral 3B produced **[N] pause markers natively** without any fine-tuning
or few-shot examples — the only model to do so. The tone and style of the
Mistral family is an excellent fit for a meditation guide.

**mistral.rs benchmark details (2026-05-07):**
- Completion: **3.89 tok/s** (prompt processing: 5.62 tok/s)
- 200 tokens generated in 63.3s (51.5s completion + 11.7s prompt)
- Model load: ~40s (all 26 layers on CPU, F16 dtype)
- NEON detected and active, PagedAttention disabled (CPU-only)
- Chat template and tokenizer extracted from GGUF (no HF token needed)
- Server: OpenAI-compatible API on port 8321

**mistral.rs is 3.9x faster than llama-cpp-2** on the same Ministral 3B
model. This is surprising — mistral.rs uses Candle (pure Rust) internally
while llama-cpp-2 wraps llama.cpp (C++). The difference may be due to
mistral.rs using F16 compute vs llama-cpp-2's dequantization path, or
better attention implementation for the Mistral3 architecture.

At 3.89 tok/s, Ministral 3B now **exceeds the 2.5 tok/s target** and is
viable as the primary model.

Additional consideration: Mistral/Ministral models are developed by
Mistral AI (Paris, France) — an EU-based company. For sovereign,
locally-hosted AI in the EU, this is a strong choice. The model runs
entirely on-device with no internet connection required.

### Analysis

llama-cpp-2 is 23x faster than llama-gguf on this hardware due to
hand-tuned ARM NEON SIMD in the C++ llama.cpp library. The llama-gguf
output quality difference (academic vs meditation text) is likely due
to sampler implementation, not the model.

mistral.rs (which uses Candle internally) achieves 3.89 tok/s on
Ministral 3B vs llama-cpp-2's 1.0 tok/s on the same model — a 3.9x
improvement. This makes the Mistral model family viable where it
previously seemed too slow.

### Decision: mistral.rs + Ministral 3B (2026-05-07)

**Engine: [mistral.rs](https://github.com/EricLBuehler/mistral.rs)** (pure Rust, Candle-based)
**Model: Ministral-3-3B-Instruct-2512 Q4_K_M** (2.0 GB, Mistral AI)

Selected for Phase 1 implementation because:
- 3.89 tok/s (exceeds 2.5 target)
- Native `[N]` pause markers without fine-tuning or few-shot
- Excellent meditation tone and style (best of all models tested)
- Pure Rust engine (no C++ build deps)
- OpenAI-compatible API (easy to integrate via HTTP or as library)
- EU sovereign model (Mistral AI, Paris) — strong provider, well-funded
- HuggingFace ecosystem (model hosting, tokenizers, community)
- 2.0 GB model size, fits comfortably in 8 GB RAM
- Installed on Rock as `mistralrs-server` (port 8321)

**Trade-offs accepted vs Qwen3-1.7B + llama-cpp-2:**
- Slower (3.89 vs 10.8 tok/s) but fast enough for meditation pacing
- Larger model (2.0 vs 1.1 GB) but still within budget
- Slower model load (~40s vs 0.84s) — acceptable for always-on device
- Better quality output and native pause marker support compensate

### Summary: all viable options

| Option | Engine | Model | tok/s | Pros | Cons |
|--------|--------|-------|-------|------|------|
| **A** | **mistral.rs** | **Ministral 3B** | **3.89** | **Best quality, native [N], pure Rust, EU** | **Slower, 40s load** |
| B | llama-cpp-2 | Qwen3-1.7B | 10.8 | Fastest quality model, small | C++ deps, no native [N] |
| C | llama-cpp-2 | SmolLM2-1.7B | 11.8 | Fastest raw speed | C++ deps, less capable |
| D | Candle | SmolLM2-1.7B | 8.72 | Pure Rust, 0.14s load | Limited arch support |

All four exceed the 2.5 tok/s target.

### Alternative candidates (untested, for future reference)

- **[oxide-rs](https://github.com/theawakener0/oxide-rs)**: Pure Rust,
  CPU-focused, GGUF. Direct llama.cpp competitor without C++ deps.
- **[Shimmy](https://github.com/Michael-A-Kuykendall/shimmy)**: Pure Rust,
  OpenAI-compatible API, GGUF+SafeTensors. Native aarch64 binaries,
  single-binary deployment, ~100ms startup. Actively maintained (v1.7.0).
- **[small-infer](https://github.com/karthikworks/small-infer)**: ~4,500
  lines pure Rust, ARM NEON kernels, memory-mapped GGUF for zero load
  time. Designed for mobile/embedded. Currently Gemma3 arch only.

### Ruled out

- **llama-gguf**: 23x slower than llama-cpp-2 (no effective ARM NEON)
- **OxiLLaMa**: OOM crash (no mmap, tried 13.3 GB on 8 GB device)
- **OxiBonsai**: Q1_0 only, doesn't support Q4_0/Q4_K_M
- **Crane**: No clear GGUF Q4 quantization support, requires full model
  weights. Build from source only (no cargo install).
- **[crabml](https://github.com/crabml/crabml)**: Pure Rust, explicit
  ARM NEON via RUSTFLAGS, GGUF support. **Failed to compile on aarch64.**

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
