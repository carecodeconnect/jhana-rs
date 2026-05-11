# 05: RK3588 NPU Acceleration

## Hardware source of truth

| Component | Spec | Source |
|-----------|------|--------|
| Board | Rock 5A | [radxa.com/products/rock5/5a](https://radxa.com/products/rock5/5a/) |
| SoC | RK3588S | [Rockchip RK3588S datasheet](https://www.rock-chips.com/uploads/pdf/2022.8.26/192/RK3588S%20Brief%20Datasheet.pdf) |
| NPU | 6 TOPS (INT8), 3 cores | Device tree: `rockchip,rk3588-rknpu` at `fdab0000.npu` |
| CPU | 4x Cortex-A76 + 4x Cortex-A55 | |
| RAM | 8 GB | |
| Kernel | 5.10.110-102-rockchip | Radxa vendor kernel (Rockchip BSP) |
| NPU kernel driver | RKNPU v0.8.2 (builtin, **outdated** — latest is v0.9.8) | Built into vendor kernel, cannot update without kernel rebuild |
| NPU userspace runtime | librknnrt.so v2.2.0 (**installed** 2026-05-07) | [airockchip/rknn-toolkit2](https://github.com/airockchip/rknn-toolkit2) |

### OS image

The Rock runs Radxa's official Ubuntu 22.04 Jammy CLI image, built
2023-08-22 via `rbuild`. Image fingerprint on device:
`/etc/radxa_image_fingerprint`.

| Field | Value |
|-------|-------|
| Build date | 2023-08-22 |
| Build command | `./rbuild --native-build --shrink rock-5a jammy cli` |
| Original kernel | `linux-image-5.10.110-12-rockchip` |
| Current kernel | `5.10.110-102-rockchip` (updated via apt) |
| U-Boot | 2017.09-1-77a5f37 |
| APT repo | `https://radxa-repo.github.io/jammy` |

Source: [Radxa Rock 5A downloads](https://wiki.radxa.com/Rock5/downloads)

### RKNPU driver is outdated

The kernel's built-in RKNPU driver is v0.8.2 (from 2022-08-29). The
latest upstream is **v0.9.8** (from [airockchip/rknn-toolkit2](https://github.com/airockchip/rknn-toolkit2)).
The driver is compiled into the kernel (`modinfo rknpu` shows
`filename: (builtin)`), so it cannot be updated as a loadable module —
it requires either:

1. A newer Radxa kernel package via `apt upgrade` (if Radxa ships one)
2. Rebuilding the kernel from [rockchip-linux/kernel](https://github.com/rockchip-linux/kernel)
   with the updated RKNPU driver source from [airockchip/rknn-toolkit2 rknpu2/](https://github.com/airockchip/rknn-toolkit2/tree/master/rknpu2)

For now, v0.8.2 is functional with librknnrt v2.2.0+ for small models
(STT, VAD) — the userspace runtime is forwards-compatible for RKNN.

**However, RKLLM (LLM on NPU) requires driver v0.9.7+.** Tested
2026-05-08: `librkllmrt.so` v1.2.3 fails with `failed to malloc npu
memory` and `failed to get pages: -12` (ENOMEM) on driver v0.8.2. The
old driver cannot allocate the ~3.2 GB contiguous NPU memory buffer that
a 3B LLM model requires. Driver upgrade is **blocking for LLM on NPU**.

Tested 2026-05-08:
- **3B model** (Llama-3.2-3B, 4.35 GB): fails to allocate 3.2 GB NPU memory
- **270M model** (Gemma-3-270M, 629 MB): loads in 1.24s, runs at 66 tok/s,
  but `matmul(w8a8) run failed` errors produce garbage output — the old
  driver's matmul implementation is incompatible with w8a8 quantization.
  NPU hardware is functional (66 tok/s proves cores are computing), but
  results are wrong.

**Conclusion:** Driver upgrade to v0.9.7+ is required for correct RKLLM
inference. See `docs/06_KERNEL.md` for upgrade options.

Options:
1. **Flash newer Radxa image** — check for Ubuntu 24.04 or newer Debian
   images with updated kernel and RKNPU driver
2. Rebuild the kernel from [rockchip-linux/kernel](https://github.com/rockchip-linux/kernel) with updated RKNPU driver
3. Fall back to CPU LLM (mistral.rs, current approach, 3.89 tok/s)

### NPU status on boot

The NPU kernel driver loads successfully:
```
RKNPU fdab0000.npu: RKNPU: rknpu iommu is enabled, using iommu mode
[drm] Initialized rknpu 0.8.2 20220829 for fdab0000.npu on minor 1
```

The `can't request region for resource` warnings for the three NPU core
memory regions (`fdab0000`, `fdac0000`, `fdad0000`) are normal on the
RK3588S BSP kernel — the RKNPU driver still initializes and reports all
three cores:
```
NPU load:  Core0:  0%, Core1:  0%, Core2:  0%
```

These warnings do **not** prevent NPU usage. They are a device tree /
resource reservation cosmetic issue in the vendor kernel, not a hardware
fault.

### fill_queue:2263 errors (NOT the NPU)

The `fill_queue:2263 Bad Desc` messages in the boot log are from the
**PL330 DMA controller** at `fea30000.dma-controller`, not the NPU at
`fdab0000.npu`. The preceding line is always:
```
dma-pl330 fea30000.dma-controller: pl330_submit_req:1737 Try increasing mcbufsz (353/256)
dma-pl330 fea30000.dma-controller: fill_queue:2263 Bad Desc(N)
```

This is a known RK3588 BSP kernel issue: the DMA microcode buffer
(`mcbufsz`) is too small (256 bytes) for the descriptor being submitted
(353 bytes). It affects audio DMA (I2S/PCM) transfers. The errors are
harmless — the DMA controller retries — but they spam the console.

Suppressed in the TUI session via `sudo dmesg -n 1` (see
`docs/01_DEV_SETUP.md`, "Suppress kernel console messages").

---

## What can use the NPU?

### Per-module NPU status

| Module | Current impl | NPU status | NPU benefit |
|--------|-------------|-----------|-------------|
| **LLM** | mistral.rs (CPU, GGUF) | **Cannot use NPU** — see below | N/A |
| **STT** | Not yet implemented | **Supported** — needs `.rknn` models (SenseVoice, Paraformer) | Big win |
| **VAD** | Not yet implemented | **Supported** — Silero VAD has RKNN code path | Marginal |
| **TTS** | sherpa-onnx Piper VITS | **Not supported in sherpa-onnx** — no RKNN TTS code path | Use Paroli instead |

### How sherpa-onnx RKNN actually works

The `provider: Some("rknpu".into())` field in the config **does NOT
enable NPU**. The `provider` string is for ONNX Runtime execution
providers (cpu, cuda, coreml, etc.) — `rknpu` is not in the list and
silently falls back to CPU.

sherpa-onnx RKNN support is a **separate code path** that loads `.rknn`
format models directly via `librknnrt.so` (not via ONNX Runtime). The
code auto-detects the model format. To use NPU:

1. Convert ONNX models to `.rknn` format using `rknn-toolkit2`
2. Pass the `.rknn` model path instead of the `.onnx` path
3. The C++ code dispatches to the `rknn/` implementation automatically

**Supported RKNN models in sherpa-onnx v1.13.0:**
- `offline-paraformer-model-rknn` (STT, Chinese)
- `offline-sense-voice-model-rknn` (STT, multilingual)
- `online-zipformer-*-model-rknn` (streaming STT)
- `silero-vad-model-rknn` (VAD)
- No TTS RKNN code path exists

### TTS benchmark: CPU vs NPU (2026-05-07)

Tested on Rock 5A with Piper en_US-lessac-medium, sentence: "Close your
eyes and take a deep breath in." (length_scale=1.3)

| Provider | Load | Run 1 | Run 2 | Run 3 | RTF |
|----------|------|-------|-------|-------|-----|
| `cpu` | 1.89s | 0.87s | 0.75s | 0.78s | ~0.31 |
| `rknpu` (fallback to cpu) | 1.79s | 0.78s | 0.75s | 0.71s | ~0.30 |

Both are identical — `rknpu` provider is not recognized and falls back to
CPU. RTF ~0.31 means synthesis is 3x faster than real-time on CPU, which
is adequate for meditation TTS. NPU would be useful for faster first-
sentence latency but is not blocking.

### LLM: cannot use RK3588 NPU

mistral.rs runs LLM inference via llama.cpp (GGUF format) on CPU. The
RK3588 NPU only supports RKNN-format models — there is no path to run
GGUF/transformer LLMs on the RKNPU.

Rockchip has a separate **RKLLM** toolkit
([airockchip/rknn-llm](https://github.com/airockchip/rknn-llm)) for
running small LLMs on the NPU (TinyLlama 1.1B ~10-15 tok/s, Qwen2 0.5B,
DeepSeek-R1-Distill-Qwen-1.5B). However:
- RKLLM provides C/C++ and Python APIs only — no Rust bindings
- Maximum supported model size is ~4B parameters (INT4)
- Ministral 3B would need conversion to RKLLM format (untested)
- [RKLLama](https://github.com/NotPunchnox/rkllama) (Go+Rust) wraps
  RKLLM as an Ollama-compatible server — potential future path

For now, LLM stays on CPU via mistral.rs. The NPU is best used for
STT/VAD/TTS where the models are small and inference-heavy.

### RKNN Rust ecosystem (surveyed 2026-05-07)

#### High-level inference crates

| Crate | Version | Author | Purpose | Relevant? |
|-------|---------|--------|---------|-----------|
| [`sensevoice-rs`](https://crates.io/crates/sensevoice-rs) | 0.1.7 | darkautism | SenseVoice STT + FSMN-VAD, Candle-based, native RKNN | **Yes — STT+VAD** |
| [`od_opencv`](https://crates.io/crates/od_opencv) | 0.8.2 | LdDl | YOLO object detection with `rknn-backend` feature | No — vision only |

`sensevoice-rs` is the only high-level Rust crate for audio/speech on
RKNN. Uses Candle (HuggingFace Rust ML framework) for CPU inference,
`rknn-rs` for NPU. Includes built-in FSMN-VAD. Heavy deps: candle-core,
candle-nn, hf-hub, sentencepiece, ndarray.

RKNN model: [`happyme531/SenseVoiceSmall-RKNN2`](https://huggingface.co/happyme531/SenseVoiceSmall-RKNN2)
(~973 MB total: encoder.rknn + tokenizer + VAD + embeddings).

#### Low-level RKNN bindings (three competing stacks)

| Crate | Version | Author | Notes |
|-------|---------|--------|-------|
| [`rknn-rs`](https://crates.io/crates/rknn-rs) | 0.2.4 | darkautism | Used by sensevoice-rs, wraps `rknn-sys-rs` |
| [`rknpu2`](https://crates.io/crates/rknpu2) | 0.0.9 | boundarybitlabs | Independent bindings, targets RKNN API v2.3.2, `rk35xx` feature |
| [`rknpu2-sys`](https://crates.io/crates/rknpu2-sys) | 0.0.9 | boundarybitlabs | Raw FFI bindings for above |
| [`rknpu-sys`](https://crates.io/crates/rknpu-sys) | 0.1.3 | ferrock (Codeberg) | Third set of FFI bindings, `custom-op` + `matmul` features |

All three wrap the same `librknnrt.so` C API but are independent projects.
`rknn-rs` is the most actively used (by sensevoice-rs). `rknpu2` has the
most features (dynamic loading, per-SoC targets). `rknpu-sys` adds custom
operator and matmul support.

#### Utilities

| Crate | Version | Purpose |
|-------|---------|---------|
| [`rktop`](https://crates.io/crates/rktop) | 0.1.5 | NPU load monitor (htop for NPU) |
| [`rockchip-pm`](https://crates.io/crates/rockchip-pm) | — | Power domain control |

### TTS on NPU: Paroli benchmark

[Paroli](https://github.com/marty1885/paroli) (C++) demonstrated Piper
VITS on the RK3588 NPU with a **real-time factor of ~0.15** (4.3x faster
than CPU). This requires converting the Piper decoder ONNX model to RKNN
format. sherpa-onnx's RKNN provider may achieve similar results if the
model is compatible.

---

## Protobuf clash: sherpa-onnx + sensevoice-rs (2026-05-07)

**sherpa-onnx cannot coexist with sensevoice-rs in the same binary.**

Both crates vendor their own copy of Google protobuf-lite via C++ -sys
crates:
- `sherpa-onnx-sys` → bundles `protobuf-lite` (via onnxruntime)
- `sentencepiece-sys` (dep of sensevoice-rs) → bundles `protobuf-lite`

This causes **duplicate symbol errors** at link time (`multiple definition
of google::protobuf::internal::WireFormatLite::*`).

### The `--allow-multiple-definition` trap

Using `RUSTFLAGS="-C link-args=-Wl,--allow-multiple-definition"` lets
the binary link, but the linker **arbitrarily picks one copy's symbols**.
At runtime, version-specific code paths diverge and protobuf throws
`libprotobuf FATAL` errors to the console. The TUI becomes unusable —
error messages flood the display and the speaker produces clicks/pops
from failed DMA audio transfers.

No upstream fix exists. Searched:
- [k2-fsa/sherpa-onnx#2444](https://github.com/k2-fsa/sherpa-onnx/issues/2444) — iOS protobuf compat, no resolution
- [onnx/onnx#1277](https://github.com/onnx/onnx/issues/1277) — same root cause in C++ static linking
- [google/sentencepiece#1129](https://github.com/google/sentencepiece/issues/1129) — link error with external protobuf
- sensevoice-rs — no protobuf conflict reports

### Resolution: drop sherpa-onnx

**Decision (2026-05-07):** Remove `sherpa-onnx` from Cargo.toml entirely.

| Module | Before (sherpa-onnx) | After |
|--------|---------------------|-------|
| **TTS** | sherpa-onnx Rust bindings (Piper VITS) | Piper CLI (`/usr/local/bin/piper`) via `std::process::Command` |
| **STT** | Not used for STT | sensevoice-rs (RKNN NPU) |
| **VAD** | sherpa-onnx Silero VAD (planned) | sensevoice-rs built-in FSMN-VAD |
| **LLM** | Not used for LLM | HTTP client to mistral.rs / rkllm-rs |

Piper CLI produces identical output to sherpa-onnx Piper VITS — same
model, same ONNX runtime, just invoked as a subprocess instead of FFI.
The TTS thread already used `aplay` for playback, so the only change is
synthesis: sherpa-onnx API → Piper CLI pipe.

### Alternatives considered and rejected

- **Patch one crate's build.rs** to use the other's protobuf — fragile,
  breaks on crate updates
- **Build sherpa-onnx as shared library (.so)** — avoids static symbol
  clash but adds deployment complexity
- **Strip protobuf from one .rlib with `ar d`** — hacky, unreliable
- **Run STT in a separate process** — overcomplicated for the benefit

---

## Why rebuild sherpa-onnx?

The `sherpa-onnx-sys` Rust crate downloads prebuilt static libraries from
GitHub releases (`sherpa-onnx-v1.13.0-linux-aarch64-static-lib.tar.bz2`).
These are **CPU-only** — they don't include the RKNN execution provider.

Setting `provider: Some("rknpu".into())` has no effect unless the
underlying C library was compiled with `-DSHERPA_ONNX_ENABLE_RKNN=ON`.

After rebuilding, we point Cargo at our custom libs via:
```bash
SHERPA_ONNX_LIB_DIR=/home/ubuntu/sherpa-onnx-rknn/build/install/lib cargo build --release
```

---

## Prerequisites

### librknnrt.so v2.2.0

The RKNN runtime library must be installed system-wide on the Rock.

**Source:** `airockchip/rknn-toolkit2` on GitHub — Rockchip's official
open-source repo for the RKNN SDK. `airockchip` is Rockchip's GitHub org
for AI/NPU tooling (separate from their main `rockchip-linux` org).

**Install on Rock (with internet access via router):**
```bash
# Download runtime library
wget -O /tmp/librknnrt.so \
  "https://github.com/airockchip/rknn-toolkit2/raw/v2.2.0/rknpu2/runtime/Linux/librknn_api/aarch64/librknnrt.so"
sudo cp /tmp/librknnrt.so /usr/lib/librknnrt.so
sudo ldconfig

# Download C headers (required for sherpa-onnx RKNN build)
wget -O /tmp/rknn_api.h \
  "https://github.com/airockchip/rknn-toolkit2/raw/v2.2.0/rknpu2/runtime/Linux/librknn_api/include/rknn_api.h"
wget -O /tmp/rknn_matmul_api.h \
  "https://github.com/airockchip/rknn-toolkit2/raw/v2.2.0/rknpu2/runtime/Linux/librknn_api/include/rknn_matmul_api.h"
wget -O /tmp/rknn_custom_op.h \
  "https://github.com/airockchip/rknn-toolkit2/raw/v2.2.0/rknpu2/runtime/Linux/librknn_api/include/rknn_custom_op.h"
sudo cp /tmp/rknn_api.h /tmp/rknn_matmul_api.h /tmp/rknn_custom_op.h /usr/include/

# Verify:
strings /usr/lib/librknnrt.so | grep "librknnrt version"
# Expected: librknnrt version: 2.2.0 (c195366594@2024-09-14T12:18:56)
```

Installed 2026-05-07. An older v1.5.2 exists in the `useful_transformers`
Python package but is too old for current RKNN SDK.

---

## Build steps

All builds happen **on the Rock** (aarch64). Script: `scripts/rock-build-sherpa-rknn.sh`

### Step 1: Install librknnrt.so v2.2.0 + headers

See Prerequisites above.

### Step 2: Clone sherpa-onnx

```bash
git clone https://github.com/k2-fsa/sherpa-onnx /home/ubuntu/sherpa-onnx-rknn
cd /home/ubuntu/sherpa-onnx-rknn
git checkout v1.13.0   # match Rust crate version
```

### Step 3: Build with RKNN support

```bash
mkdir -p build && cd build
cmake \
    -DSHERPA_ONNX_ENABLE_RKNN=ON \
    -DBUILD_SHARED_LIBS=OFF \
    -DCMAKE_BUILD_TYPE=Release \
    -DSHERPA_ONNX_ENABLE_TTS=ON \
    -DSHERPA_ONNX_ENABLE_BINARY=OFF \
    -DCMAKE_INSTALL_PREFIX=./install \
    ..
make -j4
make install
```

Note: cmake must find `rknn_api.h` in the system include path. If the
build fails with `fatal error: rknn_api.h: No such file or directory`,
install the headers from step 1.

### Step 4: Verify

```bash
ls install/lib/*.a
# Key libs: libsherpa-onnx-c-api.a, libsherpa-onnx-core.a, libonnxruntime.a
```

### Step 5: Rebuild jhana-rs

```bash
SHERPA_ONNX_LIB_DIR=/home/ubuntu/sherpa-onnx-rknn/build/install/lib \
    RUSTFLAGS="-C target-feature=+fp16" \
    cargo build --release
```

### FP16 build flag (required on RK3588)

The `gemm-f16` crate (dependency of Candle, used by sensevoice-rs) emits
aarch64 FP16 FMLA/FMUL/FADD instructions. The RK3588S Cortex-A76/A55
cores support FP16 (`fphp` + `asimdhp` in `/proc/cpuinfo`), but Rust's
default aarch64 target doesn't enable it. Without this flag, the build
fails with `error: instruction requires: fullfp16`.

**Always build with:**
```bash
RUSTFLAGS="-C target-feature=+fp16" cargo build
```

Or add to `~/.cargo/config.toml` on the Rock:
```toml
[target.aarch64-unknown-linux-gnu]
rustflags = ["-C", "target-feature=+fp16"]
```

### Step 6: Set provider in Rust code

```rust
// In OfflineTtsModelConfig (src/tts.rs):
provider: Some("rknpu".into()),

// Future: in VadModelConfig, OfflineRecognizerConfig, etc.
```

Done 2026-05-07 for TTS. STT/VAD provider will be set when those
modules are implemented.

---

## RKNN models available

sherpa-onnx RKNN support requires `.rknn` format models, not `.onnx`.
Pre-converted models are available from k2-fsa GitHub releases. Custom
models must be converted from ONNX to RKNN on an x86_64 machine using
`rknn-toolkit2` (cannot convert on the Rock itself).

### STT (pre-converted, ready to use)

| Model | Languages | Download |
|-------|-----------|----------|
| SenseVoice (recommended) | zh, en, ja, ko, yue | `sherpa-onnx-rk3588-20-seconds-sense-voice-zh-en-ja-ko-yue-2025-09-09` |
| Paraformer | zh | `sherpa-onnx-rk3588-15-seconds-paraformer-zh-2025-10-07` |
| Zipformer (streaming) | zh+en | `sherpa-onnx-rk3588-streaming-zipformer-small-bilingual-zh-en-2023-02-16` |

Download from: `https://github.com/k2-fsa/sherpa-onnx/releases/tag/asr-models`

SenseVoice is the best fit — multilingual (includes English), pre-
converted to RKNN, and tested on RK3588 by the sherpa-onnx team.

### VAD (has RKNN code path)

Silero VAD has a dedicated `silero-vad-model-rknn.cc` in sherpa-onnx.
May need ONNX->RKNN conversion, or may be bundled with the STT models.

### TTS (no sherpa-onnx RKNN path)

sherpa-onnx has **no RKNN code path for TTS**. Alternatives:

| Approach | Status |
|----------|--------|
| CPU (current) | Working, RTF ~0.31 (3x real-time) |
| [Paroli](https://github.com/marty1885/paroli) + RKNN decoder | 4.3x speedup, needs ONNX->RKNN conversion |
| [RKLLama](https://github.com/NotPunchnox/rkllama) Piper TTS endpoint | Runs encoder ONNX + decoder RKNN |
| [danielferr85/piper-checkpoints-rknn](https://huggingface.co/danielferr85/piper-checkpoints-rknn) | Pre-converted Piper RKNN decoders on HuggingFace |

### LLM (RKLLM)

[airockchip/rknn-llm](https://github.com/airockchip/rknn-llm) provides
the RKLLM C/C++ runtime for running LLMs on the NPU.

**Rust integration:** [rkllm-rs](https://crates.io/crates/rkllm-rs)
v0.1.14 — Rust FFI bindings for `librkllmrt.so`. Same author as
sensevoice-rs. Loads `.rkllm` models directly, callback-based streaming.
No HTTP server needed. Supports RKLLM runtime v1.1.4–v1.2.3.

#### librkllmrt.so installation (on Rock)

```bash
# Download v1.2.3 (matches rkllm-rs v0.1.14)
wget -O /tmp/librkllmrt.so \
  "https://raw.githubusercontent.com/airockchip/rknn-llm/release-v1.2.3/rkllm-runtime/Linux/librkllm_api/aarch64/librkllmrt.so"
sudo cp /tmp/librkllmrt.so /usr/lib/librkllmrt.so
sudo ldconfig
```

Installed 2026-05-08. File size: 7.5 MB.

#### Pre-converted .rkllm models for RK3588 (3B–4B, 8 GB RAM)

All models use w8a8 quantization (8-bit weights, 8-bit activations).
Models must fit in RAM alongside STT (~1 GB) and TTS (~0.5 GB).

**Recommended for jhana-rs (meditation text generation):**

| Model | Params | File size | RAM | Source | Notes |
|-------|--------|-----------|-----|--------|-------|
| **Llama-3.2-3B-Instruct** | 3B | 4.35 GB | ~4 GB | [jamescallander](https://huggingface.co/jamescallander/Llama-3.2-3B-Instruct_w8a8_g128_rk3588.rkllm) | **Primary choice.** General instruct, good creative prose, designed for edge deployment |
| **Gemma-3-4B-IT** | 4B | 5.62 GB | ~5 GB | [jamescallander](https://huggingface.co/jamescallander/gemma-3-4b-it_w8a8_g128_rk3588.rkllm) | Best language quality, tight RAM fit with STT+TTS. Most downloaded (564) |
| Qwen2.5-3B-Instruct | 3B | 3.83 GB | ~4 GB | [hw33](https://huggingface.co/hw33/Qwen2.5-3B-Instruct_W8A8_G256_RK3588.rkllm) | w8a8 g256 (larger group size). Less vetted, no model card |
| Qwen2.5-3B-Instruct | 3B | 3.76 GB | ~4 GB | [akmaldira](https://huggingface.co/akmaldira/Qwen2.5-3B-Instruct-RKLLM) | w8a8, low downloads (6) |

**Not suitable:**

| Model | Reason |
|-------|--------|
| MiniCPM3-4B | 8 GB RAM minimum — fills entire pool |
| Qwen2.5-Coder-3B | Coder model, not general text |
| medgemma-4b-it | Medical domain |
| Any 7B+ model | Exceeds 8 GB RAM budget |
| Mistral/Ministral | No .rkllm conversions exist (as of 2026-05-08) |

**Download Llama-3.2-3B-Instruct to Rock:**

```bash
cd /home/ubuntu/models
wget -O Llama-3.2-3B-Instruct_w8a8_g128_rk3588.rkllm \
  "https://huggingface.co/jamescallander/Llama-3.2-3B-Instruct_w8a8_g128_rk3588.rkllm/resolve/main/Llama-3.2-3B-Instruct_w8a8_g128_rk3588.rkllm"
```

**Model sources:**
- [jamescallander collection](https://huggingface.co/collections/jamescallander/rk3588-rkllm-models) — 25+ models, w8a8_g128, well-documented, RKLLM-Toolkit v1.2.1
- [Pelochus/ezrkllm-collection](https://huggingface.co/Pelochus/ezrkllm-collection) — older models with RAM estimates (wide ranges, less reliable)
- Custom conversion via [rkllm-toolkit](https://github.com/airockchip/rknn-llm) (x86_64 only)

**Mistral/Ministral note:** No .rkllm conversions of Mistral or Ministral
models exist on HuggingFace as of 2026-05-08. The only Mistral .rkllm is
a 24B model (too large for 8 GB). Ministral 3B could be converted via
rkllm-toolkit on x86_64 but no one has published one yet.

#### Convert Ministral 3B to .rkllm (on X61s)

Ministral 3B is the current CPU model (via mistral.rs, 3.89 tok/s). To
compare it on NPU, convert to .rkllm format using rkllm-toolkit on the
X61s (x86_64 only — cannot convert on the Rock).

```bash
# Install rkllm-toolkit (Python 3.8-3.12, x86_64)
pip install rkllm-toolkit

# Download Ministral-3B-Instruct from HuggingFace (safetensors)
huggingface-cli download mistralai/Ministral-3B-Instruct-2410

# Convert to .rkllm (w8a8 quantization for RK3588)
python3 -c "
from rkllm.api import RKLLM
llm = RKLLM()
llm.load_huggingface(model='mistralai/Ministral-3B-Instruct-2410')
llm.build(
    do_quantization=True,
    quantized_dtype='w8a8',
    target_platform='rk3588',
)
llm.export_rkllm('Ministral-3B-Instruct_w8a8_rk3588.rkllm')
"

# Copy to Rock
# Copy to Rock (IP and user from config.json)
scripts/rock-ssh.sh "true"  # verify connection
scp Ministral-3B-Instruct_w8a8_rk3588.rkllm \
  $(jq -r '.rock.user' config.json)@$(jq -r '.rock.ip' config.json):~/models/
```

Expected file size: ~3.5–4 GB. This would allow direct comparison of
meditation text quality: same model on CPU (mistral.rs GGUF) vs NPU
(rkllm-rs .rkllm).

### Model conversion (ONNX -> RKNN)

For models without pre-converted RKNN versions:

1. Requires x86_64 Linux machine with `rknn-toolkit2` Python package
2. Cannot be done on the Rock (aarch64) — toolkit is x86_64 only
3. Workflow: PyTorch -> ONNX -> RKNN (via `rknn.export()`)
4. See [rknn-toolkit2 docs](https://github.com/airockchip/rknn-toolkit2)

---

## Prior art: RKLLama

[RKLLama](https://github.com/NotPunchnox/rkllama) is the closest existing
project to jhana-rs — a full voice AI pipeline on the RK3588 NPU. Written
in Go + Rust, it wraps Rockchip's RKLLM runtime as an Ollama-compatible
HTTP server.

### Feature comparison

| Feature | jhana-rs | RKLLama |
|---------|----------|---------|
| **Language** | Rust | Go + Rust |
| **LLM** | mistral.rs (CPU, GGUF, 3B) | RKLLM (NPU, rkllm format, ≤4B) |
| **LLM speed** | ~3.89 tok/s (CPU, Ministral 3B) | ~10-17 tok/s (NPU, TinyLlama 1.1B) |
| **STT** | sensevoice-rs (Rust, RKNN) | omniASR-CTC or Whisper (RKNN) |
| **TTS** | sherpa-onnx Piper VITS (CPU) | Piper (ONNX encoder + RKNN decoder) |
| **TTS speed** | RTF ~0.31 (CPU) | RTF ~0.15 (NPU, 4.3x faster) |
| **VAD** | Silero VAD (via sensevoice-rs) | Built-in |
| **API** | Embedded TUI (ratatui), no server | HTTP server (Ollama-compatible) |
| **Use case** | Dedicated meditation device | General-purpose LLM server |
| **Display** | Rock 5A physical screen (DRM/KMS) | Headless server |
| **Hardware** | Rock 5A (RK3588S) | Any RK3588/RK3588S board |

### What jhana-rs does differently

- **Embedded single-purpose device** — not a server. TUI runs directly on
  the Rock's physical display with hardware buttons. No network required.
- **Pure Rust** — no Go, no Python. Entire pipeline is Rust (minus C/C++
  FFI for sherpa-onnx and sensevoice-rs deps).
- **Meditation-specific** — prompt engineering for guided meditations with
  `[N]` pause markers, sentence-by-sentence TTS.
- **Voice-synced TUI** — visual effects designed to sync with TTS playback
  (typewriter reveal, breathing animations during pauses).

### What jhana-rs could learn from RKLLama

- **TTS on NPU** — RKLLama runs the Piper VITS decoder on RKNN for 4.3x
  speedup. jhana-rs could adopt the same approach via Paroli or direct
  `rknpu2` crate integration.
- **LLM on NPU** — if meditation text quality is acceptable with a 1.1B
  model, RKLLama's Ollama-compatible API is a drop-in replacement for
  mistral.rs in `src/llm.rs` (same HTTP endpoint format).
- **STT model choice** — RKLLama uses omniASR-CTC on RKNN, which may be
  faster than SenseVoice for short English utterances. Worth benchmarking.

### Other RK3588 voice/AI projects

| Project | Description | Link |
|---------|-------------|------|
| Paroli | Streaming Piper TTS on NPU (4.3x speedup) | [github.com/marty1885/paroli](https://github.com/marty1885/paroli) |
| Offline voice assistant (Orange Pi) | Whisper + LLM + Piper on Orange Pi 5 Max | [Medium article](https://chlp8.medium.com/offline-voice-assistant-on-orange-pi-4cfe57480768) |
| rknn-llm | Rockchip official LLM on NPU toolkit | [github.com/airockchip/rknn-llm](https://github.com/airockchip/rknn-llm) |
| rknn_model_zoo | Reference RKNN models (YOLO, OCR, etc.) | [github.com/airockchip/rknn_model_zoo](https://github.com/airockchip/rknn_model_zoo) |

---

## References

### Official Rockchip

- [airockchip/rknn-toolkit2](https://github.com/airockchip/rknn-toolkit2) — RKNN SDK (runtime, headers, model conversion)
- [airockchip/rknn-llm](https://github.com/airockchip/rknn-llm) — RKLLM toolkit for LLMs on NPU
- [rockchip-linux/rknpu2](https://github.com/rockchip-linux/rknpu2) — NPU kernel driver + API (mirror)

### sherpa-onnx

- [sherpa-onnx GitHub](https://github.com/k2-fsa/sherpa-onnx) — C++/Rust speech toolkit
- [sherpa-onnx RKNN install docs](https://k2-fsa.github.io/sherpa/onnx/rknn/install.html)
- [RKNPU ONNX Runtime execution provider](https://onnxruntime.ai/docs/execution-providers/community-maintained/RKNPU-ExecutionProvider.html)

### Rust crates

- [rknpu2](https://crates.io/crates/rknpu2) — Rust bindings for RKNN Runtime API
- [sensevoice-rs](https://crates.io/crates/sensevoice-rs) — SenseVoice STT+VAD with RKNN
- [rktop](https://crates.io/crates/rktop) — NPU load monitor

### Community

- [awesome-RK3588](https://github.com/choushunn/awesome-RK3588) — curated RK3588 dev resources
- [Paroli](https://github.com/marty1885/paroli) — streaming Piper TTS with RK3588 NPU (4.3x speedup)
- [RKLLama](https://github.com/NotPunchnox/rkllama) — Ollama-compatible RKLLM server
- [RK3588 NPU benchmarks](https://tinycomputers.io/posts/rockchip-rk3588-npu-benchmarks.html) — real-world perf data
- [Radxa Rock 5A](https://radxa.com/products/rock5/5a/) — board manufacturer
