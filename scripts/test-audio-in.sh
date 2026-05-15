#!/usr/bin/env bash
# Smoke-test audio IN on the Rock 5A.
#
# Records 3 s from the Uctronics mic via the ALSA→PA plugin
# (PA owns the codec so direct plughw:1,0 returns "device busy")
# at S32_LE 48 kHz, then reports the RMS level so we can tell if
# any sound was captured. Stops jhana-rs.service first to avoid
# fighting it for the source.
set -e

SCRIPT_DIR="$(cd "$(dirname "$0")" && pwd)"

"$SCRIPT_DIR/rock-ssh.sh" "
  set -e
  echo '--- stopping jhana-rs so it does not compete for PA ---'
  sudo systemctl stop jhana-rs.service 2>/dev/null || true

  export PULSE_SERVER=unix:/var/run/pulse/native
  SRC=alsa_input.platform-uctronics-sound.stereo-fallback

  echo '--- pa source state ---'
  pactl list short sources | grep uctronics
  echo 'default-source: '\$(pactl get-default-source)

  echo
  echo '*** SPEAK NOW (3 s recording at S32_LE 48 kHz via -D pulse) ***'
  arecord -D pulse -f S32_LE -r 48000 -c 1 -d 3 /tmp/test-in.wav 2>&1 | tail -1

  echo
  echo '--- recorded signal levels ---'
  python3 - <<'PY'
import math, struct, wave
with wave.open('/tmp/test-in.wav', 'rb') as w:
    sw = w.getsampwidth(); sr = w.getframerate(); n = w.getnframes()
    raw = w.readframes(n)
fmt = 'i' if sw == 4 else 'h'
s = struct.unpack('<' + fmt * (len(raw) // sw), raw)
peak = max(abs(min(s)), max(s))
rms = math.sqrt(sum(v*v for v in s) / len(s))
hi = (1 << (sw * 8 - 1))
print(f'samples={n} sr={sr} sw={sw} peak={peak} ({peak/hi:.3%}) rms={rms:.0f} ({rms/hi:.3%})')
if rms < hi * 0.001:
    print('=> rms < 0.1 % full scale: probably no audio captured')
else:
    print('=> rms looks audible')
PY
"
