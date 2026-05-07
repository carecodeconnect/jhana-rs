//! Text-to-speech via sherpa-onnx VITS (Piper model).
//!
//! Runs TTS on a background thread, receiving sentences via an mpsc channel.
//! Each sentence is synthesized to a WAV buffer and played via `aplay`.
//! Uses the same thread + mpsc pattern as GPIO and LLM streaming.
//!
//! # Why aplay for playback?
//!
//! The Rock 5A's audio hardware requires `plughw:` for channel conversion
//! (Piper outputs mono, some devices need stereo). Using `aplay` via
//! `std::process::Command` is the simplest approach and avoids pulling in
//! rodio/cpal as dependencies. The TTS thread blocks on each `aplay` call,
//! which naturally sequences sentences without overlap.

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

/// Path to the sherpa-onnx Piper model directory on the Rock.
const MODEL_DIR: &str = "/home/ubuntu/models/vits-piper-en_US-lessac-medium";

/// ALSA playback device (Uctronics onboard speaker with plughw for mono->stereo).
const PLAYBACK_DEVICE: &str = "plughw:2,0";

/// Speech rate — higher values produce slower, calmer speech.
/// 1.0 = normal, 1.3 = meditation pace, 2.0 = very slow.
const LENGTH_SCALE: f32 = 1.3;

/// Start the TTS background thread.
///
/// Returns a `Sender<TtsCommand>` for sending sentences to be spoken.
/// The thread loads the sherpa-onnx model once and reuses it for all
/// subsequent sentences. If the model fails to load, sentences are
/// logged but not spoken (graceful degradation).
pub fn start() -> Sender<TtsCommand> {
    let (tx, rx) = std::sync::mpsc::channel::<TtsCommand>();

    std::thread::Builder::new()
        .name("tts".into())
        .spawn(move || {
            info!("TTS thread started");
            tts_loop(&rx);
            info!("TTS thread exiting");
        })
        .expect("failed to spawn TTS thread");

    tx
}

/// TTS event loop — loads model, then processes sentences until channel closes.
fn tts_loop(rx: &Receiver<TtsCommand>) {
    let model_path = format!("{MODEL_DIR}/en_US-lessac-medium.onnx");
    let tokens_path = format!("{MODEL_DIR}/tokens.txt");
    let data_dir = format!("{MODEL_DIR}/espeak-ng-data");

    let config = sherpa_onnx::OfflineTtsConfig {
        model: sherpa_onnx::OfflineTtsModelConfig {
            vits: sherpa_onnx::OfflineTtsVitsModelConfig {
                model: Some(model_path.clone()),
                tokens: Some(tokens_path),
                data_dir: Some(data_dir),
                length_scale: LENGTH_SCALE,
                ..Default::default()
            },
            ..Default::default()
        },
        ..Default::default()
    };

    let Some(tts) = sherpa_onnx::OfflineTts::create(&config) else {
        error!("Failed to load TTS model from {model_path}");
        // Drain channel silently — TUI still works, just no audio
        while let Ok(cmd) = rx.recv() {
            if let TtsCommand::Speak(s) = cmd {
                info!("TTS unavailable, skipping: {s}");
            }
        }
        return;
    };

    info!(
        "TTS model loaded (sample_rate={}, speakers={})",
        tts.sample_rate(),
        tts.num_speakers()
    );

    let gen_config = sherpa_onnx::GenerationConfig::default();

    while let Ok(cmd) = rx.recv() {
        match cmd {
            TtsCommand::Speak(sentence) => {
                speak_sentence(&tts, &gen_config, &sentence);
            }
            TtsCommand::Stop => {
                info!("TTS stop requested");
                // Drain any queued messages
                while rx.try_recv().is_ok() {}
            }
        }
    }
}

/// Synthesize a sentence and play it via aplay.
fn speak_sentence(
    tts: &sherpa_onnx::OfflineTts,
    gen_config: &sherpa_onnx::GenerationConfig,
    sentence: &str,
) {
    let start = std::time::Instant::now();

    let Some(audio) =
        tts.generate_with_config(sentence, gen_config, None::<fn(&[f32], f32) -> bool>)
    else {
        error!("TTS synthesis failed for: {sentence}");
        return;
    };

    let synth_time = start.elapsed();
    #[expect(clippy::cast_precision_loss)] // sample counts are small enough
    let duration = audio.samples().len() as f32 / audio.sample_rate() as f32;
    info!(
        "TTS: {:.2}s synth, {:.2}s audio — {}",
        synth_time.as_secs_f32(),
        duration,
        &sentence[..sentence.len().min(50)]
    );

    // Save to temp WAV and play via aplay
    let wav_path = "/tmp/jhana_tts.wav";
    if !audio.save(wav_path) {
        error!("Failed to save TTS WAV to {wav_path}");
        return;
    }

    match Command::new("aplay")
        .args(["-D", PLAYBACK_DEVICE, wav_path])
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
