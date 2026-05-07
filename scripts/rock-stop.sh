#!/usr/bin/env bash
# Stop the jhana-rs TUI on the Rock.
SCRIPT_DIR="$(cd "$(dirname "$0")" && pwd)"
"$SCRIPT_DIR/rock-ssh.sh" "echo 'ubunturock' | sudo -S pkill jhana-rs 2>/dev/null; true"
echo "TUI stopped."
