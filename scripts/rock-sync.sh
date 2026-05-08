#!/usr/bin/env bash
# Sync jhana-rs source to the Rock 5A (excludes build artifacts and .git).
set -e
SCRIPT_DIR="$(cd "$(dirname "$0")" && pwd)"
PROJECT_DIR="$(dirname "$SCRIPT_DIR")"

ROCK_IP="${ROCK_IP:-192.168.1.83}"
sshpass -p 'ubunturock' rsync -avz --delete \
  --exclude target/ --exclude '.git/' --exclude 'jhana-rs.log' \
  -e "ssh -o StrictHostKeyChecking=no" \
  "$PROJECT_DIR/" ubuntu@"$ROCK_IP":~/jhana-rs/
