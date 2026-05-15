# Routine Dev Commands

Quick reference for daily development on the Rock 5A.
All commands read credentials from `config.json` via `scripts/rock-*.sh`.

---

## Auto-start the TUI at boot

The systemd unit at `hardware/jhana-rs.service` (deployed to
`/etc/systemd/system/jhana-rs.service` on the Rock) launches the TUI
on `tty1` at boot. It conflicts with `getty@tty1.service` so no login
prompt fights for the screen.

Enable / disable:

```bash
scripts/rock-ssh.sh "sudo systemctl enable jhana-rs.service"
scripts/rock-ssh.sh "sudo systemctl disable jhana-rs.service"
scripts/rock-ssh.sh "sudo systemctl status jhana-rs.service"
scripts/rock-ssh.sh "sudo journalctl -u jhana-rs.service -b --no-pager"
```

Logs from the binary itself land in `/home/ubuntu/jhana-rs/jhana-rs.log`.

---

## SSH into the Rock

```bash
scripts/rock-ssh.sh
```

Or via Tailscale (no password needed):
```bash
ssh root@rock-5a
```

## Sync code to Rock

```bash
scripts/rock-sync.sh
```

## Build and test on Rock

```bash
scripts/rock-build.sh
```

Or manually:
```bash
scripts/rock-ssh.sh "source ~/.cargo/env && cd ~/jhana-rs && cargo check && cargo build && cargo test"
```

## Run TUI on Rock display

```bash
scripts/rock-run.sh
```

Or manually:
```bash
# Suppress kernel console messages (safe -- still goes to dmesg/kern.log)
scripts/rock-ssh.sh "sudo dmesg --console-off"

# Clear tty1
scripts/rock-ssh.sh "sudo bash -c 'echo -e \"\033c\" > /dev/tty1'"

# Launch TUI on physical display
scripts/rock-ssh.sh "sudo bash -c 'cd /home/ubuntu/jhana-rs && TERM=linux setsid ./target/debug/jhana-rs </dev/tty1 >/dev/tty1 2>/dev/tty1 &'"
```

## Stop TUI

```bash
scripts/rock-stop.sh
```

Or manually:
```bash
scripts/rock-ssh.sh "sudo pkill jhana-rs"
```

## Read TUI log

```bash
scripts/rock-log.sh
```

Or tail live:
```bash
scripts/rock-ssh.sh "tail -f /home/ubuntu/jhana-rs/jhana-rs.log"
```

## Test the full meditation flow

1. Sync and build:
   ```bash
   scripts/rock-sync.sh
   scripts/rock-build.sh
   ```
2. Launch TUI on Rock display:
   ```bash
   scripts/rock-run.sh
   ```
3. Press ENTER/→ button on Rock to start meditation
4. Watch text stream sentence-by-sentence with [N] pause markers
5. Use UP/DOWN arrows to scroll
6. Press BACK/← to quit
7. View log:
   ```bash
   scripts/rock-log.sh
   ```

## Restore kernel console messages

```bash
scripts/rock-ssh.sh "sudo dmesg --console-on"
```

## Build rustdoc

```bash
scripts/rock-ssh.sh "source ~/.cargo/env && cd ~/jhana-rs && cargo doc --no-deps"
```

## Check disk space

```bash
scripts/rock-ssh.sh "df -h / && free -h"
```

## Inspect a failed boot after recovery

Persistent journald is enabled (`Storage=persistent` in
`/etc/systemd/journald.conf`, written through `/var/log/journal →
/var/log.hdd/journal`). After recovering from a boot that hung in
userspace, fetch the previous boot's logs from a freshly-booted Rock:

```bash
scripts/rock-ssh.sh "sudo journalctl -b -1 --no-pager | tail -200"
scripts/rock-ssh.sh "sudo journalctl -b -1 -p err --no-pager"
scripts/rock-ssh.sh "sudo journalctl -b -1 -u systemd-udevd -u systemd-modules-load --no-pager"
```

If the kernel hung before userspace, journald has nothing; you need
a UART debug adapter on the Rock's 3-pin debug header.

## Mount the Rock's microSD on the dev machine (recovery)

When an audio/display overlay breaks boot the only way back in is to
pull the microSD, mount it on the dev machine, edit
`/boot/armbianEnv.txt`, and remove the offending overlay from the
`overlays=` line (see `docs/09_AUDIO.md` "DT overlay breaks
networking").

Ubuntu desktops sometimes don't auto-mount Armbian ext4 cards. Use
the desktop's udisks2 service — it mounts to `/media/$USER/<label>`
without sudo:

```bash
udisksctl mount -b /dev/mmcblk0p1
# → "Mounted /dev/mmcblk0p1 at /media/$USER/armbi_root"
```

The card is then writable as root only; use `sudo` to edit
`/media/$USER/armbi_root/boot/armbianEnv.txt`. Unmount cleanly when
done:

```bash
udisksctl unmount -b /dev/mmcblk0p1
```

(The block device name may be `mmcblk0p1` for the internal SD slot
or `sdX1` for a USB adapter — check `lsblk` first.)
