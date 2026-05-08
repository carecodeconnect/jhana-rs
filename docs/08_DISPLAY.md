# 08: Uctronics DSI Display Fix for Armbian

## Problem

The Uctronics AI in a Box display (720x1280 DSI portrait) only works
with the original Radxa Ubuntu 22.04 image. That image has a custom
kernel panel driver `panel-uctronics-lcd` (`CONFIG_DRM_PANEL_UCTRONICS_LCD=y`)
which is proprietary — not in any public kernel tree.

Armbian 26.2.1 (vendor kernel 6.1.115) does not include this driver.
The backlight powers on but the panel shows nothing.

The `rock-5a-radxa-display-8hd` overlay was tested and does not work —
it uses a different panel (`radxa,display-8hd`, 800x1280).

## Display timings (extracted from working old image)

Extracted via debugfs (`/sys/kernel/debug/dri/0/summary`) on the
running Radxa Ubuntu 22.04 image (2026-05-08):

```
Display mode: 720x1280p60
Pixel clock:  66 MHz
H: 720 760 780 835 → hactive=720, hfp=40, hsync=20, hbp=55
V: 1280 1295 1303 1318 → vactive=1280, vfp=15, vsync=8, vbp=15
Bus format:   RGB888_1X24
DSI lane rate: 480 Mbps
4 data lanes
```

## GPIO pinout (from old device tree)

| Function | GPIO | Pin | DT reference |
|----------|------|-----|-------------|
| Panel reset | GPIO3_C1 | `<&gpio3 17 1>` | active low |
| Backlight enable | GPIO3_D2 | `<&gpio3 26 0>` | active high |
| Backlight PWM | PWM10 | `<&pwm10 0 25000 0>` | 25 kHz |

## Fix: custom device tree overlay

A DTS overlay using `simple-panel-dsi` (generic DSI panel driver in
the kernel) with the exact timings from above.

File: `hardware/uctronics-dsi/rock-5a-uctronics-dsi.dts`

### Build and install on Armbian

```bash
# On the Rock (running Armbian):

# Compile the overlay
sudo apt install device-tree-compiler
dtc -@ -I dts -O dtb -o rock-5a-uctronics-dsi.dtbo \
  hardware/uctronics-dsi/rock-5a-uctronics-dsi.dts

# Install
sudo cp rock-5a-uctronics-dsi.dtbo /boot/dtb/rockchip/overlay/

# Enable — edit /boot/armbianEnv.txt:
# Replace any existing overlays= line with:
overlays=rock-5a-uctronics-dsi

# Reboot
sudo reboot
```

### If `simple-panel-dsi` is not in the kernel

Check with:
```bash
zcat /proc/config.gz | grep PANEL_SIMPLE
# Need: CONFIG_DRM_PANEL_SIMPLE=y or =m
```

If not available, alternative approaches:
1. Build the Uctronics panel driver from source (source is proprietary,
   not publicly available — see below)
2. Use `panel-dsi` generic driver instead of `simple-panel-dsi`

## Source files saved

| File | Description |
|------|-------------|
| `hardware/uctronics-dsi/radxa-ubuntu-22.04-full.dts` | Complete device tree from working old image (10,390 lines) |
| `hardware/uctronics-dsi/dsi-panel-node.dts` | Extracted DSI/backlight/power nodes |
| `hardware/uctronics-dsi/rock-5a-uctronics-dsi.dts` | Custom overlay for Armbian |
| `hardware/uctronics-dsi/README.md` | Build instructions |

## Driver source investigation (2026-05-08)

The `panel-uctronics-lcd.c` source was searched in:
- github.com/radxa/kernel (branches linux-5.10-gen-rkr4.1, stable-5.10-rock5)
- github.com/rockchip-linux/kernel
- github.com/UCTRONICS (57 repos, only RPi LCD drivers)
- github.com/moonshine-ai/ai_in_a_box (Python only, no kernel code)
- Web search for CONFIG_DRM_PANEL_UCTRONICS_LCD

**Result: not publicly available.** The driver is proprietary, embedded
in the Useful Sensors / UCTRONICS pre-built baseline image. The baseline
image can be downloaded from:
`https://storage.googleapis.com/download.usefulsensors.com/ai_in_a_box/ai_in_a_box_baseline_16gb_20240125.img.gz`

An unanswered GitHub issue exists:
[usefulsensors/ai_in_a_box#6](https://github.com/usefulsensors/ai_in_a_box/issues/6)

## Kernel symbols from old image

```
ffffffc0106df7b4 t uctronics_display_remove
ffffffc010e37494 t uctronics_display_probe
ffffffc010fa41b0 d uctronics_display_of_match
ffffffc010fa4340 d uctronics_display_desc
ffffffc01147eef0 t uctronics_display_driver_init
ffffffc0114a2140 t uctronics_display_driver_exit
ffffffc011c71e58 d uctronics_display_driver
```

The `dmesg` output also shows `arducam add mipi display test` during
panel probe, suggesting the panel may be an Arducam-sourced component.
