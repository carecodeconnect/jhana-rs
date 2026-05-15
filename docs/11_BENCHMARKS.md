# Benchmarks

Measured performance of jhana-rs components on the Rock 5A
(RK3588S, 8 GB RAM, Armbian 6.1.115-vendor-rk35xx).

Numbers are consolidated from earlier benchmarking embedded in
`03_TTS.md`, `05_NPU.md`, and `06_KERNEL.md`. New runs should be
appended here, not back into those docs.

## RAM efficiency (Rust-side levers)

The Rock 5A has unified 8 GB DRAM shared by CPU + NPU + GPU + VPU.
Every MB the userland keeps live is a MB the LLM doesn't get. Rust
patterns that help here, in order of payoff:

| Lever                                                                  | Where in jhana-rs                            | Status (2026-05-15) | Saving                          |
|------------------------------------------------------------------------|----------------------------------------------|---------------------|---------------------------------|
| **Cap `App.all_lines` as `VecDeque<Box<str>>`**                        | `src/ui.rs`                                  | **landed**          | Bounded growth (was unbounded `Vec<String>`); ~1–50 MB over long sessions; also drops `String`'s capacity word per finished line. |
| **Set `max_context_len = 1024` on RKLLM**                              | `src/llm.rs::get_or_load_model`              | **landed**          | KV cache stops growing past 1024 tokens (model baked-in is 4096) — hundreds of MB freed depending on quant + layer count. |
| **Size-optimised `[profile.release]`** (`opt-level = "z"`, fat LTO, 1 codegen unit, strip, `panic = "abort"`) | `Cargo.toml`                                 | **landed**          | Smaller resident binary text; faster cold start. |
| **`mpsc::sync_channel(N)`** instead of unbounded `channel`             | `src/main.rs`, `src/tts.rs`, `src/stt.rs`    | TODO                | Bounds peak channel-buffer RAM; backpressures LLM if TTS lags. |
| **`Arc<str>` shared sentence** between TUI + TTS + log                 | `src/main.rs` per-sentence dispatch          | TODO                | One alloc per sentence instead of three. |
| **`Vec::with_capacity()`** on `ChunkParser` buffer                     | `src/llm.rs`                                 | TODO                | Avoids realloc churn during streaming. |
| **Delete recorded WAV after STT inference**                            | `src/stt.rs::listen_and_transcribe`          | TODO                | Frees `/tmp` (RAM-backed on Armbian) once per press. |
| **`musl` static binary** instead of `gnu` glibc                         | `.cargo/config.toml` target spec             | TODO                | Smaller libc resident pages; fully self-contained binary. |
| **Drop SenseVoice when idle** (reload-on-demand)                       | `src/stt.rs::stt_loop`                       | Won't fix           | ~1 GB recoverable but adds 22 s cold-load on every press — not worth. |

Single biggest non-Rust lever: **smaller model** — swap
Llama-3.2-3B (4 GB) for Llama-3.2-1B (~1 GB), freeing ~3 GB at a
quality cost. Tracked in `14_TODO.md` and `10_SPECS.md`.

## LLM (decode throughput)

