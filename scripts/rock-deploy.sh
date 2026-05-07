#!/usr/bin/env bash
# Full deploy: sync, build, and run on the Rock.
# Combines rock-sync.sh + rock-build.sh + rock-run.sh.
set -e
SCRIPT_DIR="$(cd "$(dirname "$0")" && pwd)"

echo "=== Syncing ==="
"$SCRIPT_DIR/rock-sync.sh"

echo "=== Building ==="
"$SCRIPT_DIR/rock-build.sh"

echo "=== Launching ==="
"$SCRIPT_DIR/rock-run.sh"
