# Routine Dev Commands

Quick reference for daily development on the Rock 5A.
All commands read credentials from `config.json` via `scripts/rock-*.sh`.

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
