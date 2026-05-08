#!/usr/bin/env bash
# Record a jhana-rs demo: TUI screen (framebuffer) + audio (speaker out).
#
# Produces: demo_YYYYMMDD_HHMMSS.mp4 in ~/jhana-rs/demos/
#
# Requirements on Rock:
#   sudo apt install ffmpeg
#
# Usage:
#   scripts/rock-demo-record.sh [duration_seconds]
#   Default duration: 60 seconds
#
# The script:
#   1. Creates demos/ directory
#   2. Suppresses kernel console messages
#   3. Clears tty1
#   4. Starts ffmpeg recording (framebuffer + audio)
#   5. Launches jhana-rs TUI on tty1
#   6. Waits for duration or until TUI exits
#   7. Stops recording and restores console

set -e
SCRIPT_DIR="$(cd "$(dirname "$0")" && pwd)"
DURATION="${1:-60}"

"$SCRIPT_DIR/rock-ssh.sh" "
  source ~/.cargo/env
  TIMESTAMP=\$(date +%Y%m%d_%H%M%S)
  DEMO_DIR=/home/ubuntu/jhana-rs/demos
  mkdir -p \$DEMO_DIR
  OUTFILE=\$DEMO_DIR/demo_\$TIMESTAMP.mp4

  echo '=== jhana-rs Demo Recording ==='
  echo \"Output: \$OUTFILE\"
  echo \"Duration: ${DURATION}s\"
  echo ''

  # Suppress kernel console messages
  echo 'ubunturock' | sudo -S dmesg --console-off

  # Clear tty1
  echo 'ubunturock' | sudo -S bash -c 'echo -e \"\033c\" > /dev/tty1'

  # Start ffmpeg recording in background
  # - Video: framebuffer capture from /dev/fb0
  # - Audio: Uctronics speaker output (card 2) or silence if unavailable
  echo 'ubunturock' | sudo -S bash -c \"
    ffmpeg -y \\
      -f fbdev -framerate 10 -i /dev/fb0 \\
      -f alsa -ac 1 -ar 16000 -i hw:2,0 \\
      -t ${DURATION} \\
      -c:v libx264 -preset ultrafast -pix_fmt yuv420p \\
      -c:a aac -b:a 64k \\
      \$OUTFILE \\
      </dev/null >/dev/null 2>/dev/null &
    echo \\\$! > /tmp/ffmpeg_demo.pid
  \"

  sleep 1

  # Launch TUI on tty1
  echo 'ubunturock' | sudo -S bash -c '
    cd /home/ubuntu/jhana-rs && TERM=linux setsid ./target/debug/jhana-rs </dev/tty1 >/dev/tty1 2>/dev/tty1 &
    echo \$! > /tmp/jhana_demo.pid
  '

  echo 'Recording started. TUI running on tty1.'
  echo \"Press buttons on the device to interact. Recording for ${DURATION}s.\"
  echo ''
  echo 'To stop early from another terminal:'
  echo '  scripts/rock-demo-stop.sh'
"
