# 07: Rock 5A OS Image — Flash for NPU Support

## Why reflash?

The current Radxa Ubuntu 22.04 Jammy image (built 2023-08-22) ships with
RKNPU kernel driver v0.8.2, which cannot run RKLLM (LLM on NPU). No
kernel upgrade is available via `apt`. Flashing a newer image with
vendor kernel 6.1.115 and RKNPU v0.9.8 is the cleanest path to NPU
LLM inference.

The previous Python AI in a Box application was also running on CPU
only — the old driver couldn't do proper NPU inference.

## Current vs target

| | Current | Target |
|---|---------|--------|
| **OS** | Ubuntu 22.04 Jammy | Ubuntu 24.04 Noble |
| **Kernel** | 5.10.110-102-rockchip | 6.1.115 (Armbian vendor) |
| **RKNPU driver** | v0.8.2 (builtin) | v0.9.8 (builtin) |
| **RKLLM support** | No | Yes |
| **Source** | Radxa rbuild (2023-08-22) | Armbian 26.2.1 |

---

## Image options (surveyed 2026-05-08)

### Armbian — RECOMMENDED

Actively maintained, Rock 5A officially supported, ships RKNPU v0.9.8
on vendor kernel images out of the box.

**Download:** <https://www.armbian.com/rock-5a/>

Choose **vendor kernel** (6.1.115) — NOT "current/edge" (6.18.x) which
uses the incompatible open-source Rocket driver.

| Image | Kernel | RKNPU | Size | Notes |
|-------|--------|-------|------|-------|
| **Ubuntu 24.04 Noble CLI minimal (vendor)** | 6.1.115 | **v0.9.8** | 269 MB | **Best for jhana-rs** — headless, lightweight |
| Ubuntu 24.04 Noble + GNOME (vendor) | 6.1.115 | v0.9.8 | 1.3 GB | Desktop |
| Ubuntu 24.04 Noble + KDE Neon (vendor) | 6.1.115 | v0.9.8 | 1.4 GB | Desktop |
| Ubuntu 24.04 Noble CLI (current) | 6.18.15 | Rocket | 296 MB | **DO NOT USE** — Rocket driver incompatible with RKLLM |
| Ubuntu 26.04 Resolute CLI (vendor) | 6.1.115 | v0.9.8 | 277 MB | Cutting edge Ubuntu |
| Debian 13 Trixie CLI (vendor) | 6.1.115 | v0.9.8 | ~270 MB | If Debian is preferred |

### Radxa official — Debian only

Radxa no longer ships Ubuntu images. Their current release is Debian
Bookworm with KDE desktop.

| Image | Kernel | RKNPU | Notes |
|-------|--------|-------|-------|
| Debian 12 Bookworm KDE (rsdk-b3) | 6.1.x | v0.9.6 (upgradable to 0.9.8 via `rsetup`) | Desktop only, Nov 2024 |

Download: <https://github.com/radxa-build/rock-5a/releases>

After flash: `sudo rsetup → System → System Update` to upgrade RKNPU
to v0.9.8.

### Joshua-Riek ubuntu-rockchip — ARCHIVED

**Project archived April 29, 2026 — no more updates or security patches.**

| Image | Kernel | RKNPU | Notes |
|-------|--------|-------|-------|
| Ubuntu 24.04 server (v2.4.0) | 6.1.x | ~0.9.6-0.9.8 | No future patches |
| Ubuntu 22.04 server (v2.4.0) | 5.10.x | v0.9.6 | Below RKLLM minimum |
| Ubuntu 24.10 (v2.4.0) | 6.11 | Partial | HDMI broken, avoid |

Download (still accessible): <https://joshua-riek.github.io/ubuntu-rockchip-download/boards/rock-5a.html>

Not recommended — dead project. Use Armbian instead.

---

## Recommended image

**Armbian 26.2.1, Ubuntu 24.04 Noble, vendor kernel 6.1.115, CLI minimal**

- 269 MB download, lightweight, headless (no desktop — jhana-rs is a TUI)
- RKNPU v0.9.8 included — RKLLM works out of the box
- Ubuntu 24.04 LTS — supported until 2029
- Actively maintained by Armbian community

---

## Rock 5A storage

