//! LLM inference benchmark for the Rock 5A.
//!
//! Loads a GGUF model, runs a short meditation prompt, and reports:
//! - Time to first token
//! - Total tokens generated
//! - Tokens per second
//! - Total inference time
//!
//! Usage: `bench_llm <model_path>`
//!
//! This benchmark tests both llama-cpp-2 and llama-gguf to compare
//! performance on aarch64.

use std::env;
use std::time::Instant;

fn main() {
    let args: Vec<String> = env::args().collect();
    let model_path = args.get(1).expect("usage: bench_llm <model_path>");

    println!("=== LLM Benchmark ===");
    println!("Model: {model_path}");
    println!();

    // TODO: implement actual model loading and inference once we know
    // which crate builds successfully on aarch64.
    //
    // The benchmark will:
    // 1. Load the GGUF model
    // 2. Set inference params (top_k=40, top_p=0.95, temp=0.25)
    // 3. Run the meditation system prompt + "Guide me in a breathing meditation"
    // 4. Stream tokens, measuring time to first token and tokens/sec
    // 5. Stop after 100 tokens or generation complete

    let prompt =
        "You are a meditation guide. Generate a short breathing meditation with [N] pause markers.";
    println!("Prompt: {prompt}");
    println!();

    // Placeholder until crate builds are verified
    let start = Instant::now();

    println!("Model loading... (not yet implemented)");
    println!("Waiting for llama-cpp-2 or llama-gguf build to succeed.");
    println!();
    println!("Time elapsed: {:.2}s", start.elapsed().as_secs_f32());
    println!();
    println!("Next steps:");
    println!("  1. Verify crate builds on Rock (cargo check)");
    println!("  2. Download a small GGUF model for testing");
    println!("  3. Implement model loading and token streaming");
}
