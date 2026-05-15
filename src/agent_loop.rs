//! Bespoke Rust agent loop. Replaces the abandoned pi/Node experiment.
//!
//! The loop holds a chat history, drives `rkllm-rs` in-process turn by
//! turn, extracts `<tool_call>` blocks from the raw model output, and
//! dispatches them to a fixed catalog of six meditation-shaped tools
//! (`say`, `listen`, `ring_bell`, `pause`, `list_meditations`,
//! `read_meditation`). Streams [`AgentEvent`]s to the TUI thread via a
//! `mpsc::Sender`.
//!
//! Design rationale + NCF callouts: see `docs/16_AGENT.md`. Patterns
//! borrowed (loop body from `rig`, `AgentEvent` shape from `goose`,
//! `SentenceBuffer` from `vox`) are cited inline above the relevant
//! code.
//!
//! Not yet wired into `src/main.rs` — that's a follow-up commit. This
//! module is published so it compiles and can be unit-tested in
//! isolation first.

use std::path::PathBuf;
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::mpsc::{Receiver, Sender};
use std::sync::{Arc, Mutex};
use std::time::Duration;

use log::{info, warn};
use rkllm_rs::prelude::*;
use serde::Deserialize;
use serde_json::{Value, json};

use crate::openai_types::{ChatMessage, Role, ToolCall, ToolCallFunction, ToolDef, ToolFunction};

// ---------- Public types ----------

/// Events streamed from the agent loop to the TUI thread (and onward
/// to TTS / log pane). Shape borrowed from goose's `AgentEvent` enum
/// (see `block/goose` → `crates/goose/src/agents/agent.rs:185`).
#[derive(Debug, Clone)]
pub enum AgentEvent {
    /// A complete sentence ready for TTS + meditation pane render.
    Sentence(String),
    /// A tool call is about to dispatch — show in console pane.
    ToolStart { name: String, args: Value },
    /// A tool call finished. `snippet` is a short summary for the UI.
    ToolResult {
        name: String,
        ok: bool,
        snippet: String,
    },
    /// The model emitted a final response with no further tool calls.
    /// Session is over.
    Done,
    /// Something went wrong — log it, surface to UI, end the session.
    Error(String),
}

#[derive(Debug)]
pub enum AgentError {
    Model(String),
    Cancelled,
    MaxTurns(usize),
    Tool(String),
}

impl std::fmt::Display for AgentError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            Self::Model(s) => write!(f, "model error: {s}"),
            Self::Cancelled => write!(f, "cancelled by user"),
            Self::MaxTurns(n) => write!(f, "max_turns ({n}) reached"),
            Self::Tool(s) => write!(f, "tool dispatch error: {s}"),
        }
    }
}

impl std::error::Error for AgentError {}

// ---------- Tool catalog ----------

/// Seven fixed tools. No trait, no `Box<dyn>` — one `match` per dispatch.
/// See `docs/16_AGENT.md` § Tool catalog for NCF rationale.
///
/// `Goodnight` is the explicit session-end signal. The model emits it
/// as the very last tool of a session, and `run_agent` breaks the loop
/// after dispatching it. Without an explicit terminator, small models
/// (Qwen3-1.7B) struggle to ever emit a plain-text turn — they loop
/// closing pleasantries indefinitely. NCF-aligned: closings should be
/// *constructed*, not absent.
#[derive(Debug, Clone, Copy)]
pub enum Tool {
    Say,
    Listen,
    RingBell,
    Pause,
    ListSkills,
    ReadSkill,
    Goodnight,
}

