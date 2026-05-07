#!/usr/bin/env bash
# Read the jhana-rs TUI log from the Rock.
# Use -f flag to tail (follow) the log live.
SCRIPT_DIR="$(cd "$(dirname "$0")" && pwd)"
if [ "$1" = "-f" ]; then
  "$SCRIPT_DIR/rock-ssh.sh" "tail -f /home/ubuntu/jhana-rs/jhana-rs.log"
else
  "$SCRIPT_DIR/rock-ssh.sh" "cat /home/ubuntu/jhana-rs/jhana-rs.log"
fi
