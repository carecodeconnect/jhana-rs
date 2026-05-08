# 09: Uctronics Audio Codec Fix for Armbian

## Status: NOT WORKING (2026-05-08)

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

## Fix plan

Same approach as the display fix:

1. Extract `snd-soc-uctronics-codec` kernel module from the baseline
   image kernel (`vmlinuz-5.10.110-102-rockchip`)
2. Disassemble the codec driver to understand the hardware interface
3. Either: build a compatible module for Armbian kernel 6.1.115, or
   find if the codec is a standard chip (e.g. MAX98357A + digital mic)
   that already has upstream kernel support
4. Create a device tree overlay adding `audio-codec-0` and
   `uctronics-sound` nodes
5. Install module + overlay, test mic and speaker

### Alternative: identify the actual codec chip

The `uctronics,uctronics-codec` driver has GPIO pins for:
- `sdmode` — likely a Class D amplifier shutdown pin (MAX98357A pattern)
- `gainsel_1/2/3` — 3-bit gain selection

This is consistent with a **MAX98357A** (or similar I2S Class D amp)
for the speaker, and a separate **digital MEMS mic** (I2S input). The
driver may be a thin wrapper combining both into one ALSA card.

If the amp is MAX98357A, Armbian already has `snd-soc-max98357a.ko`.
The DMIC may be supported by `snd-soc-dmic.ko`. A device tree overlay
could wire these up without any custom kernel module.

## Workaround

Use the es8316 headphone jack (card 0, `plughw:0,0`) with an external
mic for testing until the uctronics codec is fixed.

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
