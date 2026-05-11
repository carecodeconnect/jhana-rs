# 01: Development Setup

## Create config.json

All `scripts/rock-*.sh` scripts read Rock connection details from
`config.json` in the repo root. This file is gitignored (contains
credentials). Copy the template and fill in your values:

```bash
cp config.json.example config.json
# Edit config.json with your Rock's IP and credentials
```

## SSH into the Rock 5A

### Option A: Router ethernet (current setup)

Both the X61s and Rock 5A are plugged into the router. The Rock gets
an IP via DHCP. No manual network setup needed.

```bash
scripts/rock-ssh.sh
```

The Rock IP is configured in `config.json` (single source of truth).
All `scripts/rock-*.sh` scripts read from it automatically.

### Option B: Tailscale (works from anywhere)

Both the X61s and Rock are on the same Tailscale network. This works
from any internet connection — no router or LAN needed.

```bash
ROCK_IP=rock-5a scripts/rock-ssh.sh
```

Or directly via Tailscale hostname (no password needed):
```bash
ssh root@rock-5a
```

Tailscale IPs and hostnames are in `config.json`.

Tailscale was set up on the Rock on 2026-05-11 with `tailscale up --ssh`.
The `--ssh` flag enables Tailscale SSH (no password/key needed between
Tailscale devices). To re-authenticate if expired:
```bash
# On the Rock (via local SSH or console):
sudo tailscale up --ssh
# Opens a URL to approve in browser
```

### Option C: Direct ethernet (X61s to Rock, no router)

When no router is available, connect the Rock directly to the X61s via
ethernet. Requires manual DHCP setup on the laptop.

Prerequisites:
```bash
sudo apt install dnsmasq sshpass
```

1. Assign a static IP to the laptop ethernet interface:
   ```bash
   sudo ip addr add 192.168.1.1/24 dev enp0s25
   ```

2. Start a DHCP server (runs in foreground -- use a separate terminal):
   ```bash
   sudo dnsmasq --no-daemon --interface=enp0s25 \
     --dhcp-range=192.168.1.100,192.168.1.200,12h --bind-interfaces \
     --port=0
   ```
   `--port=0` disables the DNS server (only DHCP is needed). Without it,
   dnsmasq fails because systemd-resolved already holds port 53.

3. Power on (or reboot) the Rock 5A. Wait ~2 minutes for boot.

4. Verify the DHCP lease:
   ```bash
   cat /var/lib/misc/dnsmasq.leases
   # Should show: rock-5a at some IP
   ```

5. Update `config.json` with the assigned IP, then SSH in:
   ```bash
   scripts/rock-ssh.sh
   ```

---

## Stop the Captioning Service

The AI in a Box Python captioning service (`run-chatty-startup.service`) starts
on boot and takes over the display via pygame/SDL2 on DRM/KMS. It must be
stopped to free the display and reclaim RAM (~6.8 GB).

```bash
# Stop the service (requires sudo -- will prompt for password)
sudo systemctl stop run-chatty-startup.service

# Disable it so it doesn't restart on reboot
sudo systemctl disable run-chatty-startup.service
```

The sudo password is the same as the user password in `config.json`.

Note: on the old image the `ubuntu` user did not have passwordless sudo.
On the fresh Armbian image (2026-05-11), passwordless sudo is configured.

### Restoring the service later

The Python code and models are in `/home/ubuntu/ai_in_a_box/` on the device.
The code can also be restored from the host repo at
`~/projects/ai_in_a_box/` via scp. The large model files in `downloaded/`
(~3.1 GB) are only on the device (not in git).

```bash
# Re-enable and start
sudo systemctl enable run-chatty-startup.service
sudo systemctl start run-chatty-startup.service
```

---

## Getting a CLI on the Rock's Display

### How the display works currently

- The Rock 5A has a 720x1280 portrait display connected via HDMI/DSI.
- pygame/SDL2 renders directly to DRM/KMS (no X11 or Wayland server).
- `getty@tty1.service` (Linux virtual console) runs on tty1 but pygame
  takes over the framebuffer, hiding the console.
- Kernel cmdline has `consoleblank=0` (console never blanks) and
  `console=tty0` (kernel messages go to the display).

### After stopping the service

