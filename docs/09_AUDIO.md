# 09: Uctronics Audio Codec Fix for Armbian

## Status: MIC CAPTURE + SPEAKER PLAYBACK WORKING (2026-05-15)

The Uctronics onboard mic captures real audio and the speaker plays
back on Armbian via the `uctronics-audio` overlay and the
reverse-engineered `snd-soc-uctronics-codec.ko` module.

- **Mic capture:** must be **S32_LE at 48 kHz** — see "Mic capture
  format/rate" below.
- **Speaker:** functional but **volume control via the codec's 3-bit
  gainsel GPIOs is currently a no-op on this board.** The `sdmode`
  GPIO works (we hear an amp click on probe and on each playback),
  and the `DAC Playback Volume` mixer accepts values 0–4, but all
  five values produced perceptually identical loudness in side-by-
  side tests. Working theories:
  1. The gainsel pins (claimed as `GPIO1_A2/A3/A5`) on this hardware
     revision aren't actually wired to the amp's gain-selector inputs.
  2. The amp's gain range across the 3-bit selector is too small
     (~2–3 dB total) to be perceptible.
  3. There is an additional digital/analog volume control in the
     baseline chain (e.g. an I2S-TDM digital gain) that we have not
     identified.

  Until this is understood, **boost loudness in software** — e.g.
  `ffmpeg -af "volume=20dB,alimiter=limit=0.95" …` on cue/speech WAVs
  before `aplay`. The `scripts/test-stt-tts.sh` start/stop cues now
  use this and a 1500 Hz / 700 Hz pair, which the user reports as
  clearly loud. **Cue frequency matters a lot**: the small onboard
  speaker is heavily peaked around 1–2 kHz, so a 440 Hz tone is
  almost inaudible at the same digital amplitude while a 1500 Hz
  tone is loud. Keep TTS / cues in that band.

### Reference: the original AI in a Box loudness path

We cloned the upstream `usefulsensors/ai_in_a_box` repo to
`/mnt/data/projects/ai_in_a_box` on the dev machine for reference.
Its `configure_devices.sh` reveals how the original device gets
its perceived loudness:

```bash
amixer -c 2 sset DAC 100%                     # codec gain to max (= our gain=4)
pactl set-sink-volume $DEFAULT_SINK_INDEX 0xFFFF   # PulseAudio sink volume to ~100%
```

Combined with the per-app `volume.conf` (default `8` in
`volume_file.py`) and PulseAudio's loudness-normalisation behaviour,
the path picks up an effective software-gain stage that raw
`aplay` does not have. Two ways to replicate:

1. Install PulseAudio and route playback through a default sink at
   100% (closest to baseline).
2. Apply our own ffmpeg `volume + alimiter` stage in `src/tts.rs`
   and `scripts/test-stt-tts.sh` before handing the WAV to `aplay`
   (simpler, no daemon, current choice).

Other implementation details from the reference repo worth
mirroring in jhana-rs:

- `recorder.py` sets `sd.default.dtype = 'int32', 'int32'` for both
  input and output streams — this is exactly the S32_LE we found is
  required for the I2S codec on capture. The output stream then
  overrides to `dtype="int16"` per-stream in `tts.py`.
- The mic input stream is opened *before* the TTS output stream and
  signals readiness via `/tmp/audio_input_running.bool`; TTS waits
  on that file. The codec has an input-before-output ordering
  requirement we should honour in `src/main.rs` once we wire TTS
  back up.

- **GPIO bank correction (2026-05-15):** the overlay used `&gpio3`
  for `sdmode-gpios` / `gainsel_*-gpios` for several iterations
  before we noticed the baseline DT phandle `0x16a` is
  `gpio@fec20000` = **GPIO bank 1**, not bank 3. With `&gpio3` the
  codec driver was trying to claim pins in a bank where Armbian
  pinmuxes them to other functions, which **hung systemd at boot
  every time**. The fix is `&gpio1` (`GPIO1_B5` for sdmode and
  `GPIO1_A2/A3/A5` for the three gainsel bits). See
  `docs/12_TROUBLESHOOTING.md` for the full timeline.

The earlier diagnosis ("TRCM clock gated to TX") was a red herring
caused by reading the wrong bits of the I2S word; the clock is fine.

## Status: IN PROGRESS (2026-05-11)

The Uctronics AI in a Box onboard microphone and speaker do not work on
Armbian 26.2.1. The es8316 headphone jack (card 0) works for external
audio but the Uctronics board mic/speaker need a custom codec driver.

## Problem

