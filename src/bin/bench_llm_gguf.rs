//! LLM inference benchmark using llama-gguf (pure Rust).
//!
//! Companion to `bench_llm` (llama-cpp-2). Runs the same prompt on the same
//! model to compare tokens/sec between the C++ and pure Rust implementations.
//!
//! Usage: `bench_llm_gguf <model_path> [max_tokens]`

use std::env;
use std::io::Write;
use std::sync::Arc;
use std::time::Instant;

use llama_gguf::Model as _; // bring forward() into scope
use llama_gguf::backend::cpu::CpuBackend;
use llama_gguf::gguf::GgufFile;
use llama_gguf::model::{InferenceContext, load_llama_model};
use llama_gguf::sampling::{Sampler, SamplerConfig};
use llama_gguf::tokenizer::Tokenizer;

#[allow(clippy::cast_precision_loss)]
fn main() -> Result<(), Box<dyn std::error::Error>> {
    let args: Vec<String> = env::args().collect();
    let model_path = args
        .get(1)
        .expect("usage: bench_llm_gguf <model_path> [max_tokens]");
    let max_tokens: usize = args.get(2).and_then(|s| s.parse().ok()).unwrap_or(100);

    println!("=== LLM Benchmark (llama-gguf / pure Rust) ===");
    println!("Model: {model_path}");
    println!("Max tokens: {max_tokens}");
    println!();

    // Load model
    let load_start = Instant::now();
    let model = load_llama_model(model_path)?;
    let gguf = GgufFile::open(model_path)?;
    let tokenizer = Tokenizer::from_gguf(&gguf)?;
    let load_time = load_start.elapsed();
    println!("Model loaded in {:.2}s", load_time.as_secs_f32());

    // Set up inference
    let backend: Arc<dyn llama_gguf::Backend> = Arc::new(CpuBackend::new());
    let mut ctx = InferenceContext::new(model.config(), backend);
    let vocab_size = model.config().vocab_size;
    let mut sampler = Sampler::new(
        SamplerConfig {
            temperature: 0.25,
            top_k: 40,
            top_p: 0.95,
            ..SamplerConfig::default()
        },
        vocab_size,
    );

    // Tokenize prompt (same as bench_llm for fair comparison)
    let prompt = "You are a meditation guide. Generate calming guided meditation \
                  instructions with [N] pause markers. Keep sentences short.\n\n\
                  Close your eyes and take a deep breath in.";
    let tokens = tokenizer.encode(prompt, true)?;
    println!("Prompt tokens: {}", tokens.len());
    println!();

    // Generate tokens
    println!("--- Generation ---");
    let gen_start = Instant::now();
    let mut first_token_time = None;
    let mut generated = 0usize;
    let mut output_text = String::new();
    let mut output_tokens = tokens.clone();

    for _ in 0..max_tokens {
        let logits = model.forward(&output_tokens[output_tokens.len() - 1..], &mut ctx)?;
        let next_token = sampler.sample(&logits, &output_tokens);
        output_tokens.push(next_token);

        // Time to first token
        if first_token_time.is_none() {
            first_token_time = Some(gen_start.elapsed());
        }

        // Decode token to text
        if let Ok(text) = tokenizer.decode(&[next_token]) {
            print!("{text}");
            std::io::stdout().flush().ok();
            output_text.push_str(&text);
        }
        generated += 1;
    }

    let gen_time = gen_start.elapsed();
    println!();
    println!();

    // Report
    println!("=== Results ===");
    println!(
        "Time to first token: {:.0}ms",
        first_token_time.map_or(0.0, |t| t.as_secs_f64() * 1000.0)
    );
    println!("Tokens generated: {generated}");
    println!("Generation time: {:.2}s", gen_time.as_secs_f32());
    if gen_time.as_secs_f32() > 0.0 {
        println!(
            "Tokens/sec: {:.1}",
            generated as f64 / gen_time.as_secs_f64()
        );
    }
    println!("Model load time: {:.2}s", load_time.as_secs_f32());
    println!();
    println!("Output length: {} chars", output_text.len());

    Ok(())
}