impl Tool {
    pub fn name(self) -> &'static str {
        match self {
            Self::Say => "say",
            Self::Listen => "listen",
            Self::RingBell => "ring_bell",
            Self::Pause => "pause",
            Self::ListSkills => "list_skills",
            Self::ReadSkill => "read_skill",
            Self::Goodnight => "goodnight",
        }
    }

    pub fn from_name(name: &str) -> Option<Self> {
        Some(match name {
            "say" => Self::Say,
            "listen" => Self::Listen,
            "ring_bell" => Self::RingBell,
            "pause" => Self::Pause,
            "list_skills" => Self::ListSkills,
            "read_skill" => Self::ReadSkill,
            "goodnight" => Self::Goodnight,
            _ => return None,
        })
    }

    pub fn definition(self) -> ToolDef {
        let (description, parameters) = match self {
            Self::Say => (
                "Speak a sentence to the user. Blocks until TTS playback finishes — one speaker at a time.",
                json!({
                    "type": "object",
                    "properties": { "text": { "type": "string", "description": "What to say." } },
                    "required": ["text"]
                }),
            ),
            Self::Listen => (
                "Record from the user's microphone for `seconds` seconds and return the transcribed text. Plays a brief 'speak now' cue first.",
                json!({
                    "type": "object",
                    "properties": { "seconds": { "type": "integer", "minimum": 1, "maximum": 30, "default": 7 } }
                }),
            ),
            Self::RingBell => (
                "Ring the meditation bell once. Use as a phase boundary (start, close) — not for general emphasis.",
                json!({ "type": "object", "properties": {} }),
            ),
            Self::Pause => (
                "Silent gap. Use between breaths or as reflective silence. The pause is itself a meaningful turn — not a no-op.",
                json!({
                    "type": "object",
                    "properties": { "seconds": { "type": "number", "minimum": 0, "maximum": 120 } },
                    "required": ["seconds"]
                }),
            ),
            Self::ListSkills => (
                "Return the names of available skill modules — meditation styles, inquiry, repair handling, etc. — that you can load on demand with read_skill().",
                json!({ "type": "object", "properties": {} }),
            ),
            Self::ReadSkill => (
                "Read the body of a named skill module from prompts/agent_skills/ (e.g. 'loving_kindness', 'breath_awareness', 'inquiry', 'repair', 'jhana_qa'). Use as stylistic / structural guidance — paraphrase, don't quote verbatim.",
                json!({
                    "type": "object",
                    "properties": { "name": { "type": "string", "description": "Skill module name without the .md extension." } },
                    "required": ["name"]
                }),
            ),
            Self::Goodnight => (
                "End the session. Call this as the VERY LAST tool, AFTER your final closing say()/ring_bell()/pause(). Calling goodnight() releases the session — the runtime stops processing your output. Do not call any other tools after goodnight().",
                json!({ "type": "object", "properties": {} }),
            ),
        };
        ToolDef {
            kind: "function".to_string(),
            function: ToolFunction {
                name: self.name().to_string(),
                description: Some(description.to_string()),
                parameters: Some(parameters),
            },
        }
    }
}

pub fn all_tool_defs() -> Vec<ToolDef> {
    [
        Tool::Say,
        Tool::Listen,
        Tool::RingBell,
        Tool::Pause,
        Tool::ListSkills,
        Tool::ReadSkill,
        Tool::Goodnight,
    ]
    .into_iter()
    .map(Tool::definition)
    .collect()
}

// ---------- Tool argument types ----------

#[derive(Deserialize)]
struct SayArgs {
    text: String,
}

#[derive(Deserialize)]
struct ListenArgs {
    #[serde(default = "default_listen_seconds")]
    seconds: u32,
}

fn default_listen_seconds() -> u32 {
    7
}

#[derive(Deserialize)]
struct PauseArgs {
    seconds: f32,
}

#[derive(Deserialize)]
struct ReadSkillArgs {
    name: String,
}

// ---------- Agent context (handles to the rest of the binary) ----------

/// Handles passed into the agent loop so it can dispatch tools into
/// the existing `tts` / `stt` modules. The agent loop never touches
/// audio devices directly — it always goes through these channels,
/// preserving the existing serialised TTS / single-recorder behaviour.
///
/// The STT receiver is `Arc<Mutex<...>>` because in agent mode it's
/// shared between the agent thread (which drains it during `listen()`)
/// and `main.rs` (which holds it for the inter-session idle period).
/// At any given moment only one thread locks it, so the mutex
/// contention is nil.
pub struct AgentCtx {
    pub tts_tx: Sender<crate::tts::TtsCommand>,
    pub stt_tx: Sender<crate::stt::SttCommand>,
    pub stt_results: Arc<Mutex<Receiver<crate::stt::SttResult>>>,
    pub skills_dir: PathBuf,
}

