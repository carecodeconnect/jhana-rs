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

## Rust Binding Strategy

### Approach 1: llama-cpp-2 (C++ wrapper)

Wraps the battle-tested llama.cpp C/C++ library via FFI. Proven ARM NEON
SIMD optimizations. Broad model architecture support.

- Crate: [llama-cpp-2](https://crates.io/crates/llama-cpp-2)
- Pros: Proven on aarch64, NEON optimized, supports all GGUF models
- Cons: C++ build dependency (cmake, build-essential), unsafe FFI,
  large compile time on Rock (~10-20 min first build)
- Risk: C++ compilation on aarch64 may fail (mitigation: see Approach 2)

### Approach 2: llama-gguf (pure Rust)

Pure Rust implementation of llama.cpp with full GGUF support. No C++
dependency. Must be tested head-to-head with llama-cpp-2 on the Rock.

- Crate: [llama-gguf](https://crates.io/crates/llama-gguf)
- Source: [github.com/Lexmata/llama-gguf](https://github.com/Lexmata/llama-gguf)
- Pros: No C++ deps, simpler build, pure Rust, cleaner dependency chain
- Cons: Newer/less proven, SIMD support may be partial on ARM,
  model architecture coverage may lag behind llama.cpp
- Plan: Benchmark tokens/sec against llama-cpp-2 on same GGUF model

---

## Build Results (2026-05-07)

Both crates build successfully on Rock 5A (aarch64):

| Crate | Build result | Extra deps needed |
|-------|-------------|-------------------|
| llama-cpp-2 v0.1.146 | OK | `libclang-dev` (bindgen) |
| llama-gguf v0.14.0 | OK | `protobuf-compiler` (ONNX proto) |

llama-cpp-2 compiled the full llama.cpp C++ library from source on the
Cortex-A76 cores. First build ~5 min, subsequent builds cached.

### Inference benchmark (2026-05-07)

Model: Orca Mini 3B Q4_0 GGUF (1.9 GB) via llama-cpp-2.

| Metric | Value |
|--------|-------|
| Tokens/sec | **5.8** |
| Time to first token | 1ms |
| Model load time | 23.85s |
| Tokens generated | 100 |
| Generation time | 17.11s |
| Model RAM (mmap) | ~1.9 GB |
| KV cache | 650 MB |

5.8 tok/s exceeds the 2.5 tok/s target. Faster than spoken speech, so
TTS pipeline will keep up. Flash Attention auto-enabled.

Note: Orca Mini 3B did not produce `[pause]` markers with the basic
prompt — it's not fine-tuned for that. Options:
1. More aggressive few-shot prompting
2. Use a better instruction-following model (Qwen3-4B, Llama 3.2 3B)
3. Fine-tune a small model with pause markers (like jhana-mistral)

### Head-to-head: llama-cpp-2 vs llama-gguf (2026-05-07)

Same model (Orca Mini 3B Q4_0 GGUF), same prompt, 100 token limit.

| Metric | llama-cpp-2 (C++) | llama-gguf (pure Rust) |
|--------|-------------------|------------------------|
| **Tokens/sec** | **5.8** | **~0.25** |
| Model load time | 23.85s | 1.46s |
| Output quality | Meditation text (on-topic) | Academic text (off-topic) |
| ARM NEON SIMD | Yes (hand-tuned asm) | No |

**Decision: use llama-cpp-2.** The 23x inference speed advantage is
decisive. llama-gguf's faster model load (16x) doesn't compensate —
generation speed is what matters for real-time streaming to TTS.

The llama-gguf output quality difference is likely due to sampler
implementation differences, not the model itself. But inference speed
alone disqualifies it on this hardware.

llama-gguf may become viable if/when it adds ARM NEON SIMD support.

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
