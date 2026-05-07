#!/usr/bin/env bash
# Sync jhana-rs source to the Rock 5A (excludes build artifacts and .git).
set -e
SCRIPT_DIR="$(cd "$(dirname "$0")" && pwd)"
PROJECT_DIR="$(dirname "$SCRIPT_DIR")"

sshpass -p 'ubunturock' rsync -avz --delete \
  --exclude target/ --exclude '.git/' --exclude 'jhana-rs.log' \
  -e "ssh -o StrictHostKeyChecking=no" \
  "$PROJECT_DIR/" ubuntu@192.168.1.102:~/jhana-rs/
