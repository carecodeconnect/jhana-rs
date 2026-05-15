//! OpenAI-compatible HTTP shim around rkllm-rs for the pi-port branch.
//!
//! Implements `POST /v1/chat/completions` so [pi](https://pi.dev) can talk
//! to Qwen3-1.7B on the RK3588 NPU the same way it talks to any cloud
//! provider. Only enough surface to make pi's tool-call loop work:
//! non-streaming responses, `tools[]` + `tool_choice` accepted, structured
//! `tool_calls[]` returned when the model emits `<tool_call>{...}</tool_call>`
//! blocks.
//!
//! Out of scope here (Phase 2 MVP):
//! - SSE streaming (pi will add this if it actually needs it)
//! - `/v1/audio/transcriptions` and `/v1/audio/speech` (Phase 4 — pi
//!   extensions will call separate endpoints for STT/TTS)
//! - Multi-model serving (single model, set via `JHANA_MODEL`)
//!
//! Run:
//!     ./target/release/jhana-llm-server                  # :8080, Qwen3-1.7B
//!     PORT=8081 jhana-llm-server                         # different port
//!     JHANA_MODEL=/home/ubuntu/models/foo.rkllm jhana-llm-server
//!
//! Smoke test (matches pi_sandbox/scripts/tool-call-test):
//!     curl -s http://127.0.0.1:8080/v1/chat/completions \
//!       -H 'Content-Type: application/json' \
//!       -d '{"model":"qwen3-1.7b","messages":[{"role":"user","content":"Ring the bell"}],
//!            "tools":[{"type":"function","function":{"name":"ring_bell",
//!            "description":"Ring a meditation bell once.",
//!            "parameters":{"type":"object","properties":{},"required":[]}}}]}'

use std::net::SocketAddr;
use std::sync::OnceLock;

use axum::{
    Json, Router,
    http::StatusCode,
    response::IntoResponse,
    routing::{get, post},
};
use log::{error, info};
use rkllm_rs::prelude::*;
use serde::{Deserialize, Serialize};
use serde_json::Value;
use tokio::sync::mpsc;

#[path = "../openai_types.rs"]
mod openai_types;

use openai_types::{
    ChatMessage as CanonicalMessage, Role, ToolCall, ToolCallFunction, ToolDef, WireContent,
};

const DEFAULT_MODEL: &str = "/home/ubuntu/models/Qwen3-1.7B_w8a8_g128_rk3588.rkllm";
const DEFAULT_ALIAS: &str = "qwen3-1.7b";

/// Loaded once and held for the lifetime of the process. `LLMHandle`'s
/// internal pointer is Sync, so we can call `run()` on it from any
/// blocking task. Matches the pattern in `src/llm.rs`.
static MODEL: OnceLock<LLMHandle> = OnceLock::new();

// ---------- Wire types (request envelope + response envelope only) ----------
//
// Canonical `ChatMessage`, `Role`, `ToolCall`, `ToolDef` etc. live in
// `src/openai_types.rs` (shared with the agent loop). The shim owns
// only the HTTP-specific request and response envelopes plus a
// wire-decode `WireChatMessage` that accepts OpenAI's array-form
// content and then flattens to canonical.

#[derive(Debug, Deserialize)]
struct ChatCompletionRequest {
    #[allow(dead_code)] // we serve a single model; field accepted but ignored
    model: Option<String>,
    messages: Vec<WireChatMessage>,
    #[serde(default)]
    tools: Vec<ToolDef>,
    /// `auto`, `none`, `required`, or a forced tool — we honour the
    /// presence of `tools[]` and ignore the precise mode for MVP.
    #[serde(default)]
    #[allow(dead_code)]
    tool_choice: Option<Value>,
    #[serde(default)]
    temperature: Option<f32>,
    #[serde(default)]
    top_p: Option<f32>,
    #[serde(default)]
    max_tokens: Option<i32>,
    /// Pi sends this; we don't stream yet, but accept the field so the
    /// parser doesn't reject the request.
    #[serde(default)]
    #[allow(dead_code)]
    stream: Option<bool>,
}

/// Request-side message shape. Accepts OpenAI's two `content` forms
/// (plain string or array of content parts) via `WireContent`; we
/// flatten to canonical `ChatMessage` for everything downstream.
#[derive(Debug, Deserialize)]
struct WireChatMessage {
    role: String,
    #[serde(default)]
    content: Option<WireContent>,
    #[serde(default)]
    name: Option<String>,
    #[serde(default)]
    tool_call_id: Option<String>,
    #[serde(default)]
    tool_calls: Option<Vec<Value>>,
}

