#!/usr/bin/env bash
# Stop a running demo recording and retrieve the file.
#
# Stops ffmpeg and jhana-rs, lists recorded demos.
set -e
SCRIPT_DIR="$(cd "$(dirname "$0")" && pwd)"

"$SCRIPT_DIR/rock-ssh.sh" "
  echo 'ubunturock' | sudo -S bash -c '
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
echo "  sshpass -p 'ubunturock' scp ubuntu@192.168.1.102:~/jhana-rs/demos/demo_*.mp4 ."
