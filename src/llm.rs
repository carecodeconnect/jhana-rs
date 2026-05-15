//! LLM integration via rkllm-rs on the RK3588 NPU.
//!
//! Loads a `.rkllm` model directly on the NPU via [`rkllm_rs`] and streams
//! tokens through an mpsc channel to the main TUI event loop. No HTTP server,
//! no Python — direct Rust FFI to `librkllmrt.so`.
//!
//! # Model loading
//!
//! The model is loaded once when [`start_streaming`] is first called and kept
//! in memory for subsequent meditations. Loading takes ~130s for a 3B model.
//! The model handle is stored in a global [`OnceLock`] so it survives across
//! meditation sessions.
//!
//! # Prompt loading
//!
//! Prompts are loaded from the `prompts/` directory at runtime:
//! - `prompts/system.txt` — system prompt (meditation guide persona)
//! - `prompts/meditations/{type}.txt` — meditation-specific few-shot example
//!
//! # Pause marker parsing
//!
//! The LLM is prompted to emit `[N]` markers (e.g. `[5]`, `[3.5]`) inline
//! with meditation text. A bracket state machine splits streaming tokens
//! into sentences and pause durations.

use std::sync::OnceLock;
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::mpsc::Sender;

use log::{error, info, warn};
use rkllm_rs::prelude::*;

/// Default model path on the Rock 5A.
const DEFAULT_MODEL: &str = "/home/ubuntu/models/Llama-3.2-3B-Instruct_w8a8_g128_rk3588.rkllm";

/// Maximum tokens to generate per meditation.
// Max tokens per generation now lives in config/jhana.json under
// the active model's `max_new_tokens` field.

/// Global model handle — loaded once, reused across meditations.
static MODEL: OnceLock<LLMHandle> = OnceLock::new();

/// Output from the LLM streaming pipeline.
///
/// Each variant represents a discrete event in the meditation text stream.
/// The orchestrator receives these via a channel and dispatches to the TUI
/// and TTS subsystems.
#[derive(Debug, Clone, PartialEq)]
pub enum LlmOutput {
    /// A complete sentence ready for display and TTS.
    Sentence(String),
    /// A pause marker: the meditation should be silent for this many seconds.
    /// Emitted when the LLM produces `[N]` (with N a decimal number).
    Pause(f32),
    /// Ring the meditation bell. Emitted when the LLM produces `[BELL]`.
    /// The TTS thread plays a pre-rendered bell WAV through the speaker.
    Bell,
    /// The LLM has finished generating.
    Done,
    /// An error occurred during streaming.
    Error(String),
}

/// Parse streaming text chunks into sentences and pause markers.
///
/// Implements the bracket state machine from the Python jhana-dev codebase:
/// - Text outside `[]` accumulates in a sentence buffer
/// - `[` enters pause mode, `]` exits it
/// - Content between brackets is parsed as a float (seconds)
/// - Sentences are split on `.` boundaries
///
/// This parser is designed to be fed token-by-token from the LLM streaming
/// output. Call [`ChunkParser::feed`] for each token and collect the results.
pub struct ChunkParser {
    sentence_buf: String,
    pause_buf: String,
    in_pause: bool,
    sentences_emitted: bool,
}

impl ChunkParser {
    /// Create a new parser with empty buffers.
    pub fn new() -> Self {
        Self {
            sentence_buf: String::new(),
            pause_buf: String::new(),
            in_pause: false,
            sentences_emitted: false,
        }
    }

    /// Feed a text chunk (typically one token) and return any completed outputs.
    ///
    /// May return zero, one, or multiple outputs per call (e.g. a sentence
    /// ending with `.` followed by a `[` in the same token).
    pub fn feed(&mut self, chunk: &str) -> Vec<LlmOutput> {
        let mut outputs = Vec::new();

        for ch in chunk.chars() {
            if ch == '[' && !self.in_pause {
                self.in_pause = true;
                self.pause_buf.clear();
            } else if ch == ']' && self.in_pause {
                self.in_pause = false;
                let token = self.pause_buf.trim();
                // `[BELL]` (case-insensitive) → ring the meditation bell.
                if token.eq_ignore_ascii_case("bell") {
                    outputs.push(LlmOutput::Bell);
                } else if let Ok(duration) = token.parse::<f32>() {
                    // `[N]` (a number) → silent pause for N seconds.
                    // Skip leading pauses (before any sentence has been spoken)
                    if self.sentences_emitted {
                        outputs.push(LlmOutput::Pause(duration));
                    }
                }
                // Anything else inside brackets is silently dropped; the
                // model occasionally emits stage directions like
                // `[breathing in]` that we don't want to render or speak.
                self.pause_buf.clear();
            } else if self.in_pause {
                self.pause_buf.push(ch);
            } else {
                self.sentence_buf.push(ch);
                // Emit sentence on period followed by space (or end of text)
                if ch == '.' {
                    let sentence = self.sentence_buf.trim().to_string();
                    if !sentence.is_empty() {
                        outputs.push(LlmOutput::Sentence(sentence));
                        self.sentences_emitted = true;
                    }
                    self.sentence_buf.clear();
                }
            }
        }

        outputs
    }

