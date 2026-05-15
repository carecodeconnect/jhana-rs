#!/usr/bin/env bash
# End-to-end mic → STT → speaker test on the Rock.
#
# 1. Plays a "start" beep on the Uctronics speaker.
# 2. Records 5 s from the Uctronics mic (S32_LE 48 kHz native).
# 3. Plays a "stop" beep.
# 4. Resamples to S16_LE 16 kHz for SenseVoice.
# 5. Transcribes with the test_stt binary (RKNN backend).
# 6. Prints the transcription, and plays it back via Piper if available
#    (Piper is currently broken on Armbian due to libpiper_phonemize /
#    espeak-ng symbol mismatch — see docs/09_AUDIO.md and TODO.md).
set -e

cd "$(dirname "$0")/.."

WAV_IN_NATIVE=/tmp/jhana-e2e-in-native.wav
WAV_IN=/tmp/jhana-e2e-in.wav
WAV_OUT=/tmp/jhana-e2e-out.wav
BEEP_START=/tmp/jhana-e2e-start.wav
BEEP_STOP=/tmp/jhana-e2e-stop.wav
SPK="plughw:1,0"
MIC="plughw:1,0"

PIPER=/usr/local/bin/piper
PIPER_MODEL=/home/ubuntu/models/vits-piper-en_US-lessac-medium/en_US-lessac-medium-ir8.onnx
PIPER_CONFIG=/home/ubuntu/models/vits-piper-en_US-lessac-medium/en_US-lessac-medium.onnx.json

echo "==> Generating start/stop beeps with ffmpeg..."
# The onboard Uctronics speaker has a physical loudness ceiling that the
# 3-bit codec gain selector barely budges. The original AI in a Box gets
# its perceived loudness from PulseAudio (`pactl set-sink-volume 0xFFFF`)
# which combines software gain + soft-clipping. We approximate that here
# with `volume=20dB,alimiter` — the limiter compresses peaks so the
# average power rises even though the peak is already at 0 dBFS, which
# the small speaker hears as "louder". Sounds best around 1–2 kHz (the
# speaker's response peak); 440 Hz comes through much quieter.
LOUD='volume=20dB,alimiter=limit=0.95'

# "Start": 1500 Hz, 0.4 s — short and high so it's hard to miss.
ffmpeg -y -hide_banner -loglevel error \
  -f lavfi -i "sine=frequency=1500:duration=0.4" \
  -af "$LOUD" -ar 48000 -ac 1 -sample_fmt s16 "$BEEP_START"
# "Stop": 700 Hz, 0.6 s — lower so it's distinguishable from start.
ffmpeg -y -hide_banner -loglevel error \
  -f lavfi -i "sine=frequency=700:duration=0.6" \
  -af "$LOUD" -ar 48000 -ac 1 -sample_fmt s16 "$BEEP_STOP"

echo
echo "Get ready — beep coming in 2 seconds..."
sleep 2
aplay -q -D "$SPK" "$BEEP_START" 2>/dev/null
sleep 0.4  # let speaker amp pop decay before sampling
echo
echo "*** SPEAK NOW — 5 seconds — clear loud sentence ***"
arecord -D "$MIC" -f S32_LE -r 48000 -c 1 -d 5 "$WAV_IN_NATIVE" 2>&1 | tail -1
aplay -q -D "$SPK" "$BEEP_STOP" 2>/dev/null
echo "*** Recording finished ***"

echo
echo "==> Resampling to S16_LE 16 kHz mono for SenseVoice..."
ffmpeg -y -hide_banner -loglevel error \
  -i "$WAV_IN_NATIVE" -ac 1 -ar 16000 -sample_fmt s16 "$WAV_IN"

echo
echo "==> Running STT (SenseVoice via test_stt)..."
STT_OUTPUT=$(./target/release/test_stt "$WAV_IN" 2>&1)
TRANSCRIPT=$(echo "$STT_OUTPUT" \
  | awk '/Text: / { sub(/^[ ]*Text:[ ]*/,""); print }' \
  | tr -d '\n')

if [[ -z "$TRANSCRIPT" ]]; then
  echo "STT returned no text. Output:"
  echo "$STT_OUTPUT" | tail -15
  exit 1
fi

CLEAN=$(echo "$TRANSCRIPT" | sed -E 's/<\|[^|]*\|>//g; s/  +/ /g; s/^ +| +$//g')
echo
echo "============================="
echo "Transcript: \"$CLEAN\""
echo "============================="

# Try to round-trip through Piper if it works on this system.
if [[ -x "$PIPER" && -f "$PIPER_MODEL" ]]; then
  echo
  echo "==> Trying to synthesise transcription with Piper..."
  if echo "$CLEAN" | "$PIPER" --model "$PIPER_MODEL" --config "$PIPER_CONFIG" \
       --output_file "$WAV_OUT" --length_scale 1.3 2>/tmp/piper-err.log; then
    if [[ -s "$WAV_OUT" ]]; then
      echo "==> Playing back transcription on speaker..."
      aplay -q -D "$SPK" "$WAV_OUT" 2>&1 | tail -1
    else
      echo "Piper produced empty output; skipping playback."
      tail -3 /tmp/piper-err.log
    fi
  else
    echo "Piper failed (see /tmp/piper-err.log); skipping playback."
    tail -3 /tmp/piper-err.log
  fi
else
  echo "Piper not configured (model or binary missing); skipping playback."
fi

echo
echo "Done."