// ---------- Sentence buffer (vox-inspired) ----------

/// Buffers token chunks and emits complete sentences. Borrowed from
/// `mrtozner/vox` → `src/streaming_chat.rs:38-110`. Handles "Dr." /
/// decimal points by requiring whitespace after the terminator before
/// flushing.
///
/// NCF: emitting at TCU (turn-construction-unit) boundaries — see
/// `docs/16_AGENT.md` § Streaming versus turn-level.
struct SentenceBuffer {
    buf: String,
}

impl SentenceBuffer {
    fn new() -> Self {
        Self { buf: String::new() }
    }

    fn push(&mut self, chunk: &str) -> Vec<String> {
        self.buf.push_str(chunk);
        let mut out = Vec::new();
        loop {
            let Some((idx, term)) = find_sentence_end(&self.buf) else {
                break;
            };
            // Split off everything up to and including the terminator
            // *plus* the whitespace we required to be sure it was an
            // end-of-sentence and not "Dr." mid-word.
            let split_at = idx + term.len_utf8() + 1; // +1 for the whitespace
            let split_at = split_at.min(self.buf.len());
            let sentence: String = self.buf.drain(..split_at).collect();
            let trimmed = sentence.trim();
            if !trimmed.is_empty() {
                out.push(trimmed.to_string());
            }
        }
        out
    }

    /// Drain whatever's left in the buffer as a final sentence. Used
    /// by the rkllm callback on `Finish` to flush a trailing partial.
    fn flush(&mut self) -> Option<String> {
        let out = std::mem::take(&mut self.buf).trim().to_string();
        if out.is_empty() { None } else { Some(out) }
    }
}

/// Find the first `.`, `!`, or `?` that is *followed by whitespace*.
/// That trailing whitespace is the disambiguator: "Dr. Smith" has
/// `.` followed by space too, but heuristic-good-enough for now;
/// future tighten: require space + uppercase / end-of-stream.
fn find_sentence_end(s: &str) -> Option<(usize, char)> {
    let bytes = s.as_bytes();
    for (i, ch) in s.char_indices() {
        if matches!(ch, '.' | '!' | '?') {
            let after = i + ch.len_utf8();
            if after < bytes.len() && bytes[after].is_ascii_whitespace() {
                return Some((i, ch));
            }
        }
    }
    None
}

// ---------- Prompt rendering (Qwen3 ChatML) ----------
// TODO(3d): extract this + parse_assistant() into a shared
// src/qwen_template.rs once main is wired in — currently duplicated
// with src/bin/jhana-llm-server.rs to keep the agent loop self-contained.

fn render_chatml(messages: &[ChatMessage], tools: &[ToolDef]) -> String {
    let mut out = String::new();
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
                out.push_str("\n</tool_response><|im_end|>\n");
            }
        }
    }
    out.push_str("<|im_start|>assistant\n");
    out
}

// ---------- Tool-call extraction ----------

fn parse_assistant(raw: &str) -> (String, Vec<ToolCall>) {
    let mut content = String::new();
    let mut calls = Vec::new();
    let mut rest = raw;
    while let Some(start) = rest.find("<tool_call>") {
        content.push_str(&rest[..start]);
        let after_open = &rest[start + "<tool_call>".len()..];
        let Some(end) = after_open.find("</tool_call>") else {
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
            let arguments = match v.get("arguments") {
                Some(Value::String(s)) => s.clone(),
                Some(v) => serde_json::to_string(v).unwrap_or_else(|_| "{}".to_string()),
                None => "{}".to_string(),
            };
            if !name.is_empty() {
                calls.push(ToolCall {
                    id: format!("call_{}", uuid::Uuid::new_v4().simple()),
                    kind: "function".to_string(),
                    function: ToolCallFunction { name, arguments },
                });
            }
        }
        rest = &after_open[end + "</tool_call>".len()..];
    }
    content.push_str(rest);
    (content.trim().to_string(), calls)
}

