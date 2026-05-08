# Voice Cloning Reference Samples

Copied from `jhana-dev/audio/` (Python prototype). Used with Coqui TTS
XTTS v2 for zero-shot voice cloning. WAV files are gitignored (large).

## Files

| File | Size | Format | Source |
|------|------|--------|--------|
| `voice-to-clone.wav` | 1.7 MB | 16-bit stereo 44100 Hz | Primary reference voice |
| `voice-to-clone-batman.wav` | 6.3 MB | 16-bit stereo 48000 Hz | Batman character voice |
| `voice-to-clone-tom-waits.wav` | 21 MB | 16-bit stereo 48000 Hz | Tom Waits style voice |
| `voice-to-clone-morgan-freeman.wav` | 16 MB | 16-bit stereo 48000 Hz | Morgan Freeman style voice |

## Usage in jhana-dev (Python)

```python
# src-eeg/speaker.py — XTTS v2 voice cloning
from TTS.api import TTS
tts = TTS("tts_models/multilingual/multi-dataset/xtts_v2")
tts.tts_to_file(text, speaker_wav="audio/voice-to-clone.wav", language="en", file_path="out.wav")
```

## Future: Rust integration

See `docs/TODO.md` Options G and H for planned Rust voice cloning paths
(rwkv-tts-rs, rknpu2 + custom RKNN TTS model).
