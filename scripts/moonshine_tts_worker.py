#!/usr/bin/env python3
"""Persistent Moonshine-Voice TTS worker.

Run as a long-lived child of the jhana-rs TTS thread. Loads the voice
once at startup (~3 s) and then accepts one synthesis request per stdin
line, writing a WAV and replying on stdout. Matches the paroli-cli
shape so the Rust side stays simple — `Command::spawn` then write/read
lines.

Protocol (newline-delimited JSON on each side):

  request:  {"text": "Hello.", "out": "/tmp/out.wav"}
  response: {"ok": true,  "out": "/tmp/out.wav", "infer_s": 0.83,
             "audio_s": 1.20, "sample_rate": 24000}
            {"ok": false, "error": "..."}

CLI args:
  --voice    Moonshine voice id (e.g. kokoro_am_michael)
  --language ISO code (default: en)
"""
import argparse
import json
import sys
import time
import wave

import numpy as np

from moonshine_voice.tts import TextToSpeech


def main() -> None:
    p = argparse.ArgumentParser()
    p.add_argument("--voice", required=True, help="e.g. kokoro_am_michael")
    p.add_argument("--language", default="en")
    args = p.parse_args()

    # Load voice once; subsequent synthesise() calls reuse the warm model.
    tts = TextToSpeech(language=args.language, voice=args.voice)

    # Signal ready so the Rust side can stop blocking on the first request.
    sys.stdout.write(json.dumps({"ready": True, "voice": args.voice}) + "\n")
    sys.stdout.flush()

    for line in sys.stdin:
        line = line.strip()
        if not line:
            continue
        try:
            req = json.loads(line)
            text = req["text"]
            out_path = req["out"]
        except (ValueError, KeyError) as e:
            sys.stdout.write(json.dumps({"ok": False, "error": f"bad request: {e}"}) + "\n")
            sys.stdout.flush()
            continue

        try:
            t0 = time.time()
            samples, sr = tts.synthesize(text)
            infer_s = time.time() - t0
            arr = (np.asarray(samples, dtype=np.float32) * 32767.0).clip(-32768, 32767).astype(np.int16)
            audio_s = float(len(arr)) / float(sr)
            with wave.open(out_path, "wb") as w:
                w.setnchannels(1)
                w.setsampwidth(2)
                w.setframerate(sr)
                w.writeframes(arr.tobytes())
            sys.stdout.write(json.dumps({
                "ok": True,
                "out": out_path,
                "infer_s": round(infer_s, 3),
                "audio_s": round(audio_s, 3),
                "sample_rate": sr,
            }) + "\n")
            sys.stdout.flush()
        except Exception as e:  # noqa: BLE001
            sys.stdout.write(json.dumps({"ok": False, "error": str(e)}) + "\n")
            sys.stdout.flush()


if __name__ == "__main__":
    main()
