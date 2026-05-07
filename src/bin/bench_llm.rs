//! LLM inference benchmark for the Rock 5A.
//!
//! Loads a GGUF model via llama-cpp-2, runs a meditation prompt, and reports:
//! - Time to first token
//! - Total tokens generated
//! - Tokens per second
//! - Total inference time
//!
//! Usage: `bench_llm <model_path> [max_tokens]`

use std::env;
use std::io::Write;
use std::num::NonZeroU32;
use std::time::Instant;

use llama_cpp_2::context::params::LlamaContextParams;
use llama_cpp_2::llama_backend::LlamaBackend;
use llama_cpp_2::llama_batch::LlamaBatch;
use llama_cpp_2::model::params::LlamaModelParams;
use llama_cpp_2::model::{AddBos, LlamaModel};
use llama_cpp_2::sampling::LlamaSampler;

#[allow(clippy::cast_possible_truncation, clippy::cast_possible_wrap)]
fn main() {
    let args: Vec<String> = env::args().collect();
    let model_path = args
        .get(1)
        .expect("usage: bench_llm <model_path> [max_tokens]");
    let max_tokens: u32 = args.get(2).and_then(|s| s.parse().ok()).unwrap_or(100);

    println!("=== LLM Benchmark (llama-cpp-2) ===");
    println!("Model: {model_path}");
    println!("Max tokens: {max_tokens}");
    println!();

    // Initialize backend
    let backend = LlamaBackend::init().expect("failed to init llama backend");

    // Load model
    let load_start = Instant::now();
    let model_params = LlamaModelParams::default();
    let model = LlamaModel::load_from_file(&backend, model_path, &model_params)
        .expect("failed to load model");
    let load_time = load_start.elapsed();
    println!("Model loaded in {:.2}s", load_time.as_secs_f32());

    // Create context
    let ctx_size = NonZeroU32::new(2048).unwrap();
    let ctx_params = LlamaContextParams::default().with_n_ctx(Some(ctx_size));
    let mut ctx = model
        .new_context(&backend, ctx_params)
        .expect("failed to create context");

    // Tokenize prompt
    let prompt = "You are a meditation guide. Generate calming guided meditation \
                  instructions with [N] pause markers. Keep sentences short.\n\n\
                  Close your eyes and take a deep breath in.";
    let tokens = model
        .str_to_token(prompt, AddBos::Always)
        .expect("failed to tokenize");
    println!("Prompt tokens: {}", tokens.len());
    println!();

    // Set up batch and sampler
    let mut batch = LlamaBatch::new(2048, 1);
    let mut sampler = LlamaSampler::chain_simple([
        LlamaSampler::top_k(40),
        LlamaSampler::top_p(0.95, 1),
        LlamaSampler::temp(0.25),
        LlamaSampler::dist(42),
    ]);

    // Feed prompt tokens
    let last_idx = (tokens.len() - 1) as i32;
    for (i, &token) in tokens.iter().enumerate() {
        let is_last = i as i32 == last_idx;
        batch
            .add(token, i as i32, &[0], is_last)
            .expect("failed to add token");
    }
    ctx.decode(&mut batch).expect("failed to decode prompt");

    // Generate tokens
    println!("--- Generation ---");
    let gen_start = Instant::now();
    let mut first_token_time = None;
    let mut generated = 0u32;
    let mut output_text = String::new();

    // Decoder for converting tokens back to UTF-8 text
    let mut decoder = encoding_rs::UTF_8.new_decoder();

    for _ in 0..max_tokens {
        let token = sampler.sample(&ctx, -1);
        sampler.accept(token);

        // Check for end of generation
        if model.is_eog_token(token) {
            break;
        }

        // Time to first token
        if first_token_time.is_none() {
            first_token_time = Some(gen_start.elapsed());
        }

        // Decode token to text
        let piece = model
            .token_to_piece(token, &mut decoder, false, None)
            .unwrap_or_default();
        print!("{piece}");
        std::io::stdout().flush().ok();
        output_text.push_str(&piece);
        generated += 1;

        // Prepare next batch
        batch.clear();
        let pos = tokens.len() as i32 + generated as i32 - 1;
        batch
            .add(token, pos, &[0], true)
            .expect("failed to add token");
        ctx.decode(&mut batch).expect("failed to decode");
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
            f64::from(generated) / gen_time.as_secs_f64()
        );
    }
    println!("Model load time: {:.2}s", load_time.as_secs_f32());
    println!();
    println!("Output length: {} chars", output_text.len());
}
