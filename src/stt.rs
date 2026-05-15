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
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::mpsc::{Receiver, Sender};
use std::time::Instant;

use log::{error, info};
use sensevoice_rs::SenseVoiceSmall;
use sensevoice_rs::silero_vad::VadConfig;

/// Capture via the ALSA→PulseAudio plugin. PA owns the Uctronics card
/// in system mode (see docs/09_AUDIO.md), so `plughw:1,0` returns
/// "Device or resource busy". `pulse` routes through PA's default
/// source, which our /etc/pulse/system.pa points at
/// alsa_input.platform-uctronics-sound.stereo-fallback.
const CAPTURE_DEVICE: &str = "pulse";

/// Capture format. **Must be S32_LE on this hardware.** The Uctronics I2S
/// MEMS mic delivers ~24-bit samples in a 32-bit word; capturing as S16_LE
/// reads the low/noise bits and yields DC-offset garbage even though
/// `arecord` itself succeeds. See `docs/09_AUDIO.md` "Mic capture
/// format/rate" for the analysis.
const CAPTURE_FORMAT: &str = "S32_LE";

/// Capture sample rate. The codec's I2S TDM controller is clocked for
/// 48 kHz; lower rates rely on the ALSA plug layer to resample and
/// produced poor results in testing.
const CAPTURE_RATE: u32 = 48_000;

/// Recording duration in seconds.
const RECORD_SECONDS: u32 = 5;

/// Temporary WAV file for native-format mic capture (S32_LE 48 kHz).
const RECORD_PATH: &str = "/tmp/jhana_stt.wav";

/// Resampled WAV passed to SenseVoice (S16_LE 16 kHz mono — the model
/// only accepts 8 or 16 kHz).
const RESAMPLED_PATH: &str = "/tmp/jhana_stt_16k.wav";

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
/// Set to `true` once SenseVoice has finished loading. The TUI's
/// main loop watches this together with [`crate::llm::LLM_READY`]
/// to decide when to play the welcome speech — speaking before the
/// models are ready means the user can press ENTER and then sit
/// staring at a frozen screen for tens of seconds while loads
/// finish, so we hold the greeting until the device is actually
/// ready to respond.
pub static STT_READY: AtomicBool = AtomicBool::new(false);

/// Pre-rendered "Speak now." prompt played at the start of each listen.
const CUE_WAV: &str = "/tmp/jhana_speak_now.wav";

/// Render the "Speak now." cue once at thread startup using the
/// configured TTS engine. If paroli is configured we use it (matches
/// the welcome / meditation voice). Falls back to espeak-ng on any
/// failure so the cue always exists by the time the user presses ENTER.
fn render_cue() {
    let phrase = "Speak now.";
    let tts = &crate::config::get().tts;

    if tts.engine == "paroli"
        && let Some(p) = &tts.paroli
    {
        let result = Command::new(&p.bin)
            .env("LD_LIBRARY_PATH", &p.ld_library_path)
            .args([
                "--encoder",
                &p.encoder,
                "--decoder",
                &p.decoder,
                "-c",
                &p.config,
                "--espeak_data",
                &p.espeak_data,
                "--length_scale",
                &p.length_scale.to_string(),
                "--output_file",
                CUE_WAV,
                "--quiet",
            ])
            .stdin(std::process::Stdio::piped())
            .stdout(std::process::Stdio::null())
            .stderr(std::process::Stdio::piped())
            .spawn()
            .and_then(|mut child| {
                use std::io::Write;
                if let Some(stdin) = child.stdin.as_mut() {
                    let _ = stdin.write_all(phrase.as_bytes());
                    let _ = stdin.write_all(b"\n");
                }
                child.wait_with_output()
            });
        if matches!(&result, Ok(out) if out.status.success())
            && std::fs::metadata(CUE_WAV).map(|m| m.len() > 44).unwrap_or(false)
        {
            info!("STT cue rendered via paroli at {CUE_WAV}");
            return;
        }
        error!("STT cue paroli render failed; falling back to espeak-ng");
    }

    let ok = Command::new("espeak-ng")
        .args(["-a", "100", "-s", "145", "-w", CUE_WAV, phrase])
        .status()
        .map(|s| s.success())
        .unwrap_or(false);
    if ok {
        info!("STT cue rendered via espeak-ng at {CUE_WAV}");
    } else {
        error!("STT cue render failed; pressing ENTER will give no audible cue");
    }
}

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
    STT_READY.store(true, Ordering::Release);

    // Pre-render the 'Speak now.' cue via the configured TTS engine so
    // each press just hits paplay (cheap). Falls back to espeak-ng if
    // paroli isn't configured / fails. Cached at /tmp/jhana_speak_now.wav.
    render_cue();

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

    // Audible "Speak now" cue (pre-rendered at thread startup via
    // render_cue() — paroli where available, espeak-ng otherwise).
    let _ = Command::new("paplay")
        .env("PULSE_SERVER", "unix:/var/run/pulse/native")
        .args([
            "--device",
            "alsa_output.platform-uctronics-sound.stereo-fallback",
            CUE_WAV,
        ])
        .status();

    // Record from mic via arecord
    let wav_path = PathBuf::from(RECORD_PATH);
    info!("Recording {RECORD_SECONDS}s from {CAPTURE_DEVICE}...");

    let status = Command::new("arecord")
        .args([
            "-D",
            CAPTURE_DEVICE,
            "-f",
            CAPTURE_FORMAT,
            "-r",
            &CAPTURE_RATE.to_string(),
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

    // Resample to S16_LE 16 kHz mono for SenseVoice (it rejects anything
    // other than 8 or 16 kHz). We capture at S32_LE 48 kHz natively because
    // that's the only format the Uctronics I2S codec returns usable data in
    // (S16_LE direct from plughw gives DC-offset garbage — see docs/09_AUDIO.md).
    let resampled_path = PathBuf::from(RESAMPLED_PATH);
    let ffmpeg_status = Command::new("ffmpeg")
        .args([
            "-y",
            "-hide_banner",
            "-loglevel",
            "error",
            "-i",
            wav_path.to_str().unwrap(),
            "-ac",
            "1",
            "-ar",
            "16000",
            "-sample_fmt",
            "s16",
            resampled_path.to_str().unwrap(),
        ])
        .status();
    match ffmpeg_status {
        Ok(s) if s.success() => {}
        Ok(s) => {
            let msg = format!("ffmpeg resample failed: {s}");
            error!("{msg}");
            let _ = result_tx.send(SttResult::Error(msg));
            return;
        }
        Err(e) => {
            let msg = format!("ffmpeg error: {e}");
            error!("{msg}");
            let _ = result_tx.send(SttResult::Error(msg));
            return;
        }
    }

    // Signal that we're processing
    let _ = result_tx.send(SttResult::Processing);

    // Transcribe
    let infer_start = Instant::now();
    match svs.infer_file(&resampled_path) {
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
