#!/usr/bin/env bash
# Read the jhana-rs TUI log from the Rock.
# Use -f flag to tail (follow) the log live.
if [ "$1" = "-f" ]; then
  sshpass -p 'ubunturock' ssh -o StrictHostKeyChecking=no ubuntu@192.168.1.102 \
    "tail -f /home/ubuntu/jhana-rs/jhana-rs.log"
else
  sshpass -p 'ubunturock' ssh -o StrictHostKeyChecking=no ubuntu@192.168.1.102 \
    "cat /home/ubuntu/jhana-rs/jhana-rs.log"
fi
