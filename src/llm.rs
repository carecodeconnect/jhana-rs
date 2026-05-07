//! LLM integration via llama-cpp-2 (llama.cpp Rust bindings).
//!
//! Wraps llama.cpp for on-device inference of GGUF models. llama-cpp-2 was
//! chosen over pure-Rust alternatives because llama.cpp has proven ARM NEON
//! SIMD optimizations and broad model support (Llama, Mistral, Qwen, etc.).
//!
//! # Model format
//!
//! All models must be in **GGUF** format. The original Orca Mini 3B on the
//! device is GGML v3 (`.ggmlv3.q4_0.bin`) which is no longer supported by
//! modern llama.cpp. A GGUF version must be downloaded to replace it.
//!
//! # Pause marker parsing
//!
//! The LLM is prompted to emit `[N]` markers (e.g. `[5]`, `[3.5]`) inline
//! with meditation text. A bracket state machine (ported from the Python
//! `jhana-dev/src/meditation_guide.py`) splits streaming tokens into
//! sentences and pause durations.

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