impl WireChatMessage {
    fn into_canonical(self) -> CanonicalMessage {
        let role = match self.role.as_str() {
            "system" => Role::System,
            "assistant" => Role::Assistant,
            "tool" => Role::Tool,
            _ => Role::User, // unknown roles fold to user; safer than panicking on input
        };
        // We accept assistant turns carrying tool_calls in arbitrary
        // JSON shape (pi sends them when echoing a previous turn);
        // we only flatten the *content* and pass tool_calls through
        // as the model's own historical output. The render path
        // re-serialises them into <tool_call> blocks.
        let _ = self.tool_calls; // not threaded through canonical yet
        let _ = self.name;
        CanonicalMessage {
            role,
            content: self
                .content
                .as_ref()
                .map(WireContent::as_text)
                .unwrap_or_default(),
            tool_calls: Vec::new(),
            tool_call_id: self.tool_call_id,
            name: None,
        }
    }
}

#[derive(Debug, Serialize)]
struct ChatCompletionResponse {
    id: String,
    object: &'static str,
    created: u64,
    model: String,
    choices: Vec<Choice>,
    usage: Usage,
}

#[derive(Debug, Serialize)]
struct Choice {
    index: u32,
    message: AssistantMessage,
    finish_reason: &'static str,
}

#[derive(Debug, Serialize)]
struct AssistantMessage {
    role: &'static str,
    #[serde(skip_serializing_if = "Option::is_none")]
    content: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    tool_calls: Option<Vec<ToolCall>>,
}

#[derive(Debug, Serialize, Default)]
struct Usage {
    prompt_tokens: u32,
    completion_tokens: u32,
    total_tokens: u32,
}

// ---------- Prompt rendering ----------

/// Render a Qwen3 ChatML prompt with optional tool definitions in the
/// system block. Tool format follows Qwen's official template (the same
/// format `src/bin/qwen-tool-test.rs` validated as working). Operates on
/// canonical `ChatMessage` (flat `String` content); the agent loop will
/// share this signature.
fn render_qwen_prompt(messages: &[CanonicalMessage], tools: &[ToolDef]) -> String {
    let mut out = String::new();

    // Pull the user-provided system message (if any) and prepend the
    // tool-call instructions to it. Qwen needs the tools described
    // *inside* a system turn for the template to recognise them.
    let user_system = messages
        .iter()
        .find(|m| m.role == Role::System)
        .map(|m| m.content.clone())
        .filter(|s| !s.is_empty())
        .unwrap_or_else(|| "You are a helpful assistant.".to_string());

    out.push_str("<|im_start|>system\n");
    out.push_str(&user_system);
    if !tools.is_empty() {
        out.push_str("\n\n# Tools\n\nYou may call one or more functions to assist with the user query.\n\nYou are provided with function signatures within <tools></tools> XML tags:\n<tools>\n");
        for t in tools {
            if let Ok(s) = serde_json::to_string(t) {
                out.push_str(&s);
                out.push('\n');
            }
        }
        out.push_str("</tools>\n\nFor each function call, return a json object with function name and arguments within <tool_call></tool_call> XML tags:\n<tool_call>\n{\"name\": <function-name>, \"arguments\": <args-json-object>}\n</tool_call>");
    }
    out.push_str("<|im_end|>\n");

    for m in messages {
        match m.role {
            Role::System => continue,
            Role::User => {
                out.push_str("<|im_start|>user\n");
                out.push_str(&m.content);
                out.push_str("<|im_end|>\n");
            }
            Role::Assistant => {
                out.push_str("<|im_start|>assistant\n");
                out.push_str(&m.content);
                // Serialise any historical tool_calls back as <tool_call>
                // blocks so the model sees its own previous output.
                for c in &m.tool_calls {
                    out.push_str(&format!(
                        "\n<tool_call>\n{{\"name\": \"{}\", \"arguments\": {}}}\n</tool_call>",
                        c.function.name, c.function.arguments
                    ));
                }
                out.push_str("<|im_end|>\n");
            }
            Role::Tool => {
                out.push_str("<|im_start|>user\n<tool_response>\n");
                out.push_str(&m.content);
                let _ = &m.tool_call_id; // identity is implicit in turn order
                out.push_str("\n</tool_response><|im_end|>\n");
            }
        }
    }

    out.push_str("<|im_start|>assistant\n");
    out
}