The Uctronics board has a separate audio codec connected via I2S,
distinct from the es8316 headphone jack codec. The old Radxa Ubuntu
image has:

1. **Kernel module:** `snd-soc-uctronics-codec`
   (`CONFIG_SND_SOC_UCTRONICS_CODEC=y`, built-in)
2. **Device tree node:** `audio-codec-0` with compatible
   `uctronics,uctronics-codec`
3. **Sound card node:** `uctronics-sound` using
   `rockchip,multicodecs-card` linking the codec to an I2S bus
4. **PulseAudio config:** `configure_devices.sh` sets default source/sink

Armbian doesn't have this driver or device tree nodes. Only the es8316
codec loads (card 0: `rockchip-es8316`).

## Original AI in a Box audio setup

Source: [usefulsensors/ai_in_a_box](https://github.com/usefulsensors/ai_in_a_box)
(also at `moonshine-ai/ai_in_a_box`). Local copy: `~/projects/ai_in_a_box/`.

### Baseline image

The audio hardware requires a **custom baseline image** from Useful Sensors
that has the Uctronics kernel drivers and device tree baked in:

```
https://storage.googleapis.com/download.usefulsensors.com/ai_in_a_box/ai_in_a_box_baseline_16gb_20240125.img.gz
```

This image (Radxa Ubuntu 22.04, kernel 5.10.110-102-rockchip) includes:
- `CONFIG_SND_SOC_UCTRONICS_CODEC=y` — custom codec driver built into kernel
- Device tree nodes for `audio-codec-0` and `uctronics-sound`
- Custom display driver (`CONFIG_DRM_PANEL_UCTRONICS_LCD=y`)

**The audio driver source is proprietary and not in the AI in a Box repo.**
The repo only contains application code, not kernel drivers. The driver
is baked into the baseline image kernel.

### PulseAudio requirement

The original setup uses PulseAudio, not raw ALSA:

```bash
# From run_chatty.sh:
sudo pulseaudio --start
./configure_devices.sh    # sets default source/sink
```

Audio devices appear as:
- Input: `alsa_input.platform-uctronics-sound.stereo-fallback`
- Output: `alsa_output.platform-uctronics-sound.stereo-fallback`

### Audio input/output ordering constraint

**Audio input must be configured before audio output.** The original code
enforces this with a signal file:

1. `recorder.py` opens mic input stream first (`sd.InputStream`)
2. Creates `/tmp/audio_input_running.bool` when input is ready
3. `tts.py` waits for that file before opening output stream (`sd.OutputStream`)

This ordering is required by the Uctronics audio driver — opening output
before input causes failures.

### configure_devices.sh behavior

Sets PulseAudio defaults and volumes:
- Detects USB audio devices (experimental, not reliable)
- Sets uctronics mic input to max volume (`0xFFFF`)
- Sets uctronics speaker output to max volume, `amixer -c 2 sset DAC 100%`
- Non-uctronics devices get lower default volumes

### Key finding: NOT a standard MAX98357A

The `uctronics,uctronics-codec` driver is **not a standard MAX98357A +
DMIC combination**. It is a custom Uctronics driver that:
- Has `gainsel_1/2/3` GPIOs (3-bit gain selection) — MAX98357A only has sdmode
- Wraps both speaker amp and MEMS mic into a single ALSA card
- Has specific input/output ordering requirements
- Uses a proprietary codec implementation baked into the kernel

Using separate upstream `snd-soc-max98357a.ko` + `snd-soc-dmic.ko` is
unlikely to work correctly because the hardware wiring and control scheme
differ from standard MAX98357A.

## Old image audio device tree

From `hardware/uctronics-dsi/radxa-ubuntu-22.04-full.dts`:

```dts
audio-codec-0 {
    compatible = "uctronics,uctronics-codec";
    sdmode-gpios = <&gpio1 13 0>;     /* GPIO1_B5 — speaker amp enable */
    gainsel_1-gpios = <&gpio1 3 0>;   /* GPIO1_A3 — gain select 1 */
    gainsel_2-gpios = <&gpio1 5 0>;   /* GPIO1_A5 — gain select 2 */
    gainsel_3-gpios = <&gpio1 2 0>;   /* GPIO1_A2 — gain select 3 */
    #sound-dai-cells = <0>;
};

// NOTE: the GPIO pins above are on bank 1, not bank 3. Earlier
// drafts of this document had &gpio3 — that was a misread of the
// baseline DT (phandle 0x16a → gpio@fec20000 = GPIO1).

uctronics-sound {
    status = "okay";
    compatible = "rockchip,multicodecs-card";
    rockchip,card-name = "uctronics-codec";
    rockchip,format = "i2s";
    rockchip,mclk-fs = <256>;
    rockchip,cpu = <&i2s_bus>;        /* phandle 0x16f — I2S1_8CH at 0xfe480000 */
    rockchip,codec = <&audio_codec_0>;
    io-channels = <&saradc 3>;
    io-channel-names = "adc-detect";
    keyup-threshold-microvolt = <1800000>;
};
```

## Old image audio card layout

| Card | Name | Device |
|------|------|--------|
| 0 | rockchip-es8316 | 3.5mm headphone/line (analog) |
| 1 | rockchip-hdmi0 | HDMI audio |
| 2 | uctronics-codec | Onboard mic + speaker (I2S) |
| 3 | rockchip-hdmi1 | HDMI audio (DP) |

## Armbian audio card layout (current)

| Card | Name | Device |
|------|------|--------|
| 0 | rockchip-es8316 | 3.5mm headphone/line (analog) |
| 1 | rockchip-hdmi0 | HDMI audio |
| 2 | rockchip-hdmi1 | HDMI audio (DP) |

Missing: uctronics-codec (card 2 on old image)

## Hardware identification

The `uctronics,uctronics-codec` driver has GPIO pins for:
- `sdmode` (GPIO1_B5) — speaker amplifier enable (Class D shutdown pin)
- `gainsel_1/2/3` (GPIO1_A3/A5/A2) — 3-bit gain selection

This is similar to a MAX98357A but with additional gain control GPIOs
that standard MAX98357A does not have. The custom driver wraps both
speaker amp and MEMS mic into one ALSA card.

### I2S bus

The uctronics codec uses **I2S1_8CH at `0xfe480000`** (`rk3588-i2s-tdm`),
separate from the es8316's I2S controller. This is aliased as `i2s1_8ch`
in the Armbian device tree. It is **disabled** by default in Armbian.

## Armbian kernel audio support

```
CONFIG_SND_SOC_ROCKCHIP_MULTICODECS=y    # available (sound card framework)
CONFIG_SND_SOC_MAX98357A is not set      # NOT available
CONFIG_SND_SOC_DMIC is not set           # NOT available
CONFIG_SND_SOC_SIMPLE_AMPLIFIER is not set  # NOT available
CONFIG_SND_SIMPLE_CARD=y                 # available
```

**Neither MAX98357A nor DMIC drivers are in the Armbian kernel.**

## Fix plan

### Option B: Extract uctronics codec from baseline kernel (RECOMMENDED)

Same approach as the display fix — extract the `snd-soc-uctronics-codec`
driver from the baseline image kernel (`5.10.110-102-rockchip`). This is
now the recommended approach after discovering that the hardware is NOT
a standard MAX98357A + DMIC.

**Steps:**
1. Mount the baseline image (`ai_in_a_box_baseline_16gb_20240125.img.gz`)
2. Extract vmlinuz + System.map (same as display fix)
3. Find `uctronics_codec` symbols in System.map
4. Disassemble the codec driver (native `objdump` on Rock)
5. Rewrite as a loadable kernel module for 6.1.115-vendor-rk35xx
6. Create DT overlay with `audio-codec-0` and `uctronics-sound` nodes
7. Enable I2S1_8CH (`i2s@fe480000`) in the overlay
8. Install module + overlay, test

**Baseline image download:**
```bash
curl -L -O https://storage.googleapis.com/download.usefulsensors.com/ai_in_a_box/ai_in_a_box_baseline_16gb_20240125.img.gz
```

This is the same image used for the display fix — it may already be
downloaded. The display driver was at `jadard_jd9365da_enable` in
System.map; the audio codec will be near `uctronics_codec`.

### Option A: Build standard MAX98357A + DMIC modules (ABANDONED)

Built `snd-soc-max98357a.ko` and `snd-soc-dmic.ko` from upstream kernel
source. Modules compile and load, but:
- The hardware is NOT a standard MAX98357A (has extra gain GPIOs)
- The DT overlay broke networking on first attempt
- Even if the overlay worked, standard drivers likely won't match the
  Uctronics hardware wiring

Source files still in `hardware/uctronics-audio/` for reference.

### Option C: Recompile Armbian kernel

Enable the Uctronics codec in the Armbian kernel config and rebuild.
Most reliable but requires full kernel build infrastructure and the
Uctronics driver source (which is proprietary).

## Mic capture format/rate

The Uctronics MEMS mic feeds the RK3588 I2S1_8CH (`rockchip-i2s-tdm`)
controller. Captured frames arrive as 24-bit samples in 32-bit words.
Capturing with `arecord -f S16_LE` reads the low half of the word, which
contains only noise / dither / a small DC bias — producing recordings
that look like garbage even though the codec is correctly registered.

**Use S32_LE at 48 kHz:**

```bash
arecord -D plughw:1,0 -f S32_LE -r 48000 -c 1 -d 5 mic.wav
```

Verified 2026-05-15: standalone capture (no concurrent playback) shows
clean signal in the high bits; a mic tap registers a peak of ~3.6×10^8
out of 2^31 (~17 % full-scale) and is clearly visible in 0.5 s window
RMS readings. With S16_LE on the same setup the tap was indistinguishable
from noise.

`plughw:` doesn't rescue the situation if you ask it for S16_LE 16 kHz —
the conversion is done on already-truncated samples. Capture at S32_LE
48 kHz natively, then resample / down-quantize in user code (e.g. via
SenseVoice's internal resampler, or `ffmpeg -i in.wav -ar 16000 -sample_fmt s16 out.wav`).

`src/stt.rs` is configured for `plughw:1,0`, S32_LE, 48 kHz.

## DO NOT touch `rockchip,clk-trcm` from an overlay (2026-05-15)

Two attempts to override the i2s@fe480000 clock-mode property via the
audio overlay broke boot:

1. Adding `rockchip,clk-trcm = <1>` *and* `/delete-property/ rockchip,trcm-sync-tx-only`
   → kernel boots far enough to print TTY output, systemd never reaches
   getty, no networking, no Tailscale.
2. Adding only `rockchip,clk-trcm = <1>` (no delete-property) → same
   failure mode.

Recovery in both cases required pulling the microSD on the dev machine
and removing `uctronics-audio` from `overlays=` in
`/boot/armbianEnv.txt`. The original overlay (with neither property)
boots cleanly and captures audio correctly once S32_LE is used.

The 6.1.115-vendor-rk35xx `rockchip-i2s-tdm` driver apparently expects
something additional (specific clocks/regulators?) when `clk-trcm=1` that
the Useful Sensors 5.10 baseline DT provides but our overlay doesn't.
Not worth chasing now since S32_LE makes the mic work without changing
the clock mode.

## Known issues

### Speaker pop/click on boot and reboot

The speaker amp produces an audible pop/click when the sdmode GPIO
toggles (amp power on/off). This was heard on reboot after installing
the out-of-tree modules (2026-05-11). The old Radxa image had the same
issue — the original Python app's `configure_devices.sh` managed the
amp enable timing. Fix: control sdmode GPIO sequencing in the driver
or add a startup delay. Low priority — cosmetic only, not a functional
bug.

### Audio input must start before output

The original AI in a Box enforces mic input → speaker output ordering.
The jhana-rs code should respect this constraint once the audio codec
is working. See `recorder.py` and `tts.py` in the AI in a Box repo.

## Troubleshooting

### DT overlay breaks networking

If an audio overlay breaks boot/networking:
1. Pull the microSD, mount on X61s
2. Edit `/boot/armbianEnv.txt` — remove the audio overlay from `overlays=`
3. Unmount, put card back in Rock, reboot

If SSH is unreachable but TUI shows on display, the board booted but
networking failed — the overlay is the cause.

### Fallback SSH access

- `ssh root@rock-5a` via Tailscale when LAN SSH fails
- If Tailscale also fails, pull the card and edit boot config

## ALSA mixer notes (es8316, card 0)

The es8316 has both analog (lin1/lin2) and digital mic inputs:
- `Differential Mux`: `lin1-rin1`, `lin2-rin2`, with/without 20db boost
- `Digital Mic Mux`: `dmic disable`, `dmic data at high/low level`
- `ADC PGA Gain`: 0-10
- `ADC Capture Volume`: 0-192

Testing showed the analog inputs capture only noise (not connected to
the onboard mic). The DMIC input captured signal but it's from the
es8316's DMIC pins, not the Uctronics board mic — they're different
hardware.

## References

- [usefulsensors/ai_in_a_box](https://github.com/usefulsensors/ai_in_a_box) — original Python app
- [moonshine-ai/ai_in_a_box](https://github.com/moonshine-ai/ai_in_a_box) — mirror
- [Baseline image](https://storage.googleapis.com/download.usefulsensors.com/ai_in_a_box/ai_in_a_box_baseline_16gb_20240125.img.gz) — 2.4 GB, contains custom kernel with audio + display drivers
- Local copy of AI in a Box: `~/projects/ai_in_a_box/`
