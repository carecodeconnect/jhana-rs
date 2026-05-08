//! Text-to-speech via Piper CLI.
//!
//! Runs TTS on a background thread, receiving sentences via an mpsc channel.
//! Each sentence is piped to `piper` CLI which outputs a WAV file, then
//! played via `aplay`. Uses the same thread + mpsc pattern as GPIO and LLM.
//!
//! # Why Piper CLI?
//!
//! sherpa-onnx was removed due to a protobuf-lite symbol clash with
//! sensevoice-rs's sentencepiece-sys dependency. See `docs/05_NPU.md`.
//! Piper CLI produces identical output — same model, same ONNX runtime,
//! invoked as a subprocess instead of FFI.
//!
//! # Stopgap
//!
//! This is a temporary solution. The goal is pure Rust TTS on NPU via
//! a piper-rknn-rs crate (fork of piper-rs with candle + rknn-rs backend).

use std::process::Command;
use std::sync::mpsc::{Receiver, Sender};

use log::{error, info};

/// Messages sent to the TTS thread.
#[derive(Debug)]
pub enum TtsCommand {
    /// Speak this sentence aloud.
    Speak(String),
    /// Stop any pending speech (flush the queue).
    #[expect(dead_code)] // will be used when cancel-generation is wired
    Stop,
}

/// Path to the Piper CLI binary on the Rock.
const PIPER_BIN: &str = "/usr/local/bin/piper";

/// Path to the Piper ONNX model on the Rock.
const MODEL_PATH: &str =
    "/home/ubuntu/models/vits-piper-en_US-lessac-medium/en_US-lessac-medium.onnx";

/// ALSA playback device (Uctronics onboard speaker with plughw for mono->stereo).
const PLAYBACK_DEVICE: &str = "plughw:2,0";

/// Speech rate — higher values produce slower, calmer speech.
/// 1.0 = normal, 1.3 = meditation pace, 2.0 = very slow.
const LENGTH_SCALE: f32 = 1.3;

/// Temporary WAV file for TTS output.
const WAV_PATH: &str = "/tmp/jhana_tts.wav";

/// Start the TTS background thread.
///
/// Returns a `Sender<TtsCommand>` for sending sentences to be spoken.
/// The thread processes sentences sequentially — each one is synthesized
/// via Piper CLI and played via `aplay`. If Piper is not installed,
/// sentences are logged but not spoken (graceful degradation).
pub fn start() -> Sender<TtsCommand> {
    let (tx, rx) = std::sync::mpsc::channel::<TtsCommand>();

    std::thread::Builder::new()
        .name("tts".into())
        .spawn(move || {
            info!("TTS thread started (Piper CLI)");
            tts_loop(&rx);
            info!("TTS thread exiting");
        })
        .expect("failed to spawn TTS thread");

    tx
}

/// TTS event loop — processes sentences until channel closes.
fn tts_loop(rx: &Receiver<TtsCommand>) {
    // Verify Piper is installed
    match Command::new(PIPER_BIN).arg("--help").output() {
        Ok(output) if output.status.success() || output.status.code() == Some(1) => {
            info!("Piper CLI found at {PIPER_BIN}");
        }
        _ => {
            error!("Piper CLI not found at {PIPER_BIN} — TTS disabled");
            while let Ok(cmd) = rx.recv() {
                if let TtsCommand::Speak(s) = cmd {
                    info!("TTS unavailable, skipping: {s}");
                }
            }
            return;
        }
    }

    while let Ok(cmd) = rx.recv() {
        match cmd {
            TtsCommand::Speak(sentence) => {
                speak_sentence(&sentence);
            }
            TtsCommand::Stop => {
                info!("TTS stop requested");
                while rx.try_recv().is_ok() {}
            }
        }
    }
}

/// Synthesize a sentence via Piper CLI and play via aplay.
fn speak_sentence(sentence: &str) {
    let start = std::time::Instant::now();

    // Piper CLI: echo "text" | piper --model X --output_file Y --length_scale Z
    let piper_status = Command::new(PIPER_BIN)
        .args([
            "--model",
            MODEL_PATH,
            "--output_file",
            WAV_PATH,
            "--length_scale",
            &LENGTH_SCALE.to_string(),
        ])
        .stdin(std::process::Stdio::piped())
        .stdout(std::process::Stdio::null())
        .stderr(std::process::Stdio::piped())
        .spawn()
        .and_then(|mut child| {
            use std::io::Write;
            if let Some(ref mut stdin) = child.stdin {
                let _ = stdin.write_all(sentence.as_bytes());
            }
            child.wait()
        });

    match piper_status {
        Ok(status) if status.success() => {
            let synth_time = start.elapsed();
            info!(
                "TTS: {:.2}s synth — {}",
                synth_time.as_secs_f32(),
                &sentence[..sentence.len().min(50)]
            );
        }
        Ok(status) => {
            error!(
                "Piper failed with {status} for: {}",
                &sentence[..sentence.len().min(50)]
            );
            return;
        }
        Err(e) => {
            error!("Piper error: {e}");
            return;
        }
    }

    // Play via aplay
    match Command::new("aplay")
        .args(["-D", PLAYBACK_DEVICE, WAV_PATH])
        .output()
    {
        Ok(output) if output.status.success() => {}
        Ok(output) => {
            let stderr = String::from_utf8_lossy(&output.stderr);
            error!("aplay failed: {stderr}");
        }
        Err(e) => error!("aplay error: {e}"),
    }
}