// ---------- Tool-call extraction ----------

/// Pull each `<tool_call>{...}</tool_call>` block out of the raw model
/// output. Returns `(content_without_blocks, [tool_call, ...])`. The
/// content is whatever prose was outside any block — usually empty when
/// the model decided to call a tool.
fn extract_tool_calls(raw: &str) -> (String, Vec<ToolCall>) {
    let mut content = String::new();
    let mut calls = Vec::new();
    let mut rest = raw;

    while let Some(start) = rest.find("<tool_call>") {
        content.push_str(&rest[..start]);
        let after_open = &rest[start + "<tool_call>".len()..];
        let Some(end) = after_open.find("</tool_call>") else {
            // Unterminated — bail and keep the rest as prose so we don't
            // silently swallow the broken tail.
            content.push_str(&rest[start..]);
            return (content.trim().to_string(), calls);
        };
        let json_blob = after_open[..end].trim();
        if let Ok(v) = serde_json::from_str::<Value>(json_blob) {
            let name = v
                .get("name")
                .and_then(Value::as_str)
                .unwrap_or("")
                .to_string();
            // Arguments may be an object or already-stringified; OpenAI
            // wants a string, so re-serialise the object form.
            let args = match v.get("arguments") {
                Some(Value::String(s)) => s.clone(),
                Some(v) => serde_json::to_string(v).unwrap_or_else(|_| "{}".to_string()),
                None => "{}".to_string(),
            };
            if !name.is_empty() {
                calls.push(ToolCall {
                    id: format!("call_{}", uuid::Uuid::new_v4().simple()),
                    kind: "function".to_string(),
                    function: ToolCallFunction {
                        name,
                        arguments: args,
                    },
                });
            }
        } else {
            // Unparseable JSON inside the block — surface it as content
            // so the failure is debuggable instead of swallowed.
            content.push_str("<tool_call>");
            content.push_str(json_blob);
            content.push_str("</tool_call>");
        }
        rest = &after_open[end + "</tool_call>".len()..];
    }
    content.push_str(rest);
    (content.trim().to_string(), calls)
}

// ---------- rkllm-rs glue ----------

struct Collector {
    tx: std::sync::mpsc::Sender<Result<String, String>>,
}

impl RkllmCallbackHandler for Collector {
    fn handle(&mut self, result: Option<RKLLMResult<'_>>, state: LLMCallState) {
        match state {
            LLMCallState::Normal => {
                if let Some(r) = result {
                    let _ = self.tx.send(Ok(r.text.to_string()));
                }
            }
            LLMCallState::Finish => {
                let _ = self.tx.send(Ok(String::new()));
            }
            LLMCallState::Error => {
                let _ = self.tx.send(Err("rkllm inference error".into()));
            }
            _ => {}
        }
    }
}

fn load_model() -> Result<&'static LLMHandle, String> {
    if let Some(h) = MODEL.get() {
        return Ok(h);
    }
    let model_path = std::env::var("JHANA_MODEL").unwrap_or_else(|_| DEFAULT_MODEL.to_string());
    info!("loading rkllm model: {model_path} (cold load ~80-90 s)");
    let mut config = LLMConfig::with_model_path(&model_path);
    config.max_new_tokens = 1024;
    config.max_context_len = 4096;
    // Qwen3 sampler — Qwen's official recommendation per pi_sandbox.
    config.temperature = 0.7;
    config.top_p = 0.9;
    config.top_k = 40;
    config.repeat_penalty = 1.1;
    config.frequency_penalty = 0.0;
    config.presence_penalty = 0.0;
    let handle = init(config).map_err(|e| format!("rkllm init failed: {e}"))?;
    Ok(MODEL.get_or_init(|| handle))
}

fn run_inference(prompt: &str, sampler: SamplerOverrides) -> Result<String, String> {
    let handle = load_model()?;
    let _ = sampler; // sampler override would require reconfiguring the handle per request; deferred

    let (tx, rx) = std::sync::mpsc::channel::<Result<String, String>>();
    handle
        .run(
            RKLLMInput::prompt(prompt),
            Some(RKLLMInferParam::default()),
            Collector { tx },
        )
        .map_err(|e| format!("rkllm run failed: {e}"))?;
    let mut raw = String::new();
    while let Ok(chunk) = rx.try_recv() {
        match chunk {
            Ok(s) => raw.push_str(&s),
            Err(e) => return Err(e),
        }
    }
    Ok(raw)
}

