//! Speech-to-text via sensevoice-rs with optional RKNN NPU backend.
//!
//! Runs STT on a background thread, receiving audio commands via an mpsc
//! channel. Uses the same thread + mpsc pattern as TTS, GPIO, and LLM.
//!
//! # Flow
//!
//! 1. Main thread sends `SttCommand::Listen` when user presses ENTER
//! 2. STT thread records from mic via `arecord` (5s, 16 kHz mono)
//! 3. SenseVoice transcribes the audio (includes built-in Silero VAD)
//! 4. Transcribed text sent back via `SttResult` channel
//!
//! # Why arecord for capture?
//!
//! Same rationale as aplay for TTS — the Rock 5A's Uctronics mic is on
//! ALSA card 0 (Armbian) or card 2 (old image), and shelling out avoids
//! a dependency. The STT thread blocks during recording, which is fine
//! since the user is speaking.

use std::path::PathBuf;
use std::process::Command;
use std::sync::mpsc::{Receiver, Sender};
use std::time::Instant;

use log::{error, info};
use sensevoice_rs::SenseVoiceSmall;
use sensevoice_rs::silero_vad::VadConfig;

/// ALSA capture device (Uctronics onboard mic).
/// Card 0 on Armbian 26.2.1, was card 2 on old Radxa Ubuntu image.
const CAPTURE_DEVICE: &str = "plughw:0,0";

/// Recording duration in seconds.
const RECORD_SECONDS: u32 = 5;

/// Temporary WAV file for mic capture.
const RECORD_PATH: &str = "/tmp/jhana_stt.wav";

/// Commands sent to the STT thread.
#[derive(Debug)]
pub enum SttCommand {
    /// Start listening: record from mic and transcribe.
    Listen,
    /// Shut down the STT thread.
    #[expect(dead_code)]
    Stop,
}

/// Results sent back from the STT thread.
#[derive(Debug, Clone)]
pub enum SttResult {
    /// Transcription of what the user said.
    Transcribed(String),
    /// STT is currently recording from the mic.
    Recording,
    /// STT is processing the audio.
    Processing,
    /// An error occurred.
    Error(String),
}

/// Start the STT background thread.
///
/// Returns a `Sender<SttCommand>` for triggering recording, and the
/// results come back on `result_tx` which should be polled in the
/// main event loop.
pub fn start(result_tx: Sender<SttResult>) -> Sender<SttCommand> {
    let (cmd_tx, cmd_rx) = std::sync::mpsc::channel::<SttCommand>();

    std::thread::Builder::new()
        .name("stt".into())
        .spawn(move || {
            info!("STT thread started");
            stt_loop(&cmd_rx, &result_tx);
            info!("STT thread exiting");
        })
        .expect("failed to spawn STT thread");

    cmd_tx
}

/// STT event loop — loads model once, then processes listen commands.
fn stt_loop(cmd_rx: &Receiver<SttCommand>, result_tx: &Sender<SttResult>) {
    // Use cached model files without contacting HuggingFace on every start.
    // First run requires internet; subsequent runs use ~/.cache/huggingface/.
    // Safety: called before any other threads use this env var.
    unsafe { std::env::set_var("HF_HUB_OFFLINE", "1") };

    info!("Initializing SenseVoiceSmall (offline, cached model)...");
    let load_start = Instant::now();

    let svs = match SenseVoiceSmall::init(VadConfig::default()) {
        Ok(model) => model,
        Err(e) => {
            error!("Failed to init SenseVoice model: {e}");
            // Drain channel — TUI still works, just no STT
            while let Ok(cmd) = cmd_rx.recv() {
                if let SttCommand::Listen = cmd {
                    let _ = result_tx.send(SttResult::Error(format!("STT model failed: {e}")));
                }
            }
            return;
        }
    };

    info!(
        "SenseVoice model loaded in {:.2}s",
        load_start.elapsed().as_secs_f32()
    );

    while let Ok(cmd) = cmd_rx.recv() {
        match cmd {
            SttCommand::Listen => {
                listen_and_transcribe(&svs, result_tx);
            }
            SttCommand::Stop => {
                info!("STT stop requested");
                break;
            }
        }
    }

    if let Err(e) = svs.destroy() {
        error!("SenseVoice cleanup error: {e}");
    }
}

/// Record from mic, transcribe, and send result.
fn listen_and_transcribe(svs: &SenseVoiceSmall, result_tx: &Sender<SttResult>) {
    // Signal that we're recording
    let _ = result_tx.send(SttResult::Recording);

    // Record from mic via arecord
    let wav_path = PathBuf::from(RECORD_PATH);
    info!("Recording {RECORD_SECONDS}s from {CAPTURE_DEVICE}...");

    let status = Command::new("arecord")
        .args([
            "-D",
            CAPTURE_DEVICE,
            "-f",
            "S16_LE",
            "-r",
            "16000",
            "-c",
            "1",
            "-d",
            &RECORD_SECONDS.to_string(),
            wav_path.to_str().unwrap(),
        ])
        .output();

    match status {
        Ok(output) if output.status.success() => {
            info!("Recorded to {}", wav_path.display());
        }
        Ok(output) => {
            let stderr = String::from_utf8_lossy(&output.stderr);
            let msg = format!("arecord failed: {stderr}");
            error!("{msg}");
            let _ = result_tx.send(SttResult::Error(msg));
            return;
        }
        Err(e) => {
            let msg = format!("arecord error: {e}");
            error!("{msg}");
            let _ = result_tx.send(SttResult::Error(msg));
            return;
        }
    }

    // Signal that we're processing
    let _ = result_tx.send(SttResult::Processing);

    // Transcribe
    let infer_start = Instant::now();
    match svs.infer_file(&wav_path) {
        Ok(results) => {
            let infer_time = infer_start.elapsed();
            info!(
                "STT inference: {:.3}s, {} segments",
                infer_time.as_secs_f32(),
                results.len()
            );

            // Concatenate all segment text
            let text: String = results
                .iter()
                .map(|seg| {
                    info!(
                        "  segment: lang={:?} emotion={:?} text={}",
                        seg.language, seg.emotion, seg.content
                    );
                    seg.content.trim().to_string()
                })
                .collect::<Vec<_>>()
                .join(" ");

            if text.is_empty() {
                let _ = result_tx.send(SttResult::Error("No speech detected".into()));
            } else {
                info!("Transcribed: {text}");
                let _ = result_tx.send(SttResult::Transcribed(text));
            }
        }
        Err(e) => {
            let msg = format!("STT inference failed: {e}");
            error!("{msg}");
            let _ = result_tx.send(SttResult::Error(msg));
        }
    }
}
