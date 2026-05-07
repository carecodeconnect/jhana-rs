//! Test sherpa-onnx VITS TTS with Piper model on Rock 5A.
//!
//! Usage: `test_tts`
//!
//! Expects model files at /home/ubuntu/models/ and espeak-ng-data
//! at /usr/local/lib/python3.10/dist-packages/piper_phonemize/espeak-ng-data

use std::time::Instant;

fn main() {
    let base = "/home/ubuntu/models/vits-piper-en_US-lessac-medium";
    let model = &format!("{base}/en_US-lessac-medium.onnx");
    let tokens = &format!("{base}/tokens.txt");
    let data_dir = &format!("{base}/espeak-ng-data");
    let text = "Close your eyes and take a deep breath in.";

    println!("=== sherpa-onnx VITS TTS test ===");
    println!("Model: {model}");
    println!("Text: {text}");
    println!();

    let config = sherpa_onnx::OfflineTtsConfig {
        model: sherpa_onnx::OfflineTtsModelConfig {
            vits: sherpa_onnx::OfflineTtsVitsModelConfig {
                model: Some(model.into()),
                tokens: Some(tokens.into()),
                data_dir: Some(data_dir.into()),
                length_scale: 1.3,
                ..Default::default()
            },
            ..Default::default()
        },
        ..Default::default()
    };

    let load_start = Instant::now();
    let tts = sherpa_onnx::OfflineTts::create(&config).expect("failed to create TTS");
    println!(
        "TTS loaded in {:.2}s (sample_rate={}, speakers={})",
        load_start.elapsed().as_secs_f32(),
        tts.sample_rate(),
        tts.num_speakers(),
    );

    let gen_config = sherpa_onnx::GenerationConfig::default();
    let synth_start = Instant::now();
    let audio = tts
        .generate_with_config(text, &gen_config, None::<fn(&[f32], f32) -> bool>)
        .expect("failed to synthesize");

    let synth_time = synth_start.elapsed();
    #[expect(clippy::cast_precision_loss)]
    let duration = audio.samples().len() as f32 / audio.sample_rate() as f32;

    println!("Synthesized in {:.2}s", synth_time.as_secs_f32());
    println!("Audio: {:.2}s at {} Hz", duration, audio.sample_rate());

    // Save WAV
    let ok = audio.save("/tmp/sherpa_test.wav");
    if ok {
        println!("Saved to /tmp/sherpa_test.wav");
    } else {
        eprintln!("Failed to save WAV");
    }
}
