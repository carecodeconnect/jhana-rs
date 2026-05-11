#!/usr/bin/env bash
# SSH into the Rock 5A.
# IP/user/password read from config.json (override with ROCK_IP env var).
SCRIPT_DIR="$(cd "$(dirname "$0")" && pwd)"
CONFIG="$SCRIPT_DIR/../config.json"
ROCK_IP="${ROCK_IP:-$(jq -r '.rock.ip' "$CONFIG")}"
ROCK_USER="$(jq -r '.rock.user' "$CONFIG")"
ROCK_PASS="$(jq -r '.rock.password' "$CONFIG")"
sshpass -p "$ROCK_PASS" ssh -o StrictHostKeyChecking=no "$ROCK_USER@$ROCK_IP" "$@"
