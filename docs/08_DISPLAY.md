# 08: Uctronics DSI Display Fix for Armbian

## Status: FIXED (2026-05-08)

The Uctronics AI in a Box display (720x1280 DSI portrait) now works on
Armbian 26.2.1 (kernel 6.1.115-vendor-rk35xx) using a forked
`panel-ilitek-ili9881c` kernel module with the correct init sequence.

## Solution

Forked the Armbian `panel-ilitek-ili9881c.ko` driver, added a new panel
entry (`radxa,display-8hd` compatible) with the Uctronics-specific init
sequence extracted from the baseline image kernel.

### Files

| File | Description |
|------|-------------|
| `hardware/uctronics-dsi/panel-ilitek-ili9881c.c` | Forked ILI9881C driver with uctronics init |
| `hardware/uctronics-dsi/ili9881c-init-extracted.c` | Documented init sequence (reference) |
| `hardware/uctronics-dsi/Makefile` | Out-of-tree kernel module build |
| `hardware/uctronics-dsi/panel-init-sequence.txt` | Rockchip DTS format init (reference) |

### How to build and install

```bash
# On the Rock (Armbian):
cd /home/ubuntu/uctronics-dsi
make clean && make

# Install as replacement for stock 8HD panel module
MODDIR=/lib/modules/$(uname -r)/kernel/drivers/gpu/drm/panel
sudo cp $MODDIR/panel-radxa-display-8hd.ko $MODDIR/panel-radxa-display-8hd.ko.stock
sudo cp panel-ilitek-ili9881c.ko $MODDIR/panel-radxa-display-8hd.ko
sudo depmod -a
sudo reboot
```

### Armbian boot config

`/boot/armbianEnv.txt` must have:
```
overlays=rock-5a-radxa-display-8hd
```

The stock `rock-5a-radxa-display-8hd` overlay provides the correct DSI
subsystem setup (PHY, VOP routing, pinctrl, GPIO, backlight). Our forked
module replaces only the panel driver, keeping the overlay unchanged.

## Panel IC: ILI9881C

The panel controller is an **ILI9881C** — confirmed by disassembling the
working baseline image kernel (`5.10.110-102-rockchip`). The uctronics
driver in the old kernel wraps ILI9881C page-switch commands (`0xFF 0x98
0x81 0xNN`) inside a forked JD9365DA driver framework.

## Init sequence extraction

The correct init sequence was extracted from the Useful Sensors baseline
image (`ai_in_a_box_baseline_16gb_20240125.img.gz`, 2.4 GB) by:

1. Mounting the image, extracting vmlinuz + System.map
2. Finding `jadard_jd9365da_enable` at `0x6df990` in System.map
3. Disassembling on the Rock (native aarch64 `objdump`)
4. Tracing all 200 `mipi_dsi_dcs_write` calls and their arguments
5. Extracting register/value pairs from ARM64 `mov`/`strb` immediates

The init has 200 DCS commands across 4 ILI9881C pages:
- **Page 3** (GIP timing): 128 register writes (0x01-0x8A)
- **Page 4** (power/MIPI): 13 register writes
- **Page 1** (VCOM/power): 7 register writes
- **Page 0** (gamma/display): 45 register writes + sleep out + display on

### Key driver settings

```c
.mode_flags = MIPI_DSI_MODE_VIDEO | MIPI_DSI_MODE_VIDEO_BURST |
              MIPI_DSI_MODE_NO_EOT_PACKET,
```

`MIPI_DSI_MODE_LPM` must NOT be set — the original driver doesn't use it.

## Display timings

```
Display mode: 720x1280p60
Pixel clock:  66 MHz
H: hactive=720, hfp=40, hsync=20, hbp=55 (htotal=835)
V: vactive=1280, vfp=15, vsync=8, vbp=15 (vtotal=1318)
Bus format:   RGB888_1X24
DSI:          480 Mbps x 4 lanes
```

## GPIO pinout

| Function | GPIO | State |
|----------|------|-------|
| Panel power (vdd) | GPIO3_A4 (gpio-132) | active high |
| Panel reset | GPIO3_C1 (gpio-113) | active low |
| Backlight enable | GPIO3_D2 (gpio-122) | active high |
| Backlight PWM | PWM10 | 25 kHz |

## Investigation notes

The display fix required extensive reverse engineering because the
uctronics panel driver source is proprietary and not publicly available.

### Wrong turns

- Binary pattern search found BananaPi init data, not uctronics
- Misidentified IC as JD9365DA (misleading kernel symbol names)
- Init data encoded as ARM64 immediates, not a data table — binary search failed
- `MIPI_DSI_MODE_LPM` flag prevented display output

### What worked

Downloaded baseline image, disassembled enable function with native
`objdump` on Rock, traced all 200 `mipi_dsi_dcs_write` calls.
