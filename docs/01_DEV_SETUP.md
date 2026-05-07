# 01: Development Setup

## SSH into the Rock 5A

The LAN port on the AI in a Box connects directly to the laptop (ThinkPad
X61s) via ethernet. This is not a router connection -- it requires manual
network setup on the laptop.

### Prerequisites (on the X61s)

```bash
sudo apt install dnsmasq sshpass
```

### Connect

1. Assign a static IP to the laptop ethernet interface:
   ```bash
   sudo ip addr add 192.168.1.1/24 dev enp0s25
   ```

2. Start a DHCP server (runs in foreground -- use a separate terminal):
   ```bash
   sudo dnsmasq --no-daemon --interface=enp0s25 \
     --dhcp-range=192.168.1.100,192.168.1.200,12h --bind-interfaces
   ```

3. Power on (or reboot) the Rock 5A. Wait ~2 minutes for boot.

4. Verify the DHCP lease:
   ```bash
   cat /var/lib/misc/dnsmasq.leases
   # Should show: rock-5a at 192.168.1.102
   ```

5. SSH in:
   ```bash
   sshpass -p 'ubunturock' ssh ubuntu@192.168.1.102
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

Password for sudo: `ubunturock`

Note: the `ubuntu` user does not have passwordless sudo. You will need to
type the password or use `echo 'ubunturock' | sudo -S <command>`.

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
(`ubuntu` / `ubunturock`) or continue via SSH.

If the console doesn't appear after stopping the service, switch to tty1:
```bash
sudo chvt 1
```

### Console font size

The default console font may be small on the 720x1280 display. To set a
larger font:

```bash
sudo dpkg-reconfigure console-setup
# Choose: UTF-8 -> Guess -> Terminus -> 16x32 (or largest available)
```

Or set it directly:
```bash
sudo setfont /usr/share/consolefonts/Uni3-TerminusBold32x16.psf.gz 2>/dev/null \
  || sudo apt install console-setup && sudo setfont Lat15-TerminusBold32x16.psf.gz
```

### ratatui over SSH

For Phase 1 development, the ratatui TUI runs in the SSH terminal. The
Rock's physical display is not needed until Phase 4 (hardware integration).
The SSH session gives full terminal capabilities for ratatui rendering.

---

## Development Workflow

### Why build on the Rock

The X61s is an x86_64 machine. The Rock 5A is aarch64. Cross-compiling Rust
with C/C++ dependencies (llama.cpp, whisper.cpp) is complex and fragile. The
simpler path is to build natively on the Rock.

The Rock 5A's Cortex-A76 cores are reasonably capable for compilation.
Release builds will be slow, but debug builds during development are
acceptable.

### Workflow: edit on X61s, build on Rock

1. **Edit** code on the X61s in `~/projects/jhana-rs/`.

2. **Sync** to the Rock via scp:
   ```bash
   scp -r ~/projects/jhana-rs/ ubuntu@192.168.1.102:~/jhana-rs/
   ```

   Or use rsync for incremental syncs (faster after the first copy):
   ```bash
   rsync -avz --exclude target/ ~/projects/jhana-rs/ ubuntu@192.168.1.102:~/jhana-rs/
   ```

3. **Build and run** on the Rock via SSH:
   ```bash
   ssh ubuntu@192.168.1.102
   cd ~/jhana-rs
   cargo build
   cargo run
   ```

### Alternative: edit directly on Rock via SSH

Use your preferred terminal editor (vim, nano, helix) over SSH. This avoids
the scp step but is less comfortable for large edits.

### Alternative: git-based workflow

Once the GitHub repo is set up and the Rock has internet access (via NAT
forwarding from the X61s), you could push from the X61s and pull on the Rock.
This requires setting up IP forwarding on the laptop:

```bash
# On the X61s (one-time, until reboot)
sudo sysctl -w net.ipv4.ip_forward=1
sudo iptables -t nat -A POSTROUTING -o wlp3s0 -j MASQUERADE
sudo iptables -A FORWARD -i enp0s25 -o wlp3s0 -j ACCEPT
sudo iptables -A FORWARD -i wlp3s0 -o enp0s25 -m state --state RELATED,ESTABLISHED -j ACCEPT
```

(Replace `wlp3s0` with your internet-facing interface.)

Then on the Rock:
```bash
# Test internet access
ping -c 1 8.8.8.8

# Clone and pull
git clone https://github.com/carecodeconnect/jhana-rs.git
cd jhana-rs && git pull
```

---

## Install Rust Toolchain on the Rock

### Prerequisites

First, expand the root partition (only 917 MB free on the 29.7 GB eMMC):

```bash
# Check current partition layout
lsblk
# mmcblk1p3 is the root partition (10.1 GB on a 29.7 GB disk)

# Expand partition (CAREFUL -- backup first if needed)
sudo parted /dev/mmcblk1 resizepart 3 100%
sudo resize2fs /dev/mmcblk1p3

# Verify
df -h /
```

### Install Rust

```bash
curl --proto '=https' --tlsv1.2 -sSf https://sh.rustup.rs | sh
source ~/.cargo/env

# Build dependencies
sudo apt install build-essential cmake pkg-config libasound2-dev
```

### Verify

```bash
rustc --version
cargo --version
cargo clippy --version
rustfmt --version
```

---

## Expand Disk Space

The 32 GB eMMC has ~19 GB of unpartitioned space. The NLLB translation model
(~1.2 GB) in `/home/ubuntu/ai_in_a_box/downloaded/nllb-200-distilled-600M/`
is not needed for jhana-rs and can be deleted to free additional space.

```bash
# Delete translation model (not needed for jhana-rs)
rm -rf /home/ubuntu/ai_in_a_box/downloaded/nllb-200-distilled-600M/

# Check free space after partition expansion + model deletion
df -h /
# Should show ~20+ GB free
```