The Rock 5A has both **eMMC (32 GB, primary)** and **microSD slot**.
Boot priority: eMMC → SD → NVMe. The current system runs from eMMC.

Strategy: **flash Armbian to SD card first, test, then flash to eMMC**
once confirmed working. This preserves the current eMMC as a fallback.

---

## Flash instructions

### Method 1: Test on SD card first (RECOMMENDED)

Low risk — keeps the current eMMC system intact as fallback.

**On the X61s:**

```bash
# Download image
wget https://dl.armbian.com/rock-5a/Armbian_26.2.1_Rock-5a_noble_vendor_6.1.115_minimal.img.xz

# Decompress
xz -d Armbian_26.2.1_Rock-5a_noble_vendor_6.1.115_minimal.img.xz

# Flash to SD card (replace /dev/sdX with actual device)
sudo dd if=Armbian_26.2.1_Rock-5a_noble_vendor_6.1.115_minimal.img \
  of=/dev/sdX bs=4M status=progress
sync
```

**On the Rock:**

1. Power off the Rock
2. Insert the SD card
3. To boot from SD instead of eMMC, either:
   - Hold the maskrom button while powering on, or
   - Clear the eMMC bootloader first (see Method 2)
4. The Rock should boot from SD into Armbian
5. Default login: `root` / (set on first boot)

**Verify NPU driver:**
```bash
cat /sys/kernel/debug/rknpu/version
# Expected: RKNPU driver: v0.9.8
dmesg | grep -i rknpu
```

### Method 2: Flash to eMMC via USB Maskrom

Direct eMMC flash without needing a working OS.

**Hardware setup:**
1. Remove SD card and power cable
2. Connect USB-A to USB-A cable from Rock's top USB 3.0 port to X61s
3. Short the **Maskrom pin pads** on the Rock 5A PCB with a DuPont wire
4. Plug in power — X61s should detect a Rockchip USB device

**On the X61s:**
```bash
# Install flashing tool
sudo apt install rkdeveloptool

# Verify device detected
sudo rkdeveloptool ld
# Should show: DevNo=1 Vid=0x2207 Pid=0x350b

# Download SPL loader from Radxa
wget https://dl.radxa.com/rock5/sw/images/loader/rk3588_spl_loader_v1.15.113.bin

# Load bootloader
sudo rkdeveloptool db rk3588_spl_loader_v1.15.113.bin

# Flash image to eMMC
sudo rkdeveloptool wl 0 Armbian_26.2.1_Rock-5a_noble_vendor_6.1.115_minimal.img

# Reboot
sudo rkdeveloptool rd
```

### Method 3: Flash eMMC from a running SD card system

If already booted from SD card (Method 1):

```bash
# Identify eMMC device
lsblk
# eMMC is typically /dev/mmcblk0 when booted from SD

# Flash (DESTRUCTIVE — overwrites all eMMC data)
xz -dc /path/to/Armbian.img.xz | sudo dd of=/dev/mmcblk0 bs=4M status=progress
sync
```

---

## After flashing

### 1. First boot setup

Armbian prompts for root password and user creation on first boot.

```bash
# Create ubuntu user (to match existing scripts)
adduser ubuntu
usermod -aG sudo,render,video ubuntu
```

### 2. Verify NPU driver

```bash
cat /sys/kernel/debug/rknpu/version
# Expected: RKNPU driver: v0.9.8

dmesg | grep -i rknpu
# Should show: RKNPU fdab0000.npu: RKNPU: rknpu iommu is enabled
```

### 3. Install NPU runtime libraries

```bash
# librknnrt.so v2.2.0 (for STT via sensevoice-rs)
wget -O /tmp/librknnrt.so \
  "https://github.com/airockchip/rknn-toolkit2/raw/v2.2.0/rknpu2/runtime/Linux/librknn_api/aarch64/librknnrt.so"
sudo cp /tmp/librknnrt.so /usr/lib/librknnrt.so

# RKNN headers
for h in rknn_api.h rknn_matmul_api.h rknn_custom_op.h; do
  wget -O /tmp/$h \
    "https://github.com/airockchip/rknn-toolkit2/raw/v2.2.0/rknpu2/runtime/Linux/librknn_api/include/$h"
done
sudo cp /tmp/rknn_*.h /usr/include/

# librkllmrt.so v1.2.3 (for LLM via rkllm-rs)
wget -O /tmp/librkllmrt.so \
  "https://raw.githubusercontent.com/airockchip/rknn-llm/release-v1.2.3/rkllm-runtime/Linux/librkllm_api/aarch64/librkllmrt.so"
sudo cp /tmp/librkllmrt.so /usr/lib/librkllmrt.so

sudo ldconfig
```

