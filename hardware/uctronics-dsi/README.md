# Uctronics DSI Panel — Armbian Fix

## Problem

The Uctronics AI in a Box display (720x1280 DSI) only works with the
original Radxa Ubuntu 22.04 image, which has a custom kernel driver
`panel-uctronics-lcd` (`CONFIG_DRM_PANEL_UCTRONICS_LCD=y`). This driver
is proprietary — not in any public kernel tree.

Armbian 26.2.1 (vendor kernel 6.1.115) does not include this driver.
The `rock-5a-radxa-display-8hd` overlay uses a different panel
(`radxa,display-8hd`, 800x1280) — wrong panel.

## Solution: custom DTS overlay with `panel-simple-dsi`

The Armbian kernel includes `panel-simple-dsi` — a generic DSI panel
driver that takes timing from the device tree. We create an overlay
with the exact timing extracted from the working old image.

## Timing data (extracted from running old image via debugfs)

```
Display mode: 720x1280p60
Pixel clock: 66 MHz
H: 720 760 780 835
   hactive=720, hfp=40, hsync=20, hbp=35 (htotal=835? actually 815)
V: 1280 1295 1303 1318
   vactive=1280, vfp=15, vsync=8, vbp=7 (vtotal=1310? actually 1318)
Bus format: RGB888_1X24
DSI lane rate: 480 Mbps
```

Note: H total = 720+40+20+35 = 815, but debugfs shows 835.
So hbp is likely 55 (720+40+20+55 = 835). Recalculated:
- hactive=720, hfp=40, hsync=20, hbp=55

V total = 1280+15+8+7 = 1310, but debugfs shows 1318.
So vbp is likely 15 (1280+15+8+15 = 1318). Recalculated:
- vactive=1280, vfp=15, vsync=8, vbp=15

## GPIO pinout

| Function | GPIO | DT value |
|----------|------|----------|
| Panel reset | GPIO3_C1 | `<&gpio3 RK_PC1 GPIO_ACTIVE_LOW>` |
| Backlight enable | GPIO3_D2 | `<&gpio3 RK_PD2 GPIO_ACTIVE_HIGH>` |
| Backlight PWM | PWM10 | `<&pwm10 0 25000 0>` |

## Files

- `radxa-ubuntu-22.04-full.dts` — complete device tree from old image (10,390 lines)
- `dsi-panel-node.dts` — extracted DSI/backlight/power nodes
- `rock-5a-uctronics-dsi.dts` — overlay for Armbian (TO BE CREATED)

## How to apply overlay on Armbian

```bash
# Compile overlay
dtc -@ -I dts -O dtb -o rock-5a-uctronics-dsi.dtbo rock-5a-uctronics-dsi.dts

# Install
sudo cp rock-5a-uctronics-dsi.dtbo /boot/dtb/rockchip/overlay/

# Enable in boot config
# Edit /boot/armbianEnv.txt:
overlays=rock-5a-uctronics-dsi

# Reboot
sudo reboot
```