    /// Flush any remaining text as a final sentence.
    ///
    /// Call this after the LLM signals completion to emit any trailing text
    /// that didn't end with a period.
    pub fn flush(&mut self) -> Vec<LlmOutput> {
        let mut outputs = Vec::new();
        let remaining = self.sentence_buf.trim().to_string();
        if !remaining.is_empty() {
            outputs.push(LlmOutput::Sentence(remaining));
            self.sentence_buf.clear();
        }
        outputs.push(LlmOutput::Done);
        outputs
    }
}

/// Load the system prompt and a meditation-specific user prompt from disk.
///
/// Reads `prompts/system.txt` and `prompts/meditations/{meditation_type}.txt`
/// relative to the current working directory. Returns `(system, user)` prompts.
///
/// Prompts are loaded from files rather than compiled into the binary so that
/// they can be edited on-device without rebuilding. Each meditation type
/// (flower garden, lotus flower, etc.) has its own file containing a cleaned
/// few-shot example with `[N]` pause markers and a user instruction.
pub fn load_prompts(meditation_type: &str) -> Result<(String, String), String> {
    let system = std::fs::read_to_string("prompts/system.txt")
        .map_err(|e| format!("Failed to read prompts/system.txt: {e}"))?;
    let user_path = format!("prompts/meditations/{meditation_type}.txt");
    let user = std::fs::read_to_string(&user_path)
        .map_err(|e| format!("Failed to read {user_path}: {e}"))?;
    Ok((system, user))
}

/// List available meditation types by scanning the prompts/meditations/ directory.
#[expect(dead_code)] // will be used when meditation selection menu is added
pub fn list_meditations() -> Vec<String> {
    let Ok(entries) = std::fs::read_dir("prompts/meditations") else {
        return Vec::new();
    };
    let mut types: Vec<String> = entries
        .filter_map(|e| {
            let name = e.ok()?.file_name().into_string().ok()?;
            name.strip_suffix(".txt").map(String::from)
        })
        .collect();
    types.sort();
    types
}

/// Set to `true` once the RKLLM model has finished loading via
/// [`preload`]. The TUI's main loop pairs this with
/// [`crate::stt::STT_READY`] to gate the welcome speech: greeting
/// while loads are still in flight means the user can press
/// ENTER and then sit staring at a frozen screen.
pub static LLM_READY: AtomicBool = AtomicBool::new(false);

/// Pre-load the RKLLM model in a background thread so the first
/// `start_streaming` call doesn't pay the ~37–74 s NPU init cost.
///
/// Call this once from `main()` shortly after the TUI starts. Any
/// later `start_streaming` will reuse the cached handle. The load
/// log line ("RKLLM model loaded") will appear in `jhana-rs.log`
/// when warmup completes. If this fails, the error is logged but
/// the function returns — the next `start_streaming` will retry.
pub fn preload() {
    std::thread::Builder::new()
        .name("llm-preload".into())
        .spawn(|| match get_or_load_model() {
            Ok(_) => {
                info!("RKLLM preload complete");
                LLM_READY.store(true, Ordering::Release);
            }
            Err(e) => error!("RKLLM preload failed: {e}"),
        })
        .expect("failed to spawn llm-preload thread");
}

/// Ensure the RKLLM model is loaded, returning a reference to the handle.
///
/// First call loads the model (~37–74 s for 3 B on the RK3588 NPU,
/// depending on whether page cache is warm). Subsequent calls return
/// the cached handle instantly. Call [`preload`] at startup to move
/// this cost off the first-button-press hot path.
///
/// `pub(crate)` so `agent_loop::run_agent` can grab the handle directly
/// for in-process inference, bypassing the streaming-thread pattern
/// `start_streaming` uses.
pub(crate) fn get_or_load_model() -> Result<&'static LLMHandle, String> {
    if let Some(handle) = MODEL.get() {
        return Ok(handle);
    }

    // Model + sampling parameters now come from `config/jhana.json`
    // so we can swap among Llama 3B / 1B / Qwen3 1.7B (or any other
    // .rkllm we drop into ~/models) by editing one file. The
    // RKLLM_MODEL_PATH env var still wins for one-off overrides.
    let m = crate::config::active_model();
    let model_path = std::env::var("RKLLM_MODEL_PATH").unwrap_or_else(|_| m.path.clone());

    info!("Loading RKLLM model from {model_path} (cold load ~30–90 s)...");

    let mut config = LLMConfig::with_model_path(&model_path);
    config.max_new_tokens = m.max_new_tokens;
    config.max_context_len = m.max_context_len;
    config.temperature = m.temperature;
    config.top_p = m.top_p;
    config.top_k = m.top_k;
    config.repeat_penalty = m.repeat_penalty;
    config.frequency_penalty = 0.0;
    config.presence_penalty = 0.0;

    let handle = init(config).map_err(|e| format!("Failed to load RKLLM model: {e}"))?;

    info!("RKLLM model loaded");
    Ok(MODEL.get_or_init(|| handle))
}

