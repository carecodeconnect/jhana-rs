//! Test sensevoice-rs STT with RKNN backend on Rock 5A.
//!
//! Usage: `test_stt [wav_file]`
//!
//! If no WAV file is provided, records 5 seconds from the Uctronics mic
//! via `arecord` and transcribes that.
//!
//! Models are loaded from `/home/ubuntu/models/sensevoice/` on the Rock.
//!
//! Download the RKNN model from HuggingFace:
//! ```sh
//! cd /home/ubuntu/models/sensevoice
//! wget https://huggingface.co/happyme531/SenseVoiceSmall-RKNN2/resolve/main/sense-voice-encoder.rknn
//! wget https://huggingface.co/happyme531/SenseVoiceSmall-RKNN2/resolve/main/chn_jpn_yue_eng_ko_spectok.bpe.model
//! wget https://huggingface.co/happyme531/SenseVoiceSmall-RKNN2/resolve/main/am.mvn
//! wget https://huggingface.co/happyme531/SenseVoiceSmall-RKNN2/resolve/main/embedding.npy
//! wget https://huggingface.co/happyme531/SenseVoiceSmall-RKNN2/resolve/main/fsmn-am.mvn
//! wget https://huggingface.co/happyme531/SenseVoiceSmall-RKNN2/resolve/main/fsmn-config.yaml
//! wget https://huggingface.co/happyme531/SenseVoiceSmall-RKNN2/resolve/main/fsmnvad-offline.onnx
//! ```

use std::path::PathBuf;
use std::process::Command;
use std::time::Instant;

use sensevoice_rs::SenseVoiceSmall;
use sensevoice_rs::silero_vad::VadConfig;

/// Model directory on the Rock 5A.
const MODEL_DIR: &str = "/home/ubuntu/models/sensevoice";

fn main() -> Result<(), Box<dyn std::error::Error>> {
    let wav_path = match std::env::args().nth(1) {
        Some(path) => PathBuf::from(path),
        None => {
            // Record 5 seconds from mic
            let path = PathBuf::from("/tmp/test_stt.wav");
            println!("No WAV file provided — recording 5s from mic...");
            // S32_LE @ 48 kHz on plughw:1,0 — see src/stt.rs and
            // docs/09_AUDIO.md for why these are required on the
            // Uctronics I2S mic.
            let status = Command::new("arecord")
                .args([
                    "-D",
                    "plughw:1,0",
                    "-f",
                    "S32_LE",
                    "-r",
                    "48000",
                    "-c",
                    "1",
                    "-d",
                    "5",
                    path.to_str().unwrap(),
                ])
                .status()?;
            if !status.success() {
                eprintln!("arecord failed with {status}");
                std::process::exit(1);
            }
            println!("Recorded to {}", path.display());
            path
        }
    };

    println!("=== sensevoice-rs STT test ===");
    println!("WAV: {}", wav_path.display());
    println!("Model dir: {MODEL_DIR}");
    println!();

    // Try init_with_config for local model paths first.
    // Falls back to init() which uses hf-hub auto-download.
    let load_start = Instant::now();

    // init() auto-downloads from HuggingFace and handles RKNN model
    // loading including embeddings. init_with_config() doesn't support
    // RKNN fully yet (missing embedding path).
    //
    // To use local models without re-downloading, set HF_HOME to a cache
    // dir that already has the model, or let it download once.
    let model_path = PathBuf::from(MODEL_DIR);
    if model_path.exists() {
        println!("Local model dir exists at {MODEL_DIR}");
        println!("Files: ");
        if let Ok(entries) = std::fs::read_dir(&model_path) {
            for entry in entries.flatten() {
                println!("  {}", entry.path().display());
            }
        }
    }

    println!();
    println!("Initializing SenseVoiceSmall (hf-hub will download/cache model)...");
    let svs = SenseVoiceSmall::init(VadConfig::default())?;

    println!("Model loaded in {:.2}s", load_start.elapsed().as_secs_f32());
    println!();

    // Run inference
    let infer_start = Instant::now();
    let results = svs.infer_file(&wav_path)?;
    let infer_time = infer_start.elapsed();

    println!("Inference completed in {:.3}s", infer_time.as_secs_f32());
    println!("Segments: {}", results.len());
    println!();

    for (i, seg) in results.iter().enumerate() {
        println!("--- Segment {i} ---");
        println!("  Language: {:?}", seg.language);
        println!("  Emotion:  {:?}", seg.emotion);
        println!("  Event:    {:?}", seg.event);
        println!("  Text:     {}", seg.content);
        println!();
    }

    svs.destroy()?;
    println!("Done.");
    Ok(())
}