// ---------- rkllm-rs callback glue ----------

/// Handler passed to `rkllm_rs::LLMHandle::run()`. Owns the sentence
/// buffer and the events sender so it can emit `AgentEvent::Sentence`
/// live as tokens arrive. Sends the raw completion back to the caller
/// via a one-shot channel on `Finish`/`Error`.
struct StreamHandler {
    raw: String,
    buf: SentenceBuffer,
    events: Sender<AgentEvent>,
    raw_tx: std::sync::mpsc::SyncSender<String>,
}

impl RkllmCallbackHandler for StreamHandler {
    fn handle(&mut self, result: Option<RKLLMResult<'_>>, state: LLMCallState) {
        match state {
            LLMCallState::Normal => {
                if let Some(r) = result {
                    let text: &str = r.text.as_ref();
                    self.raw.push_str(text);
                    for s in self.buf.push(text) {
                        let _ = self.events.send(AgentEvent::Sentence(s));
                    }
                }
            }
            LLMCallState::Finish | LLMCallState::Error => {
                if let Some(trailing) = self.buf.flush() {
                    let _ = self.events.send(AgentEvent::Sentence(trailing));
                }
                let _ = self.raw_tx.send(std::mem::take(&mut self.raw));
            }
            _ => {}
        }
    }
}

fn run_completion(
    handle: &LLMHandle,
    prompt: &str,
    events: &Sender<AgentEvent>,
) -> Result<String, AgentError> {
    let (raw_tx, raw_rx) = std::sync::mpsc::sync_channel::<String>(1);
    let handler = StreamHandler {
        raw: String::new(),
        buf: SentenceBuffer::new(),
        events: events.clone(),
        raw_tx,
    };
    handle
        .run(
            RKLLMInput::prompt(prompt),
            Some(RKLLMInferParam::default()),
            handler,
        )
        .map_err(|e| AgentError::Model(format!("rkllm run failed: {e}")))?;
    raw_rx
        .recv_timeout(Duration::from_secs(1))
        .map_err(|_| AgentError::Model("rkllm finished but no raw output received".into()))
}

// ---------- Tool dispatch ----------

