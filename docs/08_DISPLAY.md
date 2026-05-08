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

## Panel IC identified: ILI9881C (2026-05-08)

The panel controller is an **ILI9881C** — confirmed by finding ILI9881
page-switching commands (`98 81 03`, `98 81 04`, `98 81 01`, `98 81 00`)
in the old kernel binary near the uctronics driver data.

The Armbian kernel already has `panel-ilitek-ili9881c.ko` which supports
720x1280 panels via the `bananapi,lhr050h41` compatible string. The init
sequence will differ from the BananaPi panel but the basic ILI9881C
protocol is the same.

## ILI9881C init sequence extracted (2026-05-08)

The full init sequence (188 register writes, 5 page switches) was
extracted from the old kernel binary by:

1. Searching vmlinuz for `FF 98 81 XX` (ILI9881C page-switch commands)
2. Finding the contiguous register-value data block near the
   `uctronics_display_desc` symbol (offset 16399752 in vmlinuz)
3. Parsing `E0 XX` as page switches, `RR VV` as register-value pairs
4. Converting to `ILI9881C_SWITCH_PAGE_INSTR` / `ILI9881C_COMMAND_INSTR`
   format matching the existing `panel-ilitek-ili9881c.c` driver

Saved at: `hardware/uctronics-dsi/ili9881c-init-sequence.c`

The BananaPi LHR050H41 init (also ILI9881C, 720x1280) was tested —
DSI connects, backlight brighter, but no image. The Uctronics panel
needs its own specific init sequence.

## Testing log (2026-05-08)

| Overlay | Panel driver | Compatible | Backlight | DSI | Text | Boot |
|---------|-------------|-----------|-----------|-----|------|------|
| stock 8HD | panel-radxa-display-8hd | radxa,display-8hd | Bright | 800x1280 | **No** | Fast |
| patched 8HD | panel-ilitek-ili9881c (stock) | bananapi,lhr050h41 | Brighter | 720x1280 | **No** | 4 min |
| patched 8HD | panel-ilitek-ili9881c (ours) | uctronics,uctronics-lcd | Dim | 720x1280 | **No** | Fast |
| custom overlay | panel-uctronics-lcd (minimal) | uctronics,uctronics-lcd | None | 720x1280 | **No** | Fast |

**Conclusion:** Backlight and DSI connection work, but no panel driver
produces visible pixels. The ILI9881C init sequence extracted from the
kernel binary may be incomplete or at the wrong offset. The panel IC
is confirmed as ILI9881C but the exact register init is critical — even
one wrong value in the GIP timing (page 2) prevents display output.

**Root cause:** The extracted init sequence is wrong. The ILI9881C page
switch bytes found near the uctronics strings were likely from the
`drm_panel_desc` struct metadata, not the actual init command array.
The panel powers on (GPIO 132 high after fixing vdd regulator) and
backlight works (PWM running) but the ILI9881C controller is not
configured to accept video data — framebuffer writes to `/dev/fb0`
produce no visible output.

**Next steps (priority order):**
1. **Disassemble the probe function** — boot old image, use `objdump`
   to disassemble `uctronics_display_probe` (at ffffffc010e37494) and
   trace the init array pointer. The function loads the init data via
   `adrp`/`add` instructions that encode the exact offset.
2. **DSI bus sniffing** — use `/dev/mem` to read the DSI controller's
   TX FIFO registers during panel init on the old image, capturing
   the exact byte stream sent to the panel.
3. **Contact Arducam/Uctronics** — request the panel driver source
   or the panel IC datasheet with the init sequence. File an issue
   at [github.com/ArduCAM/RK_Kernel](https://github.com/ArduCAM/RK_Kernel).
4. **Download the baseline image** (2.5 GB from storage.googleapis.com)
   and extract the driver from it — may have source in `/usr/src/`.

**What works so far:**
- Stock 8HD overlay correctly enables DSI subsystem (PHY, VOP routing, pinctrl)
- Our forked ILI9881C driver compiles and loads as `panel-radxa-display-8hd.ko`
- Panel power (GPIO 132) and backlight (GPIO 122 + PWM10) are working
- DSI link is active at 720x1280p60, 66 MHz, 480 Mbps x 4 lanes
- Console framebuffer switches to 90x80 character mode
- Only missing: correct ILI9881C register init sequence

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

## Panel IC correction: JD9365DA, NOT ILI9881C (2026-05-08)

**Critical finding:** The Uctronics panel uses a **Jadard JD9365DA**
controller, NOT ILI9881C. Evidence:

1. The kernel string `"jadard-jd9365da\x00arducam add mipi display test"`
   at offset 0x12cf947 in vmlinuz proves the "arducam" debug print is
   inside the JD9365DA driver code.
2. The `jadard_jd9365da_enable` function at 0x6df990 immediately follows
   `uctronics_display_remove` at 0x6df7b4 in the same compilation unit.
3. `CONFIG_DRM_PANEL_JADARD_JD9365DA_H3=y` in the old kernel config.

The ILI9881C page-switch bytes (`98 81 XX`) found earlier were from a
DIFFERENT driver in the same kernel — NOT the uctronics panel.

**Armbian has `panel-jadard-jd9365da-h3.ko`** with two existing panels:
- `chongzhou,cz101b4001` (800x1280)
- `radxa,display-10hd-ad001`

The init command format is simple: `{u8 reg, u8 val}` pairs sent via
`mipi_dsi_dcs_write_buffer()`. The init sequence needs to be extracted
from the old kernel's `jadard_jd9365da_enable` function (inlined at
0x6df990, 109 `mipi_dsi_generic_write` calls).
