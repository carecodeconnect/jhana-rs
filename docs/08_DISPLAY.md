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

## Forked JD9365DA driver (2026-05-08)

Created `hardware/uctronics-dsi/panel-jadard-jd9365da-h3.c` — forked
from Armbian's stock driver with a new `uctronics,uctronics-lcd` panel
entry. Includes 720x1280p60 timings (66 MHz, H:40/20/55, V:15/8/15)
and an init sequence extracted from disassembly.

| File | Description |
|------|-------------|
| `panel-jadard-jd9365da-h3.c` | Forked driver with uctronics panel entry |
| `jd9365da-init-sequence.c` | Init sequence from disassembly (109 cmds, may be wrong) |
| `panel-init-sequence.txt` | ILI9881C-format init (WRONG IC, kept for reference) |
| `Makefile` | Out-of-tree kernel module build |

**Problem: init sequence may be incomplete.** The extracted uctronics
init has a truncated page 2 (GIP timing) section — stops at register
`0x2D` while the reference panels (`cz101b4001`, `radxa-10hd`) continue
through `0x7E`. Missing GIP routing would explain why the panel doesn't
render despite correct DSI link, power, and backlight.

Additionally, the `jd9365da-init-sequence.c` file starts with
`{ 0xFF, 0x11 }` which looks like an ILI9881C register, not JD9365DA
(`0xE0` for page switching). This file likely contains incorrect data
from a bad disassembly extraction — do NOT use it.

## Current driver state on Armbian (2026-05-08)

```
Kernel:     6.1.115-vendor-rk35xx
Overlay:    rock-5a-radxa-display-8hd
Driver:     ili9881c-dsi (WRONG — should be jadard-jd9365da)
DSI:        connected, 720x1280, 480 Mbps x 4 lanes
FB:         /dev/fb0 at 720x1280
GPIO-132:   HIGH (panel power on)
GPIO-122:   HIGH (backlight enable)
GPIO-113:   LOW  (panel reset deasserted)
Problem:    ILI9881C driver sends wrong init format for JD9365DA IC
```

The overlay's `compatible` string matches `ili9881c` when it should
match `uctronics,uctronics-lcd` to load the JD9365DA driver instead.

## Useful Sensors baseline image (2026-05-08)

Downloaded the original working image from:
`https://storage.googleapis.com/download.usefulsensors.com/ai_in_a_box/ai_in_a_box_baseline_16gb_20240125.img.gz`

**Purpose:** Extract the working `panel-uctronics-lcd` kernel module
or driver source from the image. The module contains the correct init
sequence baked into its `.data` section. Comparing with our extracted
init will reveal what's wrong.

**Extraction plan:**
1. Mount the image's root partition on the X61s (loopback)
2. Find `panel-uctronics-lcd.ko` in `/lib/modules/`
3. Use `objdump -t` to find the `init_cmds` symbol
4. Use `objdump -s -j .rodata` to dump the init data
5. Parse the `{u8 reg, u8 val}` pairs
6. Compare with our `uctronics_lcd_init_cmds` in the forked driver
7. Fix any differences, rebuild, and test

**Alternative:** If the image includes kernel source (check `/usr/src/`),
the C source will have the init array directly readable.

## CONFIRMED: Panel IC is ILI9881C (2026-05-08, baseline image analysis)

**The "JD9365DA correction" was WRONG.** Disassembly of the working
baseline kernel confirms the panel uses ILI9881C page-switch commands.

### Evidence from baseline image disassembly

Downloaded `ai_in_a_box_baseline_16gb_20240125.img.gz` (2.4 GB) from
Useful Sensors. Mounted the image, extracted vmlinuz and System.map
for kernel `5.10.110-102-rockchip-g9e38c248f2d3`.

Disassembled `jadard_jd9365da_enable` at `0x6df990` on the Rock
(native aarch64 `objdump`). Key findings:

1. **The function calls `mipi_dsi_dcs_write` (at 0x6a1e54) 200 times**
2. **First call sends `{0xFF, 0x98, 0x81, 0x03}`** — ILI9881C page 3 select!
   - w1 = 0xFF (DCS command), data = {0x98, 0x81, 0x03}, len = 3
3. **Data table at 0xfa4170** contains 4 page payloads:
   `{98 81 03} {98 81 04} {98 81 01} {98 81 00}` — pages 3, 4, 1, 0
4. All register writes use `mipi_dsi_dcs_write(dsi, reg, &val, 1)`

The driver name `jadard_jd9365da_enable` is misleading — Uctronics
forked the JD9365DA driver framework but sends ILI9881C protocol.

### Correct init sequence structure

200 total DCS write calls:
- **Page 3** (GIP timing): 128 register writes (0x01-0x8A)
- **Page 4** (power/MIPI): 13 register writes
- **Page 1** (VCOM/power): 7 register writes
- **Page 0** (gamma/display): 45 register writes + sleep out + display on

Saved at: `hardware/uctronics-dsi/ili9881c-init-extracted.c`

### Comparison with previous extractions

| File | Source | Status |
|------|--------|--------|
| `ili9881c-init-sequence.c` | Binary pattern search in vmlinuz | **WRONG** — found ILI9881C data from a different driver in the same kernel |
| `jd9365da-init-sequence.c` | Disassembly attempt | **WRONG** — starts with `{0xFF, 0x11}`, incorrect IC assumption |
| `panel-init-sequence.txt` | DTS format extraction | **WRONG** — data from wrong offset in vmlinuz |
| `ili9881c-init-extracted.c` | Baseline image disassembly | **CORRECT** — traced every mipi_dsi_dcs_write call argument |

### Why previous attempts failed

1. The kernel has MULTIPLE panel drivers compiled in (ILI9881C for
   BananaPi, JD9365DA for Radxa 10HD, AND the uctronics panel)
2. Searching for `98 81` byte patterns found the BananaPi ILI9881C
   init data, not the uctronics data
3. The uctronics init is NOT in a contiguous data table — it's
   encoded as immediate values in the ARM64 instructions (mov/strb)
4. The "JD9365DA" name in the symbol table was misleading

### Next steps

1. **Fork `panel-ilitek-ili9881c.c`** from Armbian kernel source
2. **Add `uctronics_lcd_init[]`** from `ili9881c-init-extracted.c`
   as a new panel entry with `uctronics,uctronics-lcd` compatible
3. Add mode struct: 720x1280p60, 66 MHz, H:40/20/55, V:15/8/15
4. Build as out-of-tree kernel module on the Rock
5. Update DTS overlay compatible string
6. Test — this should produce visible pixels!
