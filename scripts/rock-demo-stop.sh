#!/usr/bin/env bash
# Stop a running demo recording and retrieve the file.
#
# Stops ffmpeg and jhana-rs, lists recorded demos.
set -e
SCRIPT_DIR="$(cd "$(dirname "$0")" && pwd)"
CONFIG="$SCRIPT_DIR/../config.json"
ROCK_IP="${ROCK_IP:-$(jq -r '.rock.ip' "$CONFIG")}"
ROCK_USER="$(jq -r '.rock.user' "$CONFIG")"

"$SCRIPT_DIR/rock-ssh.sh" "
  sudo bash -c '
    # Stop TUI
    kill \$(cat /tmp/jhana_demo.pid 2>/dev/null) 2>/dev/null; true
    # Stop recording (SIGINT for clean ffmpeg close)
    kill -INT \$(cat /tmp/ffmpeg_demo.pid 2>/dev/null) 2>/dev/null; true
    rm -f /tmp/jhana_demo.pid /tmp/ffmpeg_demo.pid
  '
  sleep 2
  echo ''
  echo '=== Recorded demos ==='
  ls -lh /home/ubuntu/jhana-rs/demos/ 2>/dev/null || echo 'No demos yet'
"

echo ""
echo "To download a demo to x61s:"
echo "  scripts/rock-ssh.sh 'ls ~/jhana-rs/demos/'"
echo "  scp $ROCK_USER@$ROCK_IP:~/jhana-rs/demos/demo_*.mp4 ."
