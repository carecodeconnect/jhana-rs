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

## Rock 5A storage (verified 2026-05-08)

**The Rock 5A has NO eMMC installed.** The OS boots from a 32 GB microSD
card (`SD32G` on `mmc1`). The eMMC socket (`mmc0`) is unpopulated.
See `docs/00_HARDWARE.md` for evidence.

The microSD card is inside the Uctronics AI in a Box enclosure. The
case must be opened to access it.

---

## Flash instructions

### Step-by-step: flash Armbian to a new microSD card

**What you need:**
- A spare microSD card (8 GB+, ideally 32 GB)
- A microSD-to-USB card reader for the X61s
- A small screwdriver to open the Uctronics enclosure

**On the X61s (ThinkPad, Ubuntu):**

```bash
# 1. Download Armbian image
wget https://dl.armbian.com/rock-5a/Armbian_26.2.1_Rock-5a_noble_vendor_6.1.115_minimal.img.xz

# 2. Decompress (takes ~30s, produces ~1.5 GB .img file)
xz -d Armbian_26.2.1_Rock-5a_noble_vendor_6.1.115_minimal.img.xz

# 3. Insert the NEW (spare) microSD card into USB reader on X61s

# 4. Find the device name
lsblk
# Look for the new disk (e.g. /dev/sdb — NOT /dev/sda which is your laptop!)

# 5. Flash the image (REPLACE /dev/sdX with actual device!)
sudo dd if=Armbian_26.2.1_Rock-5a_noble_vendor_6.1.115_minimal.img \
  of=/dev/sdX bs=4M status=progress
sync

# 6. Eject the card
sudo eject /dev/sdX
```

**Swap the cards on the Rock:**

1. Power off the Rock (unplug power cable)
2. Open the Uctronics enclosure
3. Remove the current 32 GB microSD card — **label it "BACKUP"**
4. Insert the new Armbian microSD card
5. Close the enclosure (loosely — you may need to swap back)
6. Plug in power — Rock boots from the new card

**First boot (via SSH from X61s):**

The Rock should get the same IP (192.168.1.102) via DHCP, but Armbian's
default SSH may differ. Try:

```bash
# Armbian default: root with password set on first boot
# May need to connect a keyboard to the Rock's USB for first-time setup
# Or try: ssh root@192.168.1.102 (default password: 1234)
```

If SSH doesn't work, connect a USB keyboard to the Rock and log in on
the DSI display. Armbian prompts for root password on first boot.

**Verify NPU driver:**
```bash
cat /sys/kernel/debug/rknpu/version
# Expected: RKNPU driver: v0.9.8
dmesg | grep -i rknpu
```

**If it doesn't work:** Power off, swap the BACKUP microSD back in,
power on — you're back to the original system. Zero risk.

### After verification: set up the new system

If RKNPU v0.9.8 is confirmed, keep the new card and set up the system.
If not, swap the backup card back and investigate.

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
