#!/usr/bin/env bash
# Build and test jhana-rs on the Rock 5A.
set -e
sshpass -p 'ubunturock' ssh -o StrictHostKeyChecking=no ubuntu@192.168.1.102 \
  "source ~/.cargo/env && cd ~/jhana-rs && cargo check && cargo build && cargo test"
