#!/usr/bin/env bash
# Launch jhana-rs TUI on the Rock's physical display (tty1).
# Suppresses kernel console messages and clears the screen first.
set -e
ROCK="sshpass -p ubunturock ssh -o StrictHostKeyChecking=no ubuntu@192.168.1.102"

# Kill any existing instance
$ROCK "echo 'ubunturock' | sudo -S pkill jhana-rs 2>/dev/null; true"

# Suppress kernel console messages (still logged to dmesg/kern.log)
$ROCK "echo 'ubunturock' | sudo -S dmesg --console-off"

# Clear tty1
$ROCK "echo 'ubunturock' | sudo -S bash -c 'echo -e \"\033c\" > /dev/tty1'"

# Launch TUI
$ROCK "echo 'ubunturock' | sudo -S bash -c 'cd /home/ubuntu/jhana-rs && TERM=linux setsid ./target/debug/jhana-rs </dev/tty1 >/dev/tty1 2>/dev/tty1 &'"

echo "TUI launched on Rock tty1. Use 'scripts/rock-log.sh' to view output."
