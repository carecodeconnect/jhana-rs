//! LLM integration via mistral.rs OpenAI-compatible HTTP API.
//!
//! Connects to a local mistral.rs server (default `localhost:8321`) running
//! Ministral 3B. Uses streaming Server-Sent Events (SSE) to receive tokens
//! in real time, parses them through [`ChunkParser`] to split into sentences
//! and pause markers, and sends [`LlmOutput`] events through an mpsc channel
//! to the main TUI event loop.
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

use std::io::{BufRead, BufReader, Write};
use std::net::TcpStream;
use std::sync::mpsc::Sender;
use std::time::Duration;

use log::{error, info};

/// mistral.rs server address (host:port).
const SERVER_ADDR: &str = "127.0.0.1:8321";

/// API endpoint path.
const API_PATH: &str = "/v1/chat/completions";

/// TCP connect timeout in seconds.
const CONNECT_TIMEOUT: u64 = 10;

/// Read timeout in seconds — long because Ministral 3B generates slowly
/// (~3.89 tok/s) and the first token may take time if the model is loading.
const READ_TIMEOUT: u64 = 300;

/// Maximum tokens to generate per meditation.
const MAX_TOKENS: u32 = 1024;

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
    Pause(f32),
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
                // Parse pause duration, skip if not a valid number
                if let Ok(duration) = self.pause_buf.trim().parse::<f32>() {
                    // Skip leading pauses (before any sentence has been spoken)
                    if self.sentences_emitted {
                        outputs.push(LlmOutput::Pause(duration));
                    }
                }
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

/// Spawn a background thread that streams a meditation from the mistral.rs server.
///
/// Sends [`LlmOutput`] events through `tx`. The thread connects to the
/// local mistral.rs server, sends the system and user prompts with streaming
/// enabled, parses the SSE response, and feeds tokens through [`ChunkParser`].
///
/// Uses the same pattern as GPIO button polling: a background `std::thread`
/// with `std::sync::mpsc` channel. No async runtime is needed, which keeps
/// the main event loop simple and avoids tokio as a dependency on aarch64.
///
/// If the connection fails or an error occurs mid-stream, sends
/// [`LlmOutput::Error`] and exits. If the receiver is dropped (TUI quit),
/// the thread detects the closed channel and exits cleanly.
pub fn start_streaming(tx: Sender<LlmOutput>, system: String, user: String) {
    std::thread::Builder::new()
        .name("llm-stream".into())
        .spawn(move || {
            info!("LLM streaming thread started");
            if let Err(e) = stream_meditation(&tx, &system, &user) {
                error!("LLM streaming error: {e}");
                let _ = tx.send(LlmOutput::Error(e));
            }
        })
        .expect("failed to spawn LLM thread");
}

/// Perform the HTTP request and stream the response.
///
/// Uses raw `TcpStream` instead of an HTTP client crate because SSE
/// streaming over localhost doesn't need TLS, and HTTP client crates
/// (minreq, ureq) often buffer chunked responses rather than streaming
/// them line-by-line. Raw TCP gives us immediate access to each SSE
/// event as it arrives from mistral.rs.
fn stream_meditation(tx: &Sender<LlmOutput>, system: &str, user: &str) -> Result<(), String> {
    let body = serde_json::json!({
        "model": "default",
        "messages": [
            {"role": "system", "content": system},
            {"role": "user", "content": user}
        ],
        "stream": true,
        "temperature": 0.7,
        "max_tokens": MAX_TOKENS
    });

    let body_str = body.to_string();
    info!("POST {SERVER_ADDR}{API_PATH} (stream=true, max_tokens={MAX_TOKENS})");

    // Connect with timeout
    let mut stream = TcpStream::connect_timeout(
        &SERVER_ADDR
            .parse()
            .map_err(|e| format!("Bad address: {e}"))?,
        Duration::from_secs(CONNECT_TIMEOUT),
    )
    .map_err(|e| format!("Connection failed (is mistralrs-server running?): {e}"))?;

    stream
        .set_read_timeout(Some(Duration::from_secs(READ_TIMEOUT)))
        .map_err(|e| format!("Failed to set read timeout: {e}"))?;

    // Send HTTP/1.1 request. The server may respond with chunked transfer
    // encoding, which interleaves hex chunk-size lines with the SSE data.
    // The SSE parser skips non-`data:` lines, which naturally ignores the
    // chunk framing. We strip chunk size lines explicitly for clarity.
    let request = format!(
        "POST {API_PATH} HTTP/1.1\r\n\
         Host: {SERVER_ADDR}\r\n\
         Content-Type: application/json\r\n\
         Content-Length: {}\r\n\
         Accept: text/event-stream\r\n\
         \r\n\
         {body_str}",
        body_str.len()
    );

    stream
        .write_all(request.as_bytes())
        .map_err(|e| format!("Failed to send request: {e}"))?;

    // Read response — skip HTTP headers, then parse SSE body
    let reader = BufReader::new(stream);
    let mut lines = reader.lines();

    // Read status line
    let status_line = lines
        .next()
        .ok_or("No response from server")?
        .map_err(|e| format!("Failed to read status: {e}"))?;

    if !status_line.contains("200") {
        return Err(format!("Server returned: {status_line}"));
    }

    // Skip response headers (read until empty line)
    for line_result in lines.by_ref() {
        let line = line_result.map_err(|e| format!("Header read error: {e}"))?;
        if line.is_empty() {
            break;
        }
    }

    // Now parse the SSE body
    parse_sse_stream(lines, tx)
}

