//! Text-to-speech baseline via espeak-ng.
//!
//! Runs TTS on a background thread, receiving sentences via an mpsc channel.
//! Each sentence is synthesised by espeak-ng and played via `aplay` on the
//! Uctronics speaker. Uses the same thread + mpsc pattern as GPIO and LLM.
//!
//! # Why espeak-ng (right now)
//!
//! Piper CLI is broken on Armbian: `libpiper_phonemize.so.1` references
//! `espeak_TextToPhonemesWithTerminator`, which Armbian's espeak-ng 1.51
//! does not export. See docs/12_TROUBLESHOOTING.md.
//!
//! espeak-ng is the simplest working baseline: pure formant synthesis,
//! single binary, no neural model, no GPU/NPU. The voice is robotic but
//! always available. We can verify the full pipeline (LLM → TTS thread →
//! audible speaker output) without untangling the C++ Piper toolchain.
//!
//! # Next step
//!
//! Move to `piper-rs` (Rust crate, neural VITS, natural voice) on CPU,
//! then `piper-rknn-rs` on the RK3588 NPU for the VITS decoder
//! (~4× speedup). See docs/14_TODO.md.

use std::process::Command;
use std::sync::mpsc::{Receiver, Sender};

use log::{error, info};

/// Messages sent to the TTS thread.
#[derive(Debug)]
pub enum TtsCommand {
    /// Speak this sentence aloud.
    Speak(String),
    /// Silent pause for this many seconds. Honoured by the TTS thread
    /// (it sleeps) so the user actually experiences the pause instead
    /// of hearing "fifteen seconds" spoken aloud.
    Pause(f32),
    /// Ring the meditation bell.
    Bell,
    /// Stop any pending speech (flush the queue).
    #[expect(dead_code)] // will be used when cancel-generation is wired
    Stop,
}

/// PulseAudio system-mode socket (set up by pulseaudio.service on the
/// Rock). We use PA rather than raw `aplay` so we get software mixing,
/// the original baseline's 100 %-sink-volume loudness path, and no
/// per-utterance ALSA open (which on the Uctronics codec triggers a
/// speaker-amp pop every time). See docs/09_AUDIO.md "Reference: the
/// original AI in a Box loudness path".
const PULSE_SERVER: &str = "unix:/var/run/pulse/native";

/// PulseAudio sink the Uctronics speaker exposes. Set as the default
/// sink in /etc/pulse/system.pa, but we pass it explicitly here to be
/// robust against module-default-device-restore resetting it.
const PULSE_SINK: &str = "alsa_output.platform-uctronics-sound.stereo-fallback";

/// espeak-ng amplitude (0–200). 100 is the default and was the cleanest
/// non-distorting setting on the Uctronics speaker in A/B testing —
/// any higher and the small enclosure speaker clips. See docs/09_AUDIO.md.
const ESPEAK_AMPLITUDE: &str = "100";

/// espeak-ng speech rate (words/min). 145 is calm enough for meditation
/// without dragging.
const ESPEAK_RATE: &str = "145";

/// Temporary WAV file for TTS output.
const WAV_PATH: &str = "/tmp/jhana_tts.wav";

/// Pre-rendered meditation-bell WAV. Generated once at TTS-thread
/// startup via ffmpeg (a 523 Hz / C5 sine with a 2 s exponential decay)
/// so the LLM's `[BELL]` marker can trigger an audible chime without
/// shipping a binary asset.
const BELL_WAV: &str = "/tmp/jhana_bell.wav";

/// Start the TTS background thread.
///
/// Returns a `Sender<TtsCommand>` for sending sentences to be spoken.
/// The thread processes sentences sequentially — each one is synthesised
/// via espeak-ng and played via `aplay`. If espeak-ng is missing the
/// thread logs a one-time error and silently drops further sentences
/// (graceful degradation — the rest of the pipeline keeps working).
pub fn start() -> Sender<TtsCommand> {
    let (tx, rx) = std::sync::mpsc::channel::<TtsCommand>();

    std::thread::Builder::new()
        .name("tts".into())
        .spawn(move || {
            info!("TTS thread started (espeak-ng baseline)");
            tts_loop(&rx);
            info!("TTS thread exiting");
        })
        .expect("failed to spawn TTS thread");

    tx
}

