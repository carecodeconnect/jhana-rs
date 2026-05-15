#!/usr/bin/env bash
# Smoke-test audio OUT on the Rock 5A.
#
# Synthesises a short espeak-ng phrase and plays it via PulseAudio on
# the Uctronics speaker. Useful for confirming the playback path is
# alive without running the full jhana-rs TUI. Stops jhana-rs.service
# first so we're not fighting it for the PA sink.
set -e

SCRIPT_DIR="$(cd "$(dirname "$0")" && pwd)"

"$SCRIPT_DIR/rock-ssh.sh" "
  set -e
  echo '--- stopping jhana-rs so it does not compete for PA ---'
  sudo systemctl stop jhana-rs.service 2>/dev/null || true

  export PULSE_SERVER=unix:/var/run/pulse/native
  SINK=alsa_output.platform-uctronics-sound.stereo-fallback

  echo '--- pa sink state ---'
  pactl list short sinks | grep uctronics
  echo 'volume:   '\$(pactl get-sink-volume \$SINK | head -1 | awk -F'/' '{print \$2}' | tr -d ' ')
  echo 'mute:     '\$(pactl get-sink-mute \$SINK)

  echo '--- 1) sine beep ---'
  ffmpeg -y -hide_banner -loglevel error -f lavfi \
    -i 'sine=frequency=440:duration=1' \
    -af 'afade=t=out:st=0.05:d=0.95' \
    -ar 48000 -ac 1 -sample_fmt s16 /tmp/test-out-beep.wav
  paplay --device \$SINK /tmp/test-out-beep.wav
  sleep 0.3

  echo '--- 2) espeak speech ---'
  espeak-ng -a 100 -s 145 -w /tmp/test-out-speech.wav 'Audio out test.'
  paplay --device \$SINK /tmp/test-out-speech.wav

  echo '--- done ---'
"