/// Callback handler that feeds tokens through `ChunkParser` into the mpsc channel.
struct StreamHandler {
    tx: Sender<LlmOutput>,
    parser: ChunkParser,
}

impl RkllmCallbackHandler for StreamHandler {
    fn handle(&mut self, result: Option<RKLLMResult<'_>>, state: LLMCallState) {
        match state {
            LLMCallState::Normal => {
                if let Some(r) = result {
                    for output in self.parser.feed(&r.text) {
                        if self.tx.send(output).is_err() {
                            return; // receiver dropped
                        }
                    }
                }
            }
            LLMCallState::Finish => {
                for output in self.parser.flush() {
                    let _ = self.tx.send(output);
                }
            }
            LLMCallState::Error => {
                let _ = self
                    .tx
                    .send(LlmOutput::Error("RKLLM inference error".into()));
            }
            LLMCallState::Waiting | LLMCallState::GetLastHiddenLayer => {}
        }
    }
}

/// Spawn a background thread that streams a meditation from the NPU.
///
/// Sends [`LlmOutput`] events through `tx`. The thread loads the model
/// (if not already loaded), formats the prompt with Llama 3 chat template,
/// and runs inference with a callback that feeds tokens through `ChunkParser`.
pub fn start_streaming(tx: Sender<LlmOutput>, system: String, user: String) {
    std::thread::Builder::new()
        .name("llm-stream".into())
        .spawn(move || {
            info!("LLM streaming thread started");
            if let Err(e) = run_inference(&tx, &system, &user) {
                error!("LLM error: {e}");
                let _ = tx.send(LlmOutput::Error(e));
            }
        })
        .expect("failed to spawn LLM thread");
}

/// Load model and run inference, streaming tokens through the channel.
fn run_inference(tx: &Sender<LlmOutput>, system: &str, user: &str) -> Result<(), String> {
    let handle = get_or_load_model()?;

    let prompt = match crate::config::active_model().chat_template.as_str() {
        "qwen" => format!(
            "<|im_start|>system\n{system}<|im_end|>\n\
             <|im_start|>user\n{user}<|im_end|>\n\
             <|im_start|>assistant\n"
        ),
        // Default: Llama-3 family template.
        _ => format!(
            "<|begin_of_text|><|start_header_id|>system<|end_header_id|>\n\n\
             {system}<|eot_id|><|start_header_id|>user<|end_header_id|>\n\n\
             {user}<|eot_id|><|start_header_id|>assistant<|end_header_id|>\n\n"
        ),
    };

    info!(
        "Starting NPU inference (max {} tokens)",
        crate::config::active_model().max_new_tokens
    );

    let handler = StreamHandler {
        tx: tx.clone(),
        parser: ChunkParser::new(),
    };

    let input = RKLLMInput::prompt(&prompt);
    let params = RKLLMInferParam::default();

    handle
        .run(input, Some(params), handler)
        .map_err(|e| format!("RKLLM inference failed: {e}"))?;

    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn parse_sentence_and_pause() {
        let mut parser = ChunkParser::new();
        let outputs = parser.feed("Close your eyes.");
        assert_eq!(
            outputs,
            vec![LlmOutput::Sentence("Close your eyes.".into())]
        );

        let outputs = parser.feed(" [5] ");
        assert_eq!(outputs, vec![LlmOutput::Pause(5.0)]);
    }

    #[test]
    fn skip_leading_pause() {
        let mut parser = ChunkParser::new();
        // Pause before any sentence should be skipped
        let outputs = parser.feed("[3]");
        assert_eq!(outputs, vec![]);

        // Now emit a sentence, then a pause should work
        let outputs = parser.feed("Hello.");
        assert_eq!(outputs, vec![LlmOutput::Sentence("Hello.".into())]);

        let outputs = parser.feed(" [5]");
        assert_eq!(outputs, vec![LlmOutput::Pause(5.0)]);
    }

    #[test]
    fn flush_trailing_text() {
        let mut parser = ChunkParser::new();
        parser.feed("Some text without period");
        let outputs = parser.flush();
        assert_eq!(
            outputs,
            vec![
                LlmOutput::Sentence("Some text without period".into()),
                LlmOutput::Done,
            ]
        );
    }

    #[test]
    fn multiple_sentences_in_one_chunk() {
        let mut parser = ChunkParser::new();
        let outputs = parser.feed("First. Second.");
        assert_eq!(
            outputs,
            vec![
                LlmOutput::Sentence("First.".into()),
                LlmOutput::Sentence("Second.".into()),
            ]
        );
    }

    #[test]
    fn fractional_pause_duration() {
        let mut parser = ChunkParser::new();
        parser.feed("Start.");
        let outputs = parser.feed(" [3.5] ");
        assert_eq!(outputs, vec![LlmOutput::Pause(3.5)]);
    }
}
