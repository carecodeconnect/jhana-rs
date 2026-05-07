#!/usr/bin/env bash
# Sync jhana-rs source to the Rock 5A (excludes build artifacts and .git).
set -e
SCRIPT_DIR="$(cd "$(dirname "$0")" && pwd)"
PROJECT_DIR="$(dirname "$SCRIPT_DIR")"

sshpass -p 'ubunturock' rsync -avz \
  --exclude target/ --exclude '.git/' \
  -e "ssh -o StrictHostKeyChecking=no" \
  "$PROJECT_DIR/" ubuntu@192.168.1.102:~/jhana-rs/