fn dispatch_tool(call: &ToolCall, ctx: &AgentCtx) -> Result<Value, AgentError> {
    let tool = Tool::from_name(&call.function.name)
        .ok_or_else(|| AgentError::Tool(format!("unknown tool: {}", call.function.name)))?;

    let args_str = if call.function.arguments.is_empty() {
        "{}"
    } else {
        &call.function.arguments
    };

    match tool {
        Tool::Say => {
            let a: SayArgs = serde_json::from_str(args_str)
                .map_err(|e| AgentError::Tool(format!("say args: {e}")))?;
            let (ack_tx, ack_rx) = std::sync::mpsc::channel::<()>();
            ctx.tts_tx
                .send(crate::tts::TtsCommand::SpeakAndAck(a.text.clone(), ack_tx))
                .map_err(|e| AgentError::Tool(format!("tts channel: {e}")))?;
            // 120 s to accommodate first-call paroli encoder+decoder
            // cold-load on the NPU. Subsequent calls are 2-3 s.
            ack_rx
                .recv_timeout(Duration::from_secs(120))
                .map_err(|_| AgentError::Tool("tts timeout".into()))?;
            Ok(json!({ "status": "spoken" }))
        }
        Tool::Listen => {
            let a: ListenArgs = serde_json::from_str(args_str).unwrap_or(ListenArgs {
                seconds: default_listen_seconds(),
            });
            ctx.stt_tx
                .send(crate::stt::SttCommand::Listen)
                .map_err(|e| AgentError::Tool(format!("stt channel: {e}")))?;
            let timeout = Duration::from_secs(u64::from(a.seconds) + 30);
            let deadline = std::time::Instant::now() + timeout;
            // Lock for the duration of this call — agent thread is the
            // only stt_results consumer in agent mode, so contention is
            // a non-issue. Drain Recording/Processing progress events,
            // wait for the terminal Transcribed/Error.
            let rx = ctx
                .stt_results
                .lock()
                .map_err(|_| AgentError::Tool("stt_results mutex poisoned".into()))?;
            loop {
                let remaining = deadline
                    .checked_duration_since(std::time::Instant::now())
                    .ok_or_else(|| AgentError::Tool("listen timeout".into()))?;
                match rx.recv_timeout(remaining) {
                    Ok(crate::stt::SttResult::Recording) => continue,
                    Ok(crate::stt::SttResult::Processing) => continue,
                    Ok(crate::stt::SttResult::Transcribed(text)) => {
                        return Ok(json!({ "transcript": text }));
                    }
                    Ok(crate::stt::SttResult::Error(e)) => {
                        return Err(AgentError::Tool(format!("stt error: {e}")));
                    }
                    Err(_) => return Err(AgentError::Tool("listen timeout".into())),
                }
            }
        }
        Tool::RingBell => {
            ctx.tts_tx
                .send(crate::tts::TtsCommand::Bell)
                .map_err(|e| AgentError::Tool(format!("tts channel: {e}")))?;
            // Bell is fire-and-forget; the TTS queue serialises it.
            Ok(json!({ "status": "rung" }))
        }
        Tool::Pause => {
            let a: PauseArgs = serde_json::from_str(args_str)
                .map_err(|e| AgentError::Tool(format!("pause args: {e}")))?;
            let ms = (a.seconds.clamp(0.0, 120.0) * 1000.0) as u64;
            std::thread::sleep(Duration::from_millis(ms));
            Ok(json!({ "status": "paused", "seconds": a.seconds }))
        }
        Tool::ListSkills => {
            let mut names: Vec<String> = std::fs::read_dir(&ctx.skills_dir)
                .map_err(|e| AgentError::Tool(format!("list_skills: {e}")))?
                .filter_map(|e| {
                    let name = e.ok()?.file_name().into_string().ok()?;
                    name.strip_suffix(".md").map(String::from)
                })
                .collect();
            names.sort();
            Ok(json!({ "names": names }))
        }
        Tool::ReadSkill => {
            let a: ReadSkillArgs = serde_json::from_str(args_str)
                .map_err(|e| AgentError::Tool(format!("read_skill args: {e}")))?;
            // Constrain path to skills dir — no path traversal.
            if a.name.contains('/') || a.name.contains("..") {
                return Err(AgentError::Tool("invalid skill name".into()));
            }
            let path = ctx.skills_dir.join(format!("{}.md", a.name));
            let body = std::fs::read_to_string(&path)
                .map_err(|e| AgentError::Tool(format!("read {path:?}: {e}")))?;
            Ok(json!({ "body": body }))
        }
        Tool::Goodnight => {
            // Dispatch is a no-op; the marker is the side effect.
            // run_agent checks for goodnight by name after every
            // dispatch and breaks the loop.
            Ok(json!({ "status": "session_ended" }))
        }
    }
}

fn summarize_value(v: &Value) -> String {
    let s = v.to_string();
    if s.len() > 120 {
        format!("{}…", &s[..120])
    } else {
        s
    }
}

// ---------- The loop ----------

