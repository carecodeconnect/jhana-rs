# 00: Hardware — Radxa Rock 5A (Uctronics AI in a Box)

Evidence gathered directly from the running device on 2026-05-08.

## Board

| Field | Value | Source |
|-------|-------|--------|
| Model | Radxa ROCK 5A | `/proc/device-tree/model` |
| SoC | RK3588S | `/proc/device-tree/compatible` = `radxa,rock-5a rockchip,rk3588` |
| Enclosure | Uctronics AI in a Box | Physical inspection |

## CPU

| Field | Value | Source |
|-------|-------|--------|
| Architecture | aarch64 | `lscpu` |
| CPUs | 8 (4x Cortex-A76 + 4x Cortex-A55) | `lscpu` (reports A55, A76 via `big.LITTLE`) |
| Max frequency | 1800 MHz (A55), 2400 MHz (A76) | `lscpu` |
| FP16 support | Yes (`fphp` + `asimdhp`) | `/proc/cpuinfo Features` |

Note: `lscpu` only shows Cortex-A55 because it reports the boot CPU.
The A76 cores (4-7) are present and used for LLM inference.

## Memory

| Field | Value | Source |
|-------|-------|--------|
| Total RAM | 7.8 GiB (8,133,944 kB) | `/proc/meminfo MemTotal` |
| Swap | 3.9 GiB (zram) | `free -h` |

## Storage

**The Rock 5A has NO eMMC installed.** The OS boots from a microSD card.

| Slot | Controller | Device | Card | Size | Status |
|------|-----------|--------|------|------|--------|
| eMMC (mmc0) | `sdhci-dwcmshc fe2e0000.mmc` | — | — | — | **Empty — no eMMC module installed** |
| microSD (mmc1) | `dwmmc_rockchip fe2c0000.mmc` | `/dev/mmcblk1` | SD32G | 29.7 GiB | **Boot device** — Ubuntu 22.04 |

Evidence:
- `ls /dev/mmcblk0*` → `No such file or directory` (no eMMC)
- `cat /sys/class/mmc_host/mmc0/mmc0:*/type` → no card found
- `cat /sys/class/mmc_host/mmc1/mmc1:*/type` → `SD`
- `cat /sys/class/mmc_host/mmc1/mmc1:*/name` → `SD32G`
- `dmesg` → `mmc1: new ultra high speed SDR104 SDHC card at address aaaa`
- `dmesg` → `mmcblk1: mmc1:aaaa SD32G 29.7 GiB`
- `mmc0: SDHCI controller on fe2e0000.mmc` appears in dmesg but no card
  is enumerated — the eMMC socket exists but is unpopulated

The microSD card is inside the Uctronics enclosure. The case must be
opened to access it.

### Partition layout (microSD, mmcblk1)

| Partition | Size | Mount | Filesystem | Purpose |
|-----------|------|-------|------------|---------|
| mmcblk1p1 | 16 MB | /config | vfat | U-Boot config |
| mmcblk1p2 | 300 MB | (unmounted) | — | Boot partition |
| mmcblk1p3 | 29.4 GB | / | ext4 | Root filesystem |

## NPU

| Field | Value | Source |
|-------|-------|--------|
| NPU | 6 TOPS (INT8), 3 cores | Device tree `rockchip,rk3588-rknpu` at `fdab0000.npu` |
| Kernel driver | RKNPU v0.8.2 (builtin) | `dmesg` |
| IOMMU | Enabled | `dmesg`: `rknpu iommu is enabled, using iommu mode` |
| Status | Functional but driver too old for RKLLM | See `docs/06_KERNEL.md` |

## GPU

| Field | Value | Source |
|-------|-------|--------|
| GPU | Mali-G610 MP4 | `dmesg`: `mali fb000000.gpu` |
| Driver | Kernel DDK g13p0-01eac0 | `dmesg` |
| Vulkan | Supported (for wgpu/rwkv-tts-rs) | Mali-G610 supports Vulkan 1.2 |

## Display

| Field | Value | Source |
|-------|-------|--------|
| Primary display | DSI (Uctronics panel) | `card0-DSI-1: connected` |
| Resolution | 720x1280 (portrait) | `/sys/class/drm/card0-DSI-1/modes` |
| HDMI | Disconnected | `card0-HDMI-A-1: disconnected` |
| DP | Disconnected | `card0-DP-1: disconnected` |

