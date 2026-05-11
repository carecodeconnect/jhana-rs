# 09: Uctronics Audio Codec Fix for Armbian

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

## Old image audio device tree

From `hardware/uctronics-dsi/radxa-ubuntu-22.04-full.dts`:

```dts
audio-codec-0 {
    compatible = "uctronics,uctronics-codec";
    sdmode-gpios = <&gpio3 13 0>;     /* GPIO3_A5 — speaker amp enable */
    gainsel_1-gpios = <&gpio3 3 0>;   /* GPIO3_A3 — gain select 1 */
    gainsel_2-gpios = <&gpio3 5 0>;   /* GPIO3_A5 — gain select 2 */
    gainsel_3-gpios = <&gpio3 2 0>;   /* GPIO3_A2 — gain select 3 */
    #sound-dai-cells = <0>;
};

uctronics-sound {
    status = "okay";
    compatible = "rockchip,multicodecs-card";
    rockchip,card-name = "uctronics-codec";
    rockchip,format = "i2s";
    rockchip,mclk-fs = <256>;
    rockchip,cpu = <&i2s_bus>;        /* phandle 0x16f — need to identify */
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
- `sdmode` (GPIO3_A5) — speaker amplifier enable (Class D shutdown pin)
- `gainsel_1/2/3` (GPIO3_A3/A5/A2) — 3-bit gain selection

This pattern matches a **MAX98357A** (or similar I2S Class D amp) for
the speaker, plus a **digital MEMS microphone** on the same I2S bus.
The custom driver is likely a thin wrapper combining both into one
ALSA card.

### I2S bus

The uctronics codec uses **I2S2 at `0xfe480000`** (`rk3588-i2s-tdm`),
separate from the es8316's I2S controller. Pinctrl phandles: `0xf7`,
`0xf8`, `0xf9`, `0xfa`. 2-channel playback + 2-channel capture.

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

### Option A: Build out-of-tree modules (recommended, in progress)

Build `snd-soc-max98357a.ko` and `snd-soc-dmic.ko` from upstream kernel
source as out-of-tree modules (same approach as the display fix). Then
create a DT overlay.

Source: `hardware/uctronics-audio/` — Makefile, DT overlay, README.

**Progress (2026-05-11):**
1. [x] Downloaded `max98357a.c` and `dmic.c` from Linux v6.1
2. [x] Built both .ko modules successfully on Rock
3. [x] Modules load cleanly (`modprobe snd-soc-max98357a`, `modprobe snd-soc-dmic`)
4. [ ] **DT overlay broke networking** — first attempt (`uctronics-audio.dtbo`)
       caused the Rock to boot without ethernet. Overlay was reverted.
       The overlay needs debugging — likely wrong I2S node reference
       (`i2s1_8ch` alias for `i2s@fe480000`), or a dependency conflict
       with the ethernet DMA controller. Speaker pop/click was heard on
       reboot, suggesting the MAX98357A sdmode GPIO _is_ being toggled.
5. [ ] Fix DT overlay — investigate correct I2S node, test incrementally
6. [ ] Verify new ALSA card appears with speaker + mic
7. [ ] Test playback and capture

**Troubleshooting the DT overlay failure:**
- If the overlay breaks boot/networking, pull the microSD, mount on X61s,
  and edit `/boot/armbianEnv.txt` to remove `uctronics-audio` from the
  `overlays=` line. If SSH is unreachable but TUI shows on display, the
  board booted but networking failed — the overlay is the cause.
- Test overlay changes incrementally: try enabling just I2S first, then
  add codecs, then add sound card node.
- Use `ssh root@rock-5a` (Tailscale) as fallback when LAN SSH fails.

### Option B: Extract uctronics codec from baseline kernel

Same approach as the display fix — disassemble the built-in driver from
the baseline image vmlinuz. More complex than Option A since we'd need
to reverse-engineer a complete codec driver, but guaranteed to match the
original behavior.

### Option C: Recompile Armbian kernel

Enable `CONFIG_SND_SOC_MAX98357A=m` and `CONFIG_SND_SOC_DMIC=m` in the
Armbian kernel config and rebuild. Most reliable but requires full
kernel build infrastructure.

## Known issues

### Speaker pop/click on boot and reboot

The MAX98357A amp produces an audible pop/click when the sdmode GPIO
toggles (amp power on/off). This was heard on reboot after installing
the out-of-tree modules (2026-05-11). The old Radxa image had the same
issue — the original Python app's `configure_devices.sh` managed the
amp enable timing. Fix: control sdmode GPIO sequencing in the driver
or add a startup delay. Low priority — cosmetic only, not a functional
bug.

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
