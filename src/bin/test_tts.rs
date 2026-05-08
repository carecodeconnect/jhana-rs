//! Test Piper CLI TTS on Rock 5A.
//!
//! Usage: `test_tts`
//!
//! Synthesizes a test sentence via Piper CLI and measures latency.

use std::io::Write;
use std::process::{Command, Stdio};
use std::time::Instant;

fn main() {
    let model = "/home/ubuntu/models/vits-piper-en_US-lessac-medium/en_US-lessac-medium.onnx";
    let text = "Close your eyes and take a deep breath in.";
    let wav_path = "/tmp/piper_test.wav";
    let length_scale = 1.3;

    println!("=== Piper CLI TTS test ===");
    println!("Model: {model}");
    println!("Text: {text}");
    println!("Length scale: {length_scale}");
    println!();

    for i in 1..=3 {
        let start = Instant::now();

        let mut child = Command::new("/usr/local/bin/piper")
            .args([
                "--model",
                model,
                "--output_file",
                wav_path,
                "--length_scale",
                &length_scale.to_string(),
            ])
            .stdin(Stdio::piped())
            .stdout(Stdio::null())
            .stderr(Stdio::piped())
            .spawn()
            .expect("failed to start piper");

        if let Some(ref mut stdin) = child.stdin {
            stdin
                .write_all(text.as_bytes())
                .expect("failed to write to piper stdin");
        }

        let output = child.wait_with_output().expect("failed to wait for piper");
        let synth_time = start.elapsed();

        if output.status.success() {
            println!(
                "[run {i}] synth={:.2}s — saved to {wav_path}",
                synth_time.as_secs_f32(),
            );
        } else {
            let stderr = String::from_utf8_lossy(&output.stderr);
            eprintln!("[run {i}] piper failed: {stderr}");
        }
    }

    println!();
    println!("Play with: aplay -D plughw:0,0 {wav_path}");
}