Once `run-chatty-startup.service` is stopped, pygame releases the DRM/KMS
display. The getty on tty1 should become visible on the physical screen,
showing a login prompt. You can log in directly on the device
(credentials in `config.json`) or continue via SSH.

If the console doesn't appear after stopping the service, switch to tty1:
```bash
sudo chvt 1
```

### Console font size

The pygame captioning service used 70px Noto font. Console fonts max out at
32px height (Terminus). On the 720x1280 display, TerminusBold 32x16 gives
45 columns x 40 rows -- the closest match available.

Install console-setup (required -- not present on stock image):
```bash
sudo apt install console-setup
```

Set font (one-time):
```bash
sudo setfont /usr/share/consolefonts/Uni3-TerminusBold32x16.psf.gz
```

Persist across reboots -- edit `/etc/default/console-setup`:
```
CODESET="Uni3"
FONTFACE="TerminusBold"
FONTSIZE="32x16"
```

Then apply:
```bash
sudo setupcon
```

Configured 2026-05-07. Font is Uni3-TerminusBold32x16 (Unicode, bold,
32px height, 16px width).

### Suppress kernel console messages

The RK3588S DMA controller logs `fill_queue` errors to the console (tty1),
which overwrite the ratatui TUI. These are harmless hardware messages --
suppressing console output does not disable logging. Messages still go to
`dmesg` and `/var/log/kern.log`.

```bash
# Suppress (only KERN_EMERG printed to console)
sudo dmesg -n 1

# Restore default (for debugging kernel issues)
sudo dmesg -n 7
```

This is non-persistent -- resets on reboot. To make permanent, add
`loglevel=1` to the kernel cmdline in `/boot/extlinux/extlinux.conf`.

### ratatui on the Rock display

