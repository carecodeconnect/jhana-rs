#!/usr/bin/env bash
# Stop the jhana-rs TUI on the Rock.
sshpass -p 'ubunturock' ssh -o StrictHostKeyChecking=no ubuntu@192.168.1.102 \
  "echo 'ubunturock' | sudo -S pkill jhana-rs 2>/dev/null; true"
echo "TUI stopped."