/// TTS event loop — processes sentences until the channel closes.
fn tts_loop(rx: &Receiver<TtsCommand>) {
    // One-time availability check so we don't fight to spawn espeak per sentence.
    match Command::new("espeak-ng").arg("--version").output() {
        Ok(output) if output.status.success() => {
            info!("espeak-ng available");
        }
        _ => {
            error!("espeak-ng not found — TTS will be silent");
            while let Ok(cmd) = rx.recv() {
                if let TtsCommand::Speak(s) = cmd {
                    info!("TTS unavailable, skipping: {s}");
                }
            }
            return;
        }
    }

    // Pre-render the meditation bell once. Cheap (<100 ms) but avoids
    // re-synth on every [BELL] marker.
    render_bell();

    while let Ok(cmd) = rx.recv() {
        match cmd {
            TtsCommand::Speak(sentence) => speak_sentence(&sentence),
            TtsCommand::Pause(seconds) => {
                let clamped = seconds.clamp(0.0, 120.0);
                info!("TTS: pausing {clamped:.1}s");
                std::thread::sleep(std::time::Duration::from_millis(
                    (clamped * 1000.0) as u64,
                ));
            }
            TtsCommand::Bell => {
                info!("TTS: ring bell");
                play_wav(BELL_WAV);
            }
            TtsCommand::Stop => {
                info!("TTS stop requested");
                while rx.try_recv().is_ok() {}
            }
        }
    }
}

/// Generate a meditation-bell WAV at startup. We mix three sine
/// partials with separate decay envelopes to approximate a Tibetan
/// singing-bowl-ish sound rather than a flat synth beep:
///
///   - Fundamental 523 Hz (C5)   — loud, slow 4 s decay
///   - Octave     1046 Hz (C6)   — softer, 1.5 s decay (the "ping")
///   - Just fifth  784 Hz (G5)   — middle, 2.0 s decay (warmth)
///
/// Idempotent: re-renders on every run (cheap, <200 ms on the Rock).
/// Cleaner than shipping a binary asset and works on any host where
/// ffmpeg + lavfi are installed.
fn render_bell() {
    let result = Command::new("ffmpeg")
        .args([
            "-y",
            "-hide_banner",
            "-loglevel",
            "error",
            "-f", "lavfi", "-i", "sine=frequency=523:duration=4",
            "-f", "lavfi", "-i", "sine=frequency=1046:duration=4",
            "-f", "lavfi", "-i", "sine=frequency=784:duration=4",
            "-filter_complex",
            "[0:a]volume=0.85,afade=t=out:st=0.05:d=3.9[a0];\
             [1:a]volume=0.35,afade=t=out:st=0.02:d=1.5[a1];\
             [2:a]volume=0.25,afade=t=out:st=0.03:d=2.0[a2];\
             [a0][a1][a2]amix=inputs=3:duration=longest:normalize=0",
            "-ar", "48000",
            "-ac", "1",
            "-sample_fmt", "s16",
            BELL_WAV,
        ])
        .status();
    match result {
        Ok(s) if s.success() => info!("bell rendered at {BELL_WAV}"),
        Ok(s) => error!("bell render failed: {s}"),
        Err(e) => error!("bell render error: {e}"),
    }
}

/// Play any pre-rendered WAV through the configured PA sink.
fn play_wav(path: &str) {
    match Command::new("paplay")
        .env("PULSE_SERVER", PULSE_SERVER)
        .args(["--device", PULSE_SINK, path])
        .output()
    {
        Ok(output) if output.status.success() => {}
        Ok(output) => error!("paplay failed: {}", String::from_utf8_lossy(&output.stderr)),
        Err(e) => error!("paplay error: {e}"),
    }
}

/// Synthesise `sentence` to WAV via espeak-ng and play it on the speaker.
fn speak_sentence(sentence: &str) {
    let preview = &sentence[..sentence.len().min(50)];
    let start = std::time::Instant::now();

    let synth = Command::new("espeak-ng")
        .args([
            "-a",
            ESPEAK_AMPLITUDE,
            "-s",
            ESPEAK_RATE,
            "-w",
            WAV_PATH,
            sentence,
        ])
        .output();

    match synth {
        Ok(output) if output.status.success() => {
            info!(
                "TTS: {:.2}s synth — {preview}",
                start.elapsed().as_secs_f32()
            );
        }
        Ok(output) => {
            error!(
                "espeak-ng failed ({}): {}",
                output.status,
                String::from_utf8_lossy(&output.stderr)
            );
            return;
        }
        Err(e) => {
            error!("espeak-ng error: {e}");
            return;
        }
    }

    match Command::new("paplay")
        .env("PULSE_SERVER", PULSE_SERVER)
        .args(["--device", PULSE_SINK, WAV_PATH])
        .output()
    {
        Ok(output) if output.status.success() => {}
        Ok(output) => {
            error!("paplay failed: {}", String::from_utf8_lossy(&output.stderr));
        }
        Err(e) => error!("paplay error: {e}"),
    }
}
