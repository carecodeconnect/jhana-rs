# 06: RKNPU Kernel Driver Upgrade

## Current state (2026-05-08)

| Component | Version | Status |
|-----------|---------|--------|
| Kernel | 5.10.110-102-rockchip | Radxa vendor BSP (2022) |
| RKNPU driver | v0.8.2 (builtin) | **Too old for RKLLM** |
| librknnrt.so | v2.2.0 | OK for STT/VAD (RKNN) |
| librkllmrt.so | v1.2.3 | Installed, but driver too old |

## Why the driver must be upgraded

The RKNPU kernel driver v0.8.2 (baked into kernel 5.10.110) cannot run
RKLLM inference. Tested 2026-05-08:

- **3B model** (Llama-3.2-3B, 4.35 GB): `failed to malloc npu memory`
  — cannot allocate 3.2 GB contiguous NPU buffer
- **270M model** (Gemma-3-270M, 629 MB): loads successfully (1.24s),
  runs at 66 tok/s, but `matmul(w8a8) run failed` on every operation.
  Output is garbage (wrong computation results). NPU hardware is
  functional — 66 tok/s proves cores are computing — but the old
  driver's matmul implementation is incompatible with w8a8 quantization.

**Minimum required:** RKNPU driver v0.9.7+
**Latest available:** RKNPU driver v0.9.8 (2024-10-09)

## Driver version compatibility

| RKNPU driver | Kernel | RKLLM support | Source |
|-------------|--------|---------------|--------|
| v0.8.2 | 5.10.110 | **No** — matmul w8a8 fails | Current (builtin) |
| v0.9.6 | 5.10.x / 6.1.x | Partial — below RKLLM minimum | Joshua-Riek v2.3.0 |
| **v0.9.7** | 5.10.x / 6.1.x | **Minimum for RKLLM** | airockchip |
| **v0.9.8** | 5.10.198+ / 6.1.57+ | **Recommended** | Armbian vendor, Radxa rsetup |
| Rocket (mainline) | 6.18+ | **Incompatible** — different ABI | Collabora/DRM accel |

**Important:** The mainline "Rocket" driver (merged Linux 6.18, July 2025)
is a completely different open-source driver by Collabora. It uses the DRM
accel subsystem (`/dev/accel/accel0`) and is **NOT compatible** with
Rockchip's RKLLM, RKNN, or any proprietary NPU userspace tools. Do not
use kernel 6.18+ "current/edge" images for RKLLM work.

## Upgrade options

### Option A: Flash Armbian with vendor kernel 6.1.115 (RECOMMENDED)

See `docs/07_IMAGE.md` for full details.

Armbian 26.2.1 ships Ubuntu 24.04 Noble with vendor kernel 6.1.115 and
**RKNPU v0.9.8 included out of the box**. This is the cleanest path.

### Option B: In-place RKNPU driver upgrade (no reflash)

Attempt to compile RKNPU v0.9.8 as an out-of-tree kernel module against
the current 5.10.110 kernel. Lowest risk but unconfirmed on this kernel.

The current driver is **builtin** (not a loadable module), so `modprobe`
won't work. Options:

1. **Try `rsetup`** — if installed, run `sudo rsetup → System → System
   Update`. This may pull RKNPU v0.9.8 as a package update.

2. **Compile out-of-tree module:**
   ```bash
   # Download driver source
   wget https://github.com/airockchip/rknn-llm/raw/main/rknpu-driver/rknpu_driver_0.9.8_20241009.tar.bz2
   tar xf rknpu_driver_0.9.8_20241009.tar.bz2

   # Install kernel headers
   sudo apt install linux-headers-$(uname -r)

   # Build module
   cd rknpu-driver
   make -C /lib/modules/$(uname -r)/build M=$(pwd)/drivers/rknpu modules

   # Load (overrides builtin)
   sudo insmod drivers/rknpu/rknpu.ko

   # Verify
   cat /sys/kernel/debug/rknpu/version
   ```

   **Caveats:**
   - Driver source targets 5.10.198; our 5.10.110 may need patches
   - Kernel headers for this custom Radxa kernel may not be available
   - If it fails to compile, fall back to Option A (reflash)

3. **DKMS approach** — [bmilde/rknpu-driver-dkms](https://github.com/bmilde/rknpu-driver-dkms)
   attempts to package as DKMS module, but was WIP as of last check.

### Option C: Rebuild kernel from source

Build the Radxa 5.10 kernel with updated RKNPU driver source.

```bash
# On the Rock (or cross-compile on x86_64):
git clone https://github.com/radxa/kernel -b linux-5.10-gen-rkr4.1
cd kernel

# Replace rknpu driver
rm -rf drivers/rknpu
cp -r /path/to/rknpu_driver_0.9.8/drivers/rknpu drivers/

# Configure and build
make ARCH=arm64 rockchip_linux_defconfig
make ARCH=arm64 -j4
sudo make ARCH=arm64 modules_install
sudo make ARCH=arm64 install
```

**Risk:** High — may break boot if device tree or bootloader is
incompatible. Keep a backup boot medium.

### Option D: Stay on CPU LLM

Keep mistral.rs at 3.89 tok/s. No driver change needed. NPU is used
only for STT (sensevoice-rs + librknnrt.so, which works on v0.8.2).

## Recommended path

1. **First:** Try Option B (in-place driver module) — 30 minutes, no data loss
2. **If B fails:** Option A (flash Armbian) — see `docs/07_IMAGE.md`
3. **Fallback:** Option D (stay on CPU) while planning the reflash

## References

- [airockchip/rknn-llm rknpu-driver](https://github.com/airockchip/rknn-llm/tree/main/rknpu-driver) — v0.9.8 source
- [airockchip/rknn-toolkit2](https://github.com/airockchip/rknn-toolkit2) — RKNN SDK
- [Pelochus/ezrknpu](https://github.com/Pelochus/ezrknpu) — RKNPU + RKLLM installer script
- [Pelochus/armbian-build-rknpu-updates](https://github.com/Pelochus/armbian-build-rknpu-updates) — Armbian + RKNPU 0.9.8
- [bmilde/rknpu-driver-dkms](https://github.com/bmilde/rknpu-driver-dkms) — DKMS approach (WIP)
- [Armbian Forum: NPU and RKLLM on RK3588](https://forum.armbian.com/topic/56993-npu-and-rkllm-support-on-rockchip-rk3588-nanopc-t6-and-rk3576-nanopi-m5/)
- [Armbian Forum: Best way to get RKNPU 0.9.6 update](https://forum.armbian.com/topic/44174-what-is-the-best-way-to-get-rknpu-v096-update/)
- [Tomeu Vizoso: Rocket driver mainlined in Linux 6.18](https://blog.tomeuvizoso.net/2025/07/rockchip-npu-update-6-we-are-in-mainline.html)
- [Collabora: RK3588 upstream support](https://www.collabora.com/news-and-blog/news-and-events/rockchip-rk3588-upstream-support-progress-future-plans.html)
