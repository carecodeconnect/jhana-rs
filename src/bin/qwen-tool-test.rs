//! Phase 1 go/no-go: does Qwen3-1.7B (w8a8_g128) on the RK3588 NPU emit
//! structured tool calls when given a Qwen3-templated prompt with a tool
//! definition?
//!
//! Loads the model, sends one prompt asking it to ring a bell with a
//! `ring_bell` tool defined, dumps the raw output, then extracts and
//! validates any `<tool_call>{...}</tool_call>` blocks.
//!
//! Run on the Rock 5A:
//!   ./target/release/qwen-tool-test
//!   RKLLM_MODEL_PATH=/path/to/other.rkllm ./target/release/qwen-tool-test

use std::sync::mpsc;

use rkllm_rs::prelude::*;

const QWEN3_MODEL: &str = "/home/ubuntu/models/Qwen3-1.7B_w8a8_g128_rk3588.rkllm";

/// Build a Qwen3 chat-template prompt with one tool defined.
///
/// This is the minimal subset of Qwen's official template needed to test
/// tool-call emission — system block with `<tools>` definitions and the
/// `<tool_call>` output instruction, then a user turn that should trigger
/// a tool call.
fn build_prompt() -> String {
    let tools = r#"{"type": "function", "function": {"name": "ring_bell", "description": "Ring a single meditation bell tone.", "parameters": {"type": "object", "properties": {}, "required": []}}}"#;

    format!(
        "<|im_start|>system\n\
         You are a meditation guide assistant. You have access to tools you can call.\n\n\
         # Tools\n\n\
         You may call one or more functions to assist with the user query.\n\n\
         You are provided with function signatures within <tools></tools> XML tags:\n\
         <tools>\n\
         {tools}\n\
         </tools>\n\n\
         For each function call, return a json object with function name and arguments within <tool_call></tool_call> XML tags:\n\
         <tool_call>\n\
         {{\"name\": <function-name>, \"arguments\": <args-json-object>}}\n\
         </tool_call><|im_end|>\n\
         <|im_start|>user\n\
         Please ring the meditation bell once to start the session.<|im_end|>\n\
         <|im_start|>assistant\n"
    )
}

struct Collector {
    tx: mpsc::Sender<String>,
}

impl RkllmCallbackHandler for Collector {
    fn handle(&mut self, result: Option<RKLLMResult<'_>>, state: LLMCallState) {
        match state {
            LLMCallState::Normal => {
                if let Some(r) = result {
                    let _ = self.tx.send(r.text.to_string());
                }
            }
            LLMCallState::Finish | LLMCallState::Error => {
                let _ = self.tx.send(String::new()); // sentinel ignored; drain on channel close
            }
            _ => {}
        }
    }
}

fn extract_tool_calls(raw: &str) -> Vec<Result<serde_json::Value, String>> {
    let mut out = Vec::new();
    let mut rest = raw;
    while let Some(start) = rest.find("<tool_call>") {
        let after_open = &rest[start + "<tool_call>".len()..];
        let Some(end) = after_open.find("</tool_call>") else {
            out.push(Err(format!(
                "unterminated <tool_call> starting at byte {start}"
            )));
            break;
        };
        let json_blob = after_open[..end].trim();
        let parsed = serde_json::from_str::<serde_json::Value>(json_blob)
            .map_err(|e| format!("parse error: {e} (blob: {json_blob:?})"));
        out.push(parsed);
        rest = &after_open[end + "</tool_call>".len()..];
    }
    out
}

fn main() -> Result<(), String> {
    let model_path = std::env::var("RKLLM_MODEL_PATH").unwrap_or_else(|_| QWEN3_MODEL.to_string());
    eprintln!("[qwen-tool-test] loading {model_path}");

    let mut config = LLMConfig::with_model_path(&model_path);
    // Qwen3-1.7B params per config/jhana.json
    config.max_new_tokens = 256;
    config.max_context_len = 1024;
    config.temperature = 0.7;
    config.top_p = 0.9;
    config.top_k = 40;
    config.repeat_penalty = 1.1;
    config.frequency_penalty = 0.0;
    config.presence_penalty = 0.0;

    let load_start = std::time::Instant::now();
    let handle = init(config).map_err(|e| format!("init failed: {e}"))?;
    eprintln!(
        "[qwen-tool-test] model loaded in {:.2}s",
        load_start.elapsed().as_secs_f32()
    );

    let prompt = build_prompt();
    eprintln!("[qwen-tool-test] prompt:\n{prompt}\n--- end prompt ---");

    let (tx, rx) = mpsc::channel::<String>();
    let collector = Collector { tx };

    let infer_start = std::time::Instant::now();
    handle
        .run(RKLLMInput::prompt(&prompt), Some(RKLLMInferParam::default()), collector)
        .map_err(|e| format!("inference failed: {e}"))?;
    let infer_secs = infer_start.elapsed().as_secs_f32();

    let mut raw = String::new();
    while let Ok(chunk) = rx.try_recv() {
        raw.push_str(&chunk);
    }

    eprintln!(
        "[qwen-tool-test] inference {:.2}s, {} bytes raw output",
        infer_secs,
        raw.len()
    );
    println!("===== RAW OUTPUT =====");
    println!("{raw}");
    println!("===== END RAW =====");

    let tool_calls = extract_tool_calls(&raw);
    if tool_calls.is_empty() {
        println!("\n[VERDICT] NO <tool_call> blocks found. Qwen3-1.7B did not emit a structured tool call.");
        std::process::exit(2);
    }
    println!("\n[VERDICT] Found {} <tool_call> block(s):", tool_calls.len());
    let mut ok = 0;
    for (i, tc) in tool_calls.iter().enumerate() {
        match tc {
            Ok(v) => {
                println!("  #{i}: OK  {v}");
                ok += 1;
            }
            Err(e) => println!("  #{i}: ERR {e}"),
        }
    }
    if ok == tool_calls.len() {
        println!("\nPASS: all tool_call blocks parsed as JSON.");
    } else {
        println!("\nPARTIAL: {ok}/{} parsed.", tool_calls.len());
        std::process::exit(3);
    }
    Ok(())
}
