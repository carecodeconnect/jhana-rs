#!/usr/bin/env bash
# Launch jhana-rs TUI on the Rock's physical display (tty1).
# Uses a single SSH connection to avoid repeated handshake delay.
set -e
SCRIPT_DIR="$(cd "$(dirname "$0")" && pwd)"

"$SCRIPT_DIR/rock-ssh.sh" "
  sudo bash -c '
    pkill jhana-rs 2>/dev/null; true
    # Stop getty@tty1 so it does not redraw a login prompt over the TUI.
    # restore_console() in src/main.rs will start it again on exit.
    systemctl stop getty@tty1.service 2>/dev/null; true
    dmesg --console-off
    echo -e \"\033c\" > /dev/tty1
    cd /home/ubuntu/jhana-rs && TERM=linux setsid ./target/debug/jhana-rs </dev/tty1 >/dev/tty1 2>/dev/tty1 &
  '
"

echo "TUI launched on Rock tty1. Use 'scripts/rock-log.sh' to view output."
