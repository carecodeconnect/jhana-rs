//! OpenAI-shape chat types shared between the HTTP shim
//! (`src/bin/jhana-llm-server.rs`) and the in-process agent loop
//! (`src/agent_loop.rs`).
//!
//! Internally we use a flat `String` content; the OpenAI wire form
//! that allows `content` as `[{type:"text",text:"..."}, ...]` is
//! handled by `WireContent` at request deserialisation only, then
//! flattened to a `String` so downstream code (prompt rendering,
//! agent loop, history serialisation) never has to branch on shape.

use serde::{Deserialize, Serialize};
use serde_json::Value;

/// Conversation role. Serialises to lowercase per OpenAI.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum Role {
    System,
    User,
    Assistant,
    Tool,
}

/// Canonical chat message — used as the agent loop's history element
/// and as the post-deserialisation shape inside the HTTP shim.
///
/// `content` is empty when an assistant turn carried only tool calls;
/// `tool_calls` is empty on user/system/tool turns; `tool_call_id` is
/// set on `Role::Tool` turns to pair the result with the originating
/// call.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ChatMessage {
    pub role: Role,
    #[serde(default, skip_serializing_if = "String::is_empty")]
    pub content: String,
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub tool_calls: Vec<ToolCall>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub tool_call_id: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub name: Option<String>,
}

impl ChatMessage {
    pub fn system(content: impl Into<String>) -> Self {
        Self::new(Role::System, content)
    }
    pub fn user(content: impl Into<String>) -> Self {
        Self::new(Role::User, content)
    }
    pub fn assistant_text(content: impl Into<String>) -> Self {
        Self::new(Role::Assistant, content)
    }
    pub fn assistant_with_calls(content: impl Into<String>, tool_calls: Vec<ToolCall>) -> Self {
        Self {
            role: Role::Assistant,
            content: content.into(),
            tool_calls,
            tool_call_id: None,
            name: None,
        }
    }
    pub fn tool_result(tool_call_id: impl Into<String>, content: impl Into<String>) -> Self {
        Self {
            role: Role::Tool,
            content: content.into(),
            tool_calls: Vec::new(),
            tool_call_id: Some(tool_call_id.into()),
            name: None,
        }
    }

    fn new(role: Role, content: impl Into<String>) -> Self {
        Self {
            role,
            content: content.into(),
            tool_calls: Vec::new(),
            tool_call_id: None,
            name: None,
        }
    }
}

/// A tool call emitted by the model.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ToolCall {
    pub id: String,
    #[serde(rename = "type", default = "default_function_kind")]
    pub kind: String,
    pub function: ToolCallFunction,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ToolCallFunction {
    pub name: String,
    /// Stringified JSON of the call's arguments per OpenAI spec. Callers
    /// parse this themselves with `serde_json::from_str` against their
    /// own argument type.
    pub arguments: String,
}

fn default_function_kind() -> String {
    "function".to_string()
}

/// A tool definition declared to the model. Same shape OpenAI uses
/// for the `tools[]` field of a chat-completion request.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ToolDef {
    #[serde(rename = "type")]
    pub kind: String,
    pub function: ToolFunction,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ToolFunction {
    pub name: String,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub description: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub parameters: Option<Value>,
}

/// Wire-form content: OpenAI accepts either a plain string or an array
/// of content parts (`[{"type":"text","text":"..."}, ...]`). The HTTP
/// shim uses this on request deserialisation only; everything past
/// the wire boundary operates on flat `String`s via `as_text()`.
#[derive(Debug, Deserialize)]
#[serde(untagged)]
pub enum WireContent {
    Text(String),
    Parts(Vec<ContentPart>),
}

#[derive(Debug, Deserialize)]
pub struct ContentPart {
    #[serde(rename = "type")]
    pub kind: String,
    #[serde(default)]
    pub text: Option<String>,
}

impl WireContent {
    /// Flatten to a single string. Array form concatenates all `text`
    /// parts and drops anything else (images, audio, tool_use blocks).
    /// Fine for a text-only model like Qwen3-1.7B.
    pub fn as_text(&self) -> String {
        match self {
            Self::Text(s) => s.clone(),
            Self::Parts(parts) => parts
                .iter()
                .filter(|p| p.kind == "text")
                .filter_map(|p| p.text.as_deref())
                .collect::<Vec<_>>()
                .join(""),
        }
    }
}