The Uctronics AI in a Box has a 720x1280 portrait DSI display built into
the enclosure. No HDMI monitor is connected.

### Uctronics DSI panel driver

The display uses a **custom kernel panel driver** (`uctronics,uctronics-lcd`)
that is only present in the original Radxa Ubuntu 22.04 image. It is NOT
available in Armbian or mainline kernels.

| Field | Value |
|-------|-------|
| Panel compatible | `uctronics,uctronics-lcd` |
| Kernel config | `CONFIG_DRM_PANEL_UCTRONICS_LCD=y` (builtin) |
| DSI controller | `dsi@fde20000` (MIPI DSI2, rockchip,rk3588-mipi-dsi2) |
| DSI lane rate | 480 Mbps (`rockchip,lane-rate = <0x1e0>`) |
| Reset GPIO | GPIO3_C1 (`<0xe0 0x11 0x00>`) |
| Backlight enable GPIO | GPIO3_D2 (`<0xe0 0x1a 0x00>`) |
| Backlight | PWM-based, 256 levels |
| VOP port | vp3 (MIPI0) |

The full device tree from the working Radxa image is saved at:
- `hardware/uctronics-dsi/radxa-ubuntu-22.04-full.dts` — complete DTS (10,390 lines)
- `hardware/uctronics-dsi/dsi-panel-node.dts` — extracted DSI/backlight/power nodes

**Armbian 26.2.1 does not work with this display** — the Armbian vendor
kernel 6.1.115 does not include `CONFIG_DRM_PANEL_UCTRONICS_LCD`. The
`rock-5a-radxa-display-8hd` overlay uses a different panel driver
(`radxa,display-8hd`, 800x1280) which is not compatible. The backlight
powers on but the panel shows nothing.

To fix: build the Uctronics panel driver as a kernel module from Radxa's
kernel source, or create a custom overlay with `panel-dsi` generic driver
using the timing data from the saved device tree. See TODO.

## Audio

4 ALSA devices (no Bluetooth):

| Card | Name | Type | Status |
|------|------|------|--------|
| 0 | rockchip-hdmi1 | HDMI SPDIF | Not connected |
| 1 | rockchip-es8316 | 3.5mm jack | Working (headphone/line out) |
| 2 | uctronics-codec | Onboard speaker + mic | Working (AI in a Box hardware) |
| 3 | rockchip-hdmi0 | HDMI I2S | Not connected |

Playback: `aplay -D plughw:2,0` (onboard speaker) or `plughw:1,0` (3.5mm)
Capture: `arecord -D plughw:2,0` (onboard mic)
Volume: `amixer -c 2 sset DAC N` (0-4, 1=25% indoor, 3=75% default)

## USB

| Bus | Device | Description |
|-----|--------|-------------|
| 001 | 002 | Terminus Technology Hub (USB 2.0) |

No external USB devices attached. The Terminus hub is the Uctronics
enclosure's built-in USB hub.

## Network

| Interface | Status |
|-----------|--------|
| eth0 | UP — connected to router via DHCP |
| (no wifi) | Rock 5A does not have onboard WiFi |

Access: `scripts/rock-ssh.sh` (IP and credentials in `config.json`)

## OS / Image

| Field | Value | Source |
|-------|-------|--------|
| OS | Ubuntu 22.04.5 LTS (Jammy Jellyfish) | `/etc/os-release` |
| Kernel | 5.10.110-102-rockchip | `uname -r` |
| Image build date | 2023-08-22 | `/etc/radxa_image_fingerprint` |
| Build command | `./rbuild --native-build --shrink rock-5a jammy cli` | `/etc/radxa_image_fingerprint` |
| Original kernel | linux-image-5.10.110-12-rockchip | `/etc/radxa_image_fingerprint` |
| U-Boot | 2017.09-1-77a5f37 | `/etc/radxa_image_fingerprint` |

## Flashing implications

Since the OS runs from the **microSD card** (not eMMC):

1. The microSD is the only boot device — there is no eMMC fallback
2. To flash a new image, the current microSD must be removed from the
   Uctronics enclosure (requires opening the case)
3. Flash the new image to a **different microSD** on the X61s, then swap
4. Keep the old microSD as a backup — can always swap it back
5. No USB maskrom flashing is needed (that's for eMMC)

See `docs/07_IMAGE.md` for flash instructions.