### 4. Install build dependencies

```bash
sudo apt update
sudo apt install build-essential cmake pkg-config libasound2-dev \
  rsync console-setup libclang-dev protobuf-compiler ffmpeg
```

### 5. Install Rust toolchain

```bash
curl --proto '=https' --tlsv1.2 -sSf https://sh.rustup.rs | sh
source ~/.cargo/env

# Set FP16 flag permanently
mkdir -p ~/.cargo
cat >> ~/.cargo/config.toml << 'EOF'
[target.aarch64-unknown-linux-gnu]
rustflags = ["-C", "target-feature=+fp16"]
EOF
```

### 6. Install Piper TTS

```bash
# Download Piper binary for aarch64
wget https://github.com/rhasspy/piper/releases/download/2023.11.14-2/piper_linux_aarch64.tar.gz
tar xzf piper_linux_aarch64.tar.gz
sudo cp piper/piper /usr/local/bin/
```

### 7. Restore models and data

```bash
# Copy models from backup or re-download
mkdir -p ~/models

# Piper TTS model
# (copy from backup or download from sherpa-onnx releases)

# SenseVoice RKNN model
# (copy from backup or re-download from HuggingFace)

# Llama-3.2-3B RKLLM model (already on old eMMC)
# (copy or re-download)

# Sync jhana-rs source
# (from X61s via scripts/rock-sync.sh)
```

### 8. Configure console

```bash
# Large font for the 720x1280 display
sudo setfont /usr/share/consolefonts/Uni3-TerminusBold32x16.psf.gz

# Persist font
sudo sed -i 's/FONTFACE=.*/FONTFACE="TerminusBold"/' /etc/default/console-setup
sudo sed -i 's/FONTSIZE=.*/FONTSIZE="32x16"/' /etc/default/console-setup
sudo setupcon

# Suppress DMA console messages
sudo dmesg -n 1
```

---

## Backup before flashing

**Back up everything from the current eMMC before flashing:**

```bash
# From X61s, backup via SSH:
sshpass -p 'ubunturock' ssh ubuntu@192.168.1.102 \
  "tar czf /tmp/home-backup.tar.gz -C /home ubuntu"

sshpass -p 'ubunturock' scp ubuntu@192.168.1.102:/tmp/home-backup.tar.gz .

# Or backup specific directories:
sshpass -p 'ubunturock' rsync -avz \
  ubuntu@192.168.1.102:~/models/ ./backup-models/

sshpass -p 'ubunturock' rsync -avz \
  ubuntu@192.168.1.102:~/jhana-rs/ ./backup-jhana-rs/
```

Models to preserve:
- `/home/ubuntu/models/` (~6 GB total)
- `/home/ubuntu/jhana-rs/` (synced from X61s, can be re-synced)
- `/home/ubuntu/.cargo/` (Rust toolchain, can be reinstalled)

---

## References

- [Armbian Rock 5A](https://www.armbian.com/rock-5a/)
- [Armbian downloads mirror](https://dl.armbian.com/rock-5a/)
- [Radxa Rock 5A install docs](https://docs.radxa.com/en/rock5/rock5a/getting-started/install-os)
- [Radxa maskrom flashing (Linux)](https://docs.radxa.com/en/rock5/rock5a/low-level-dev/maskrom/linux)
- [radxa-build/rock-5a releases](https://github.com/radxa-build/rock-5a/releases)
- [Joshua-Riek ubuntu-rockchip (archived)](https://github.com/Joshua-Riek/ubuntu-rockchip)
- [Pelochus/ezrknpu](https://github.com/Pelochus/ezrknpu) — RKNPU + RKLLM installer
- [airockchip/rknn-llm](https://github.com/airockchip/rknn-llm) — RKLLM runtime + driver source