#[derive(Default)]
struct SamplerOverrides {
    _temperature: Option<f32>,
    _top_p: Option<f32>,
    _max_tokens: Option<i32>,
}

// ---------- HTTP handlers ----------

async fn chat_completions(Json(req): Json<ChatCompletionRequest>) -> impl IntoResponse {
    // Flatten the wire-form messages (which may have array-shaped content)
    // into canonical String-content form before prompt rendering.
    let messages: Vec<CanonicalMessage> = req
        .messages
        .into_iter()
        .map(WireChatMessage::into_canonical)
        .collect();
    let prompt = render_qwen_prompt(&messages, &req.tools);
    info!(
        "chat.completions: {} messages, {} tools, {} prompt bytes",
        messages.len(),
        req.tools.len(),
        prompt.len()
    );
    let sampler = SamplerOverrides {
        _temperature: req.temperature,
        _top_p: req.top_p,
        _max_tokens: req.max_tokens,
    };

    let raw = match tokio::task::spawn_blocking(move || run_inference(&prompt, sampler)).await {
        Ok(Ok(s)) => s,
        Ok(Err(e)) => {
            error!("inference failed: {e}");
            return (
                StatusCode::INTERNAL_SERVER_ERROR,
                Json(serde_json::json!({"error": e})),
            )
                .into_response();
        }
        Err(e) => {
            error!("inference task panicked: {e}");
            return (
                StatusCode::INTERNAL_SERVER_ERROR,
                Json(serde_json::json!({"error": e.to_string()})),
            )
                .into_response();
        }
    };

    let (content, tool_calls) = extract_tool_calls(&raw);
    let (finish_reason, message) = if !tool_calls.is_empty() {
        (
            "tool_calls",
            AssistantMessage {
                role: "assistant",
                content: if content.is_empty() {
                    None
                } else {
                    Some(content)
                },
                tool_calls: Some(tool_calls),
            },
        )
    } else {
        (
            "stop",
            AssistantMessage {
                role: "assistant",
                content: Some(content),
                tool_calls: None,
            },
        )
    };

    let alias = std::env::var("JHANA_ALIAS").unwrap_or_else(|_| DEFAULT_ALIAS.to_string());
    let resp = ChatCompletionResponse {
        id: format!("chatcmpl-{}", uuid::Uuid::new_v4().simple()),
        object: "chat.completion",
        created: std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .map(|d| d.as_secs())
            .unwrap_or(0),
        model: alias,
        choices: vec![Choice {
            index: 0,
            message,
            finish_reason,
        }],
        usage: Usage::default(),
    };
    Json(resp).into_response()
}

async fn health() -> impl IntoResponse {
    Json(serde_json::json!({"status": "ok", "model_loaded": MODEL.get().is_some()}))
}

async fn props() -> impl IntoResponse {
    // Minimal llama-server-compatible /props so pi's chat-template probe
    // (per pi_sandbox docs/04-tool-calling.md) doesn't error out.
    Json(serde_json::json!({
        "chat_template": "{# Qwen3 ChatML, rendered server-side in src/bin/jhana-llm-server.rs #}",
        "default_generation_settings": {
            "temperature": 0.7,
            "top_p": 0.9,
            "top_k": 40,
        }
    }))
}

#[tokio::main]
async fn main() -> Result<(), Box<dyn std::error::Error>> {
    simplelog::SimpleLogger::init(log::LevelFilter::Info, simplelog::Config::default()).ok();

    // Pre-load the model so the first request doesn't pay the cold-load
    // cost. Blocking, intentional — we want startup to fail fast if the
    // model file is missing rather than 500 on the first request.
    info!("jhana-llm-server starting");
    if let Err(e) = load_model() {
        error!("model load failed at startup: {e}");
        std::process::exit(1);
    }
    info!("model loaded; ready for requests");

    let app = Router::new()
        .route("/health", get(health))
        .route("/props", get(props))
        .route("/v1/chat/completions", post(chat_completions));

    let port: u16 = std::env::var("PORT")
        .ok()
        .and_then(|s| s.parse().ok())
        .unwrap_or(8080);
    let addr = SocketAddr::from(([0, 0, 0, 0], port));
    let listener = tokio::net::TcpListener::bind(addr).await?;
    info!("listening on http://{addr}");
    axum::serve(listener, app).await?;
    Ok(())
}

// Silence the unused mpsc import on builds where the type alias collapses.
#[allow(dead_code)]
fn _force_use_tokio_mpsc() {
    let _: mpsc::Sender<()>;
}
