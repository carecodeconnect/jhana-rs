# Benchmarks

Measured performance of jhana-rs components on the Rock 5A
(RK3588S, 8 GB RAM, Armbian 6.1.115-vendor-rk35xx).

Numbers are consolidated from earlier benchmarking embedded in
`03_TTS.md`, `05_NPU.md`, and `06_KERNEL.md`. New runs should be
appended here, not back into those docs.

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
| **espeak-ng**  | (formant synth, no model)      | CPU           | ~0.02 s synth per cue      | Current baseline TTS for jhana-rs (2026-05-15). Robotic but always works. Used for TUI cues and meditation narration until Piper-rs lands. |
| Piper CLI      | en_US-lessac-medium (~60 MB ONNX) | CPU         | 0.75–0.87 s per ~7-word sentence, RTF ~0.31 | Working stack on the Radxa 5.10 image (2026-05-07). Currently **broken** on Armbian 6.1 due to libpiper_phonemize ↔ espeak-ng symbol mismatch; see `12_TROUBLESHOOTING.md`. |
| Piper CLI w/ `rknpu` provider | same           | (fell back to CPU) | Same as CPU       | `rknpu` provider in sherpa-onnx fell back to CPU; not recognised as an EP. |
| Paroli (Piper-rknn, external) | Piper VITS               | RK3588 NPU    | 4.3× speedup vs CPU       | Independent benchmark from [marty1885/paroli](https://github.com/marty1885/paroli). Indicates the NPU path is worthwhile for TTS once we move off Piper CLI. |

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
