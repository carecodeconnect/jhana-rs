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
# PA owns the Uctronics codec in system mode, so route through PA's
# ALSA plug for `arecord -D pulse` and paplay for output.
export PULSE_SERVER="${PULSE_SERVER:-unix:/var/run/pulse/native}"
PA_SINK="alsa_output.platform-uctronics-sound.stereo-fallback"
MIC="pulse"

PIPER=/usr/local/bin/piper
PIPER_MODEL=/home/ubuntu/models/vits-piper-en_US-lessac-medium/en_US-lessac-medium-ir8.onnx
PIPER_CONFIG=/home/ubuntu/models/vits-piper-en_US-lessac-medium/en_US-lessac-medium.onnx.json

echo "==> Generating spoken cues + chime via espeak-ng + ffmpeg..."
# Cue config (chosen 2026-05-15 after A/B):
#   espeak-ng -a 100 (default amplitude) + -s 145 (calm meditation pace),
#   piped through ffmpeg only for sample-rate conversion to 48 kHz.
#   No ffmpeg volume boost — any boost on top of espeak's default ouput
#   distorted on the Uctronics speaker. The 1-2 kHz speaker peak means
#   the formant-synth voice comes through clearly even without gain.
gen_speech() {
  local text="$1" out="$2" tmp="${out}.raw.wav"
  espeak-ng -a 100 -s 145 -w "$tmp" "$text" >/dev/null 2>&1
  ffmpeg -y -hide_banner -loglevel error \
    -i "$tmp" -ar 48000 -ac 1 -sample_fmt s16 "$out"
  rm -f "$tmp"
}
gen_speech "Speak now." "$BEEP_START"
gen_speech "Stop."      "$BEEP_STOP"

echo
echo "Get ready — cue coming in 2 seconds..."
sleep 2
# Play "Speak now" through the speaker so the user hears the cue.
paplay --device "$PA_SINK" "$BEEP_START" 2>/dev/null
sleep 0.3  # let speaker amp pop decay before sampling
arecord -D "$MIC" -f S32_LE -r 48000 -c 1 -d 5 "$WAV_IN_NATIVE" 2>&1 | tail -1
# "Stop" cue — also lets the user know recording ended.
paplay --device "$PA_SINK" "$BEEP_STOP" 2>/dev/null

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
      paplay --device "$PA_SINK" "$WAV_OUT" 2>&1 | tail -1
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