/// Run the agent loop until the model emits a textual response with no
/// tool calls, or until `max_turns` is hit (graceful close), or until
/// `cancel` is set (user pressed BACK).
///
/// `history` is mutated in place — caller seeds it with a system prompt
/// and optionally a few-shot exemplar, then receives the full
/// conversation back including the final assistant turn.
pub fn run_agent(
    handle: &LLMHandle,
    history: &mut Vec<ChatMessage>,
    events: &Sender<AgentEvent>,
    ctx: &AgentCtx,
    cancel: &AtomicBool,
    max_turns: usize,
) -> Result<(), AgentError> {
    let tools = all_tool_defs();
    info!(
        "agent_loop: starting, {} tools, max_turns={max_turns}",
        tools.len()
    );

    for turn in 0..max_turns {
        if cancel.load(Ordering::Acquire) {
            info!("agent_loop: cancelled by user at turn {turn}");
            return Err(AgentError::Cancelled);
        }

        let prompt = render_chatml(history, &tools);
        let raw = run_completion(handle, &prompt, events)?;
        let (text, calls) = parse_assistant(&raw);
        history.push(ChatMessage::assistant_with_calls(text, calls.clone()));

        if calls.is_empty() {
            info!("agent_loop: done — no more tool calls after turn {turn}");
            let _ = events.send(AgentEvent::Done);
            return Ok(());
        }

        for call in &calls {
            let args: Value = serde_json::from_str(&call.function.arguments).unwrap_or(Value::Null);
            let _ = events.send(AgentEvent::ToolStart {
                name: call.function.name.clone(),
                args,
            });

            let (ok, snippet, result_content) = match dispatch_tool(call, ctx) {
                Ok(v) => (true, summarize_value(&v), v.to_string()),
                Err(e) => {
                    warn!("agent_loop: tool {} failed: {e}", call.function.name);
                    (
                        false,
                        e.to_string(),
                        json!({ "error": e.to_string() }).to_string(),
                    )
                }
            };
            let _ = events.send(AgentEvent::ToolResult {
                name: call.function.name.clone(),
                ok,
                snippet,
            });
            history.push(ChatMessage::tool_result(call.id.clone(), result_content));

            // Explicit session-end signal — see Tool::Goodnight. Break
            // the moment goodnight() dispatches so the model can't
            // emit more closing-pleasantry turns after it.
            if ok && call.function.name == "goodnight" {
                info!("agent_loop: goodnight() — session ended explicitly");
                let _ = events.send(AgentEvent::Done);
                return Ok(());
            }
        }
    }

    // max_turns hit — graceful close, not an error.
    // NCF: "sequence-closing sequence" — synthesise a closing turn
    // rather than crashing. See docs/16_AGENT.md § The loop.
    warn!("agent_loop: max_turns={max_turns} hit, closing gracefully");
    let close = "Let's pause here. Thank you for sitting.";
    let _ = events.send(AgentEvent::Sentence(close.to_string()));
    let _ = events.send(AgentEvent::Done);
    history.push(ChatMessage::assistant_text(close));
    Err(AgentError::MaxTurns(max_turns))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn sentence_buffer_splits_on_period_space() {
        let mut b = SentenceBuffer::new();
        let out = b.push("Hello. World!");
        assert_eq!(out, vec!["Hello."]);
        // No trailing space yet, so "World!" stays buffered until we
        // see one — that's the disambiguator.
        let out = b.push(" Again.");
        assert_eq!(out, vec!["World!"]);
    }

    #[test]
    fn sentence_buffer_flush_drains() {
        let mut b = SentenceBuffer::new();
        let _ = b.push("Trailing without terminator");
        let last = b.flush();
        assert_eq!(last.as_deref(), Some("Trailing without terminator"));
        // Subsequent flush is empty.
        assert!(b.flush().is_none());
    }

    #[test]
    fn parse_assistant_extracts_tool_call() {
        let raw =
            "Before. <tool_call>\n{\"name\":\"ring_bell\",\"arguments\":{}}\n</tool_call> after.";
        let (text, calls) = parse_assistant(raw);
        assert!(text.contains("Before."));
        assert!(text.contains("after."));
        assert_eq!(calls.len(), 1);
        assert_eq!(calls[0].function.name, "ring_bell");
    }

    #[test]
    fn tool_from_name_roundtrip() {
        for t in [
            Tool::Say,
            Tool::Listen,
            Tool::RingBell,
            Tool::Pause,
            Tool::ListSkills,
            Tool::ReadSkill,
            Tool::Goodnight,
        ] {
            assert_eq!(Tool::from_name(t.name()).map(|x| x.name()), Some(t.name()));
        }
        assert!(Tool::from_name("bash").is_none());
    }
}