The ratatui TUI runs on tty1 (the Rock's physical display). SSH is used
for development and can also run the TUI, but the primary target is the
physical screen visible on the device.

Run the TUI on the physical display (via SSH):
```bash
sudo TERM=linux setsid ./target/debug/jhana-rs </dev/tty1 >/dev/tty1 2>/dev/tty1 &
```

Logs go to `jhana-rs.log` in the working directory. Quit via `q` key on
the physical keyboard, or `sudo kill <pid>` / `SIGTERM` from SSH.

---

## Development Workflow

### Why build on the Rock

The X61s (Core 2 Duo L7500) is too slow for Rust compilation and is x86_64
while the Rock 5A is aarch64. Cross-compiling with C/C++ dependencies
(llama.cpp, whisper.cpp) is complex and fragile. **All builds happen on the
Rock.** The X61s is only used for editing, linting, and `cargo check`.

The Rock 5A's Cortex-A76 cores handle debug builds well. Release builds are
slower but acceptable.

### What runs where

| Machine | Arch | Role | Commands |
|---------|------|------|----------|
| X61s | x86_64 | Edit, lint, check | `cargo fmt`, `cargo clippy`, `cargo check` |
| Rock 5A | aarch64 | **Build, run, test, doc** | `cargo build`, `cargo run`, `cargo test`, `cargo doc` |

The Cargo.toml is the same on both machines. The X61s can run `cargo check`
and clippy (they verify code without linking) but **never `cargo build`** --
it's too slow and the wrong architecture.

### Workflow: edit on X61s, build on Rock

1. **Edit** code on the X61s in `~/projects/jhana-rs/`.

2. **Check locally** (fast feedback on x86_64 -- no cross-compile needed):
   ```bash
   cargo check
   cargo clippy
   ```
   Run `cargo check` after every code change, before syncing to Rock.
   Pre-commit hooks run `rustfmt` and `clippy` automatically on commit.

3. **Sync** to the Rock via rsync (installed 2026-05-07 via `sudo apt install rsync`):
   ```bash
   scripts/rock-sync.sh
   ```

4. **Build and run** on the Rock via SSH:
   ```bash
   scripts/rock-build.sh   # cargo check + build + test
   scripts/rock-run.sh     # launch TUI on display
   ```

### Alternative: edit directly on Rock via SSH

Use your preferred terminal editor (vim, nano, helix) over SSH. This avoids
the sync step but is less comfortable for large edits.

### Give the Rock internet access

With router ethernet (current setup), the Rock already has internet access
via DHCP. No extra steps needed. Download speed ~12 MB/s.

#### Fallback: NAT forwarding via X61s (direct ethernet, no router)

When the Rock is connected directly to the X61s via ethernet (no router),
forward the X61s wifi connection. Download speed ~1 MB/s.

On the X61s (one-time, until reboot):
```bash
sudo ip addr add 192.168.1.1/24 dev enp0s25
# In a separate terminal:
sudo dnsmasq --no-daemon --interface=enp0s25 \
  --dhcp-range=192.168.1.100,192.168.1.200,12h --bind-interfaces \
  --port=0

sudo sysctl -w net.ipv4.ip_forward=1
sudo iptables -t nat -A POSTROUTING -o wlan0 -j MASQUERADE
sudo iptables -A FORWARD -i enp0s25 -o wlan0 -j ACCEPT
sudo iptables -A FORWARD -i wlan0 -o enp0s25 -m state --state RELATED,ESTABLISHED -j ACCEPT
```

On the Rock (via SSH):
```bash
sudo ip route add default via 192.168.1.1
echo 'nameserver 8.8.8.8' | sudo tee /etc/resolv.conf
```

### Alternative: git-based workflow

With internet access enabled above, you can push from the X61s and pull on
the Rock:

```bash
git clone https://github.com/carecodeconnect/jhana-rs.git
cd jhana-rs && git pull
```

---

## Pre-commit Hooks

The repo includes a pre-commit hook that runs `rustfmt` and `clippy` on
staged `.rs` files. Commits are blocked if either check fails.

Install (on X61s -- runs against the local x86_64 toolchain):
```bash
cp scripts/pre-commit .git/hooks/pre-commit
```

The hook runs:
1. `cargo fmt -- --check` (fails if formatting is wrong)
2. `cargo clippy -- -D warnings` (fails on any warning)

To fix formatting before committing: `cargo fmt`

---

## Install Rust Toolchain

### X61s (dev machine)

Rust is pre-installed. Used for `cargo check`, `cargo clippy`, `cargo fmt`,
and pre-commit hooks only (no cross-compilation).

```
rustc 1.94.1 (x86_64)
```

### Rock 5A (build target)

Requires internet access (see NAT forwarding above) and expanded disk space
(see below).

```bash
curl --proto '=https' --tlsv1.2 -sSf https://sh.rustup.rs | sh
source ~/.cargo/env
```

Build dependencies (already installed on the device):
```bash
sudo apt install build-essential cmake pkg-config libasound2-dev
```

Installed 2026-05-07:
```
rustc 1.95.0 (aarch64)
cargo 1.95.0
clippy 0.1.95
rustfmt 1.9.0
```

---

## Expand Disk Space

### Pre-resize state (2026-05-07)

```
Partition Table: gpt (31.9 GB eMMC)
  1  config   16.8 MB  fat32
  2  boot    315   MB  fat32
  3  rootfs   10.9 GB  ext4   (8.6 GB used, 917 MB free, 91% full)
     free     20.7 GB          unallocated at end of disk

Filesystem state: clean (no errors, not dirty)
```

This is safe to resize online because:
- No partitions exist after partition 3 -- the 20.7 GB is contiguous free
  space at the end of the disk.
- ext4 supports online (mounted) growth via `resize2fs`. Only metadata is
  updated; no data moves.
- `resizepart` only shifts the partition end boundary forward.

### Expand the root partition

```bash
# Grow partition 3 to fill the disk (moves end boundary only)
# parted will warn "Partition is being used" -- answer Yes
sudo parted /dev/mmcblk1 resizepart 3 100%

# Grow the ext4 filesystem to match (online, no unmount needed)
sudo resize2fs /dev/mmcblk1p3

# Verify
df -h /
```

### Post-resize state (2026-05-07)

```
/dev/mmcblk1p3   29G  8.6G   20G  31% /
```

Resized online while mounted. No reboot required. All data intact.

### Delete unused models

The NLLB translation model (~1.2 GB) in
`/home/ubuntu/ai_in_a_box/downloaded/nllb-200-distilled-600M/` is not needed
for jhana-rs and can be deleted to free additional space.

```bash
rm -rf /home/ubuntu/ai_in_a_box/downloaded/nllb-200-distilled-600M/
df -h /
```