/// Parse an SSE stream and send `LlmOutput` events.
///
/// Accepts an iterator of line results for flexibility: the live code passes
/// `BufReader::lines()` from a `TcpStream`, while tests pass lines from a
/// `Cursor<&[u8]>`. This decoupling lets us test SSE parsing without a
/// network connection.
fn parse_sse_stream(
    lines: impl Iterator<Item = std::io::Result<String>>,
    tx: &Sender<LlmOutput>,
) -> Result<(), String> {
    let mut parser = ChunkParser::new();
    let mut token_count: u32 = 0;

    for line_result in lines {
        let line = line_result.map_err(|e| format!("Stream read error: {e}"))?;

        // SSE format: empty lines are event separators, skip them
        if line.is_empty() {
            continue;
        }

        // Skip SSE comments
        if line.starts_with(':') {
            continue;
        }

        // Extract data payload — also skip chunked encoding size lines
        // (hex numbers like "4e", "0") which don't start with "data: "
        let Some(data) = line.strip_prefix("data: ") else {
            log::debug!("skip non-data line: {}", &line[..line.len().min(60)]);
            continue;
        };

        // End of stream signal
        if data == "[DONE]" {
            info!("SSE stream done, {token_count} tokens received");
            for output in parser.flush() {
                let _ = tx.send(output);
            }
            return Ok(());
        }

        // Parse JSON payload
        let json: serde_json::Value =
            serde_json::from_str(data).map_err(|e| format!("JSON parse error: {e}"))?;

        // Check for finish_reason
        if let Some(reason) = json["choices"][0]["finish_reason"].as_str()
            && (reason == "stop" || reason == "length")
        {
            info!("Finish reason: {reason}, {token_count} tokens");
            for output in parser.flush() {
                let _ = tx.send(output);
            }
            return Ok(());
        }

        // Extract content delta
        let Some(content) = json["choices"][0]["delta"]["content"].as_str() else {
            continue;
        };

        if content.is_empty() {
            continue;
        }

        token_count += 1;

        // Feed through ChunkParser
        for output in parser.feed(content) {
            if tx.send(output).is_err() {
                // Receiver dropped (TUI quit) — exit cleanly
                info!("Channel closed, stopping stream");
                return Ok(());
            }
        }
    }

    // Stream ended without [DONE] — flush remaining
    info!("Stream EOF, {token_count} tokens received");
    for output in parser.flush() {
        let _ = tx.send(output);
    }
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

    #[test]
    fn parse_sse_stream_basic() {
        let sse_data = "\
data: {\"choices\":[{\"delta\":{\"content\":\"Hello.\"}}]}\n\
\n\
data: {\"choices\":[{\"delta\":{\"content\":\" [5] \"}}]}\n\
\n\
data: {\"choices\":[{\"delta\":{\"content\":\"World.\"}}]}\n\
\n\
data: {\"choices\":[{\"finish_reason\":\"stop\",\"delta\":{\"content\":\"\"}}]}\n\
\n\
data: [DONE]\n";

        let reader = BufReader::new(std::io::Cursor::new(sse_data.as_bytes()));
        let (tx, rx) = std::sync::mpsc::channel();
        parse_sse_stream(reader.lines(), &tx).unwrap();
        drop(tx);

        let outputs: Vec<LlmOutput> = rx.iter().collect();
        assert_eq!(
            outputs,
            vec![
                LlmOutput::Sentence("Hello.".into()),
                LlmOutput::Pause(5.0),
                LlmOutput::Sentence("World.".into()),
                LlmOutput::Done,
            ]
        );
    }

    #[test]
    fn parse_sse_stream_eof_without_done() {
        let sse_data = "\
data: {\"choices\":[{\"delta\":{\"content\":\"Partial text\"}}]}\n\
\n";

        let reader = BufReader::new(std::io::Cursor::new(sse_data.as_bytes()));
        let (tx, rx) = std::sync::mpsc::channel();
        parse_sse_stream(reader.lines(), &tx).unwrap();
        drop(tx);

        let outputs: Vec<LlmOutput> = rx.iter().collect();
        assert_eq!(
            outputs,
            vec![LlmOutput::Sentence("Partial text".into()), LlmOutput::Done,]
        );
    }
}
