#!/usr/bin/env bash
# Sync jhana-rs source to the Rock 5A (excludes build artifacts and .git).
set -e
SCRIPT_DIR="$(cd "$(dirname "$0")" && pwd)"
PROJECT_DIR="$(dirname "$SCRIPT_DIR")"
CONFIG="$PROJECT_DIR/config.json"

ROCK_IP="${ROCK_IP:-$(jq -r '.rock.ip' "$CONFIG")}"
ROCK_USER="$(jq -r '.rock.user' "$CONFIG")"
ROCK_PASS="$(jq -r '.rock.password' "$CONFIG")"
sshpass -p "$ROCK_PASS" rsync -avz --delete \
  --exclude target/ --exclude '.git/' --exclude 'jhana-rs.log' \
  -e "ssh -o StrictHostKeyChecking=no" \
  "$PROJECT_DIR/" "$ROCK_USER@$ROCK_IP":~/jhana-rs/