| Model                                | Quant   | Engine      | Hardware      | Load (cold) | First token | Decode    | Notes / date |
|--------------------------------------|---------|-------------|---------------|-------------|-------------|-----------|---------------|
| **Llama-3.2-3B-Instruct**            | w8a8 g128 | rkllm-rs / librkllmrt 1.2.3 | RK3588 NPU (3 cores) | **74 s**  | 2.17 s    | **4.5 tok/s** | **Current default** (2026-05-15). Generated coherent meditation guidance. Driver v0.9.8, `cma=0` (CMA disabled, IOMMU page-based path). |
| Ministral-3B-Instruct                | Q4_K_M  | mistral.rs (HTTP, CPU) | 4× Cortex-A76    | n/a         | n/a         | ~3.9 tok/s    | Pre-NPU baseline (2026-05-07). Used while NPU path was blocked. |
| Gemma-3 270M (vendor 5.10 kernel)    | w8a8    | rkllm (older driver) | RK3588 NPU 0.8.2 | 1.24 s    | n/a         | 66 tok/s     | "Compute is fast but **matmul output garbage**" — driver pre-0.9.7 didn't support w8a8 matmul correctly. Historical only; we are now on 0.9.8 where this works. |
| TinyLlama 1.1B (external)            | ?       | rkllm       | RK3588 NPU    | n/a         | n/a         | 10–15 tok/s | Reported by [TinyComputers](https://tinycomputers.io/posts/rockchip-rk3588-npu-benchmarks.html). Upper bound for what to expect from sub-2 B models on this NPU. |

### LLM benchmark notes

- 4.5 tok/s is a comfortable margin above natural meditation speech
  pacing (~2–3 words/sec, i.e. ~3–4 tok/s). The user will not hear
  "the LLM struggling to keep up" at this rate.
- The 74 s cold load matters operationally: the TUI must not show a
  "failed" state inside that window. `src/llm.rs` should be checked
  for a load timeout ≥ 90 s (currently 60 s — bug filed in 14_TODO.md
  via this doc).
- Subsequent loads in the same session are faster (page-cache hit on
  the 4.35 GB `.rkllm` blob) — not yet measured but expected.
- The 66 tok/s Gemma-270M figure is historical and was on the broken
  v0.8.2 driver — keep it in the table to explain why "fast decode
  numbers from 2026-05-08 don't imply the model worked".

## STT

| Model                                 | Engine       | Hardware   | Inference time (5 s audio) | Notes / date |
|---------------------------------------|--------------|------------|----------------------------|---------------|
| **SenseVoiceSmall (CN/JP/YUE/EN/KO)** | sensevoice-rs (RKNN) | RK3588 NPU | **0.5–7 s** | Production STT. Cold path ~7 s (model not in cache); warm path ~0.35–0.5 s. Driver 0.9.8, model in `~/models/sensevoice/`. End-to-end tested 2026-05-15 from the TUI: `"please teach me meditation thank you"`. |
| SenseVoiceSmall — model load (cold)   | sensevoice-rs | NPU        | 21 s (first init)          | One-time per process. 0.6 s on warm cache. |

## TTS

| Engine        | Model                          | Hardware      | RTF / Latency             | Notes / date |
|---------------|--------------------------------|---------------|---------------------------|---------------|
| espeak-ng     | (formant synth, no model)      | CPU           | ~0.02 s synth per cue      | Fallback baseline. Robotic. Used only when paroli fails or for the "Speak now" cue (now also paroli-rendered at startup; cached). |
| Piper CLI      | en_US-lessac-medium (~60 MB ONNX) | CPU         | 0.75–0.87 s per ~7-word sentence, RTF ~0.31 | Working stack on the Radxa 5.10 image (2026-05-07). Broken on Armbian 6.1 (libpiper_phonemize ↔ espeak-ng symbol mismatch); see `12_TROUBLESHOOTING.md`. |
| paroli (streaming Piper VITS) | ljspeech encoder.onnx + decoder.onnx (114 MB) | CPU (4× A76) | **RTF ≈ 1.46** (1.62 s infer / 5.62 s audio) — slower than playback | Measured on the Rock 5A, 2026-05-15. Worked but TTS queue lagged the LLM stream by tens of seconds; tool effects (bell) fired late as a result. |
| **paroli (streaming Piper VITS)** | ljspeech encoder.onnx + decoder.**rknn** (47 MB) | **RK3588 NPU** | **RTF ≈ 0.29** (1.62 s infer / 5.62 s audio) — **3.5× faster than playback** | **Current default (2026-05-15).** ~5× speedup vs the CPU build above. The TTS queue stays ahead of the LLM stream, so bell/pause tool calls now fire close to when the model emits them. **User-reported voice quality is also noticeably better on the NPU build than the CPU one** — likely because CPU synth was so slow that the playback was hitting buffer-underflow gaps; the NPU run produces a clean continuous waveform. |

## Audio I/O

| Path                          | Format              | Notes / date |
|-------------------------------|---------------------|---------------|
| Mic (Uctronics, ALSA card 1)   | S32_LE 48 kHz mono  | Native format. S16_LE 16 kHz capture reads the wrong bits of the I2S word and yields DC-offset garbage — must capture S32 48 kHz and resample. See `09_AUDIO.md`. |
| Mic → SenseVoice resample      | ffmpeg `-ac 1 -ar 16000 -sample_fmt s16` | ~0.02 s (negligible). |
| Speaker (Uctronics, card 1)    | plughw resamples from any → 48 kHz codec rate | Speaker amp gain selector wired (`sdmode-gpios` etc on `&gpio1`), but the 5-level gain control is not perceptible on this hardware revision. Loudness on small-driver speech needs `ffmpeg volume=+6dB` (more = distortion). |

## How to add a new benchmark

1. Run the workload three times (warm) and pick the median.
2. Note: model file size, engine + version, driver version, kernel
   cmdline (esp. `cma=`), date.
3. Append a row to the relevant table above.
4. If the run took unusual setup (kernel rebuild, special model
   conversion), cross-link to the doc that describes it.
5. Avoid re-adding benchmarks to `03_TTS.md` / `05_NPU.md` —
   this doc is now the canonical place.
