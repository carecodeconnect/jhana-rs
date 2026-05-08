//! Test rkllm-rs LLM inference on Rock 5A NPU.
//!
//! Usage: `test_rkllm [model_path]`
//!
//! Loads a `.rkllm` model via rkllm-rs, sends a meditation prompt, and
//! streams tokens from the NPU. Prints each token as it arrives with
//! timing stats at the end.
//!
//! Default model path: `/home/ubuntu/models/Llama-3.2-3B-Instruct_w8a8_g128_rk3588.rkllm`
//!
//! ## Setup on Rock
//!
//! 1. Install librkllmrt.so v1.2.3:
//!    ```sh
//!    wget -O /tmp/librkllmrt.so \
//!      "https://raw.githubusercontent.com/airockchip/rknn-llm/release-v1.2.3/rkllm-runtime/Linux/librkllm_api/aarch64/librkllmrt.so"
//!    sudo cp /tmp/librkllmrt.so /usr/lib/librkllmrt.so
//!    sudo ldconfig
//!    ```
//!
//! 2. Download a .rkllm model from HuggingFace:
//!    ```sh
//!    cd /home/ubuntu/models
//!    wget -O Llama-3.2-3B-Instruct_w8a8_g128_rk3588.rkllm \
//!      "https://huggingface.co/jamescallander/Llama-3.2-3B-Instruct_w8a8_g128_rk3588.rkllm/resolve/main/Llama-3.2-3B-Instruct_w8a8_g128_rk3588.rkllm"
//!    ```
//!    See `docs/05_NPU.md` for alternative models.

use std::sync::mpsc;
use std::time::Instant;

use rkllm_rs::prelude::*;

/// Default model path on the Rock 5A.
const DEFAULT_MODEL: &str = "/home/ubuntu/models/Llama-3.2-3B-Instruct_w8a8_g128_rk3588.rkllm";

/// System prompt for meditation generation.
const SYSTEM_PROMPT: &str = "\
You are a calm meditation guide. Generate a short guided meditation \
(3-5 sentences) with [N] pause markers where N is seconds of silence. \
Example: Close your eyes. [3] Take a deep breath in. [5] Now slowly exhale.";

/// User prompt for the test.
const USER_PROMPT: &str = "Guide me through a simple breathing meditation.";

/// Max tokens to generate.
const MAX_TOKENS: i32 = 256;

/// Callback handler that sends streaming tokens through an mpsc channel.
///
/// This mirrors the pattern used in the main TUI: the callback runs on
/// the rkllm-rs inference thread and sends tokens to a receiver that
/// can be polled from the main thread.
struct TokenSender {
    tx: mpsc::Sender<CallbackEvent>,
    token_count: u32,
}

/// Events sent from the callback to the main thread.
enum CallbackEvent {
    Token(String),
    Done,
    Error,
}

impl RkllmCallbackHandler for TokenSender {
    fn handle(&mut self, result: Option<RKLLMResult<'_>>, state: LLMCallState) {
        match state {
            LLMCallState::Normal => {
                if let Some(r) = result {
                    self.token_count += 1;
                    let _ = self.tx.send(CallbackEvent::Token(r.text.to_string()));
                }
            }
            LLMCallState::Waiting => {
                // Waiting for complete UTF-8 character, skip
            }
            LLMCallState::Finish => {
                let _ = self.tx.send(CallbackEvent::Done);
            }
            LLMCallState::Error => {
                eprintln!("ERROR: rkllm callback reported error");
                let _ = self.tx.send(CallbackEvent::Error);
            }
            LLMCallState::GetLastHiddenLayer => {}
        }
    }
}

fn main() -> Result<(), Box<dyn std::error::Error + Send + Sync>> {
    let model_path = std::env::args()
        .nth(1)
        .unwrap_or_else(|| DEFAULT_MODEL.to_string());

    println!("=== rkllm-rs NPU LLM test ===");
    println!("Model: {model_path}");
    println!("Max tokens: {MAX_TOKENS}");
    println!();

    // Load model
    println!("Loading model (this may take 30-60s on first load)...");
    let load_start = Instant::now();

    let mut config = LLMConfig::with_model_path(&model_path);
    config.max_new_tokens = MAX_TOKENS;
    config.temperature = 0.7;
    config.top_p = 0.9;
    config.top_k = 40;
    config.repeat_penalty = 1.1;
    config.frequency_penalty = 0.0;
    config.presence_penalty = 0.0;

    let handle = init(config)?;
    let load_time = load_start.elapsed();
    println!("Model loaded in {:.2}s", load_time.as_secs_f32());
    println!();

    // Build prompt with Llama 3 chat template.
    // rkllm-rs models may have a built-in chat template, but we format
    // explicitly to ensure correct results across models.
    let prompt = format!(
        "<|begin_of_text|><|start_header_id|>system<|end_header_id|>\n\n\
         {SYSTEM_PROMPT}<|eot_id|><|start_header_id|>user<|end_header_id|>\n\n\
         {USER_PROMPT}<|eot_id|><|start_header_id|>assistant<|end_header_id|>\n\n"
    );

    println!("System: {SYSTEM_PROMPT}");
    println!("User: {USER_PROMPT}");
    println!();
    println!("--- Generated meditation ---");

    // Set up token streaming channel
    let (tx, rx) = mpsc::channel();
    let handler = TokenSender { tx, token_count: 0 };

    // Run inference
    let gen_start = Instant::now();
    let infer_params = RKLLMInferParam::default();

    // Spawn inference — handle.run() blocks until complete, the callback
    // streams tokens through the channel as they are generated
    let input = RKLLMInput::prompt(&prompt);

    // Run in a thread so we can print tokens as they arrive
    let handle_thread = std::thread::spawn(move || handle.run(input, Some(infer_params), handler));

    // Print tokens as they stream in
    let mut total_tokens = 0u32;
    let mut first_token_time = None;

    loop {
        match rx.recv() {
            Ok(CallbackEvent::Token(text)) => {
                total_tokens += 1;
                if first_token_time.is_none() {
                    first_token_time = Some(gen_start.elapsed());
                }
                print!("{text}");
                // Flush so tokens appear immediately
                use std::io::Write;
                std::io::stdout().flush().ok();
            }
            Ok(CallbackEvent::Done) => {
                println!();
                println!();
                println!("--- Stats ---");
                let gen_time = gen_start.elapsed();
                let tok_per_sec = if gen_time.as_secs_f32() > 0.0 {
                    f64::from(total_tokens) / gen_time.as_secs_f64()
                } else {
                    0.0
                };
                println!("Load time:         {:.2}s", load_time.as_secs_f32());
                if let Some(ft) = first_token_time {
                    println!("First token:       {:.3}s", ft.as_secs_f32());
                }
                println!("Total tokens:      {total_tokens}");
                println!("Generation time:   {:.2}s", gen_time.as_secs_f32());
                println!("Speed:             {tok_per_sec:.2} tok/s");
                break;
            }
            Ok(CallbackEvent::Error) => {
                eprintln!("\nInference error after {total_tokens} tokens");
                break;
            }
            Err(_) => {
                eprintln!("\nChannel closed unexpectedly");
                break;
            }
        }
    }

    // Wait for the inference thread to finish
    match handle_thread.join() {
        Ok(Ok(())) => {}
        Ok(Err(e)) => eprintln!("Inference error: {e}"),
        Err(_) => eprintln!("Inference thread panicked"),
    }

    println!("Done.");
    Ok(())
}
