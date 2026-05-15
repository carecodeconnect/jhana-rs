# 16: Agent Loop Design (Rust-native)

This doc captures the design of `src/agent_loop.rs` — the agent harness
that replaces the abandoned pi/Node experiment. The previous design
documents (`10_SPECS.md`, `15_INTERACTION.md`) assumed pi as the
orchestrator; this file supersedes the harness section of both and is
the source of truth going forward. NCF interaction principles in
`15_INTERACTION.md` still hold — what changes is *what dispatches the
tool calls*, not what tools mean.

## Goals and constraints

- **In-process.** No HTTP, no Node, no subprocess overhead. The agent
  loop calls `rkllm_rs::LLMHandle::run()` directly and dispatches tool
  calls into the existing `tts.rs` / `stt.rs` modules with regular
  function calls.
- **Fixed tool catalog of 6.** `say`, `listen`, `ring_bell`, `pause`,
  `list_meditations`, `read_meditation`. No `bash`, no `read`/`write`,
  no plug-in surface — the model only ever sees tools we want it to
  call.

  > **NCF:** A small, fixed catalog is itself an NCF design choice. In CA
  > terms, the tool definitions form the *affordances* the agent
  > perceives in the situation — what kinds of actions are possible.
  > Pi exposed `bash`, `read`, `write`, `edit` so a coding agent could
  > infer "I can edit files." A meditation agent should infer "I can
  > speak, listen, ring a bell, and pause" — nothing else. Restricting
  > the affordance surface is how we keep the agent on-genre.
- **~150 LoC budget.** Counting the new types, the loop, and the
  dispatch match. About half of that lifts from existing code in
  `src/bin/jhana-llm-server.rs` and `src/bin/qwen-tool-test.rs` via
  a shared `src/openai_types.rs` module.
- **Streams to ratatui.** Each interesting state change (a complete
  sentence, a tool dispatching, a tool returning) goes through an
  `mpsc::Sender<AgentEvent>` to the existing TUI thread — matches the
  pattern already established by `src/llm.rs`.

## Influences (what we're borrowing, with file refs)

Three layers of design lineage. The **inner loop** comes from the Rust
agent-framework survey (rig / goose / vox / kalosm); the **interaction
model** comes from Natural Conversation Framework + Conversation
Analysis; the **wider product feel** ("system that does things, with
a live canvas, with heartbeats") comes from OpenClaw and the tool-as-
affordance discipline from pi.

### Rust agent harnesses

| From | What | File reference |
|---|---|---|
| **rig** | The loop body shape — `loop { call_model → partition into (tool_calls, texts) → if no tool_calls break → else dispatch + push tool_result → continue }`, bounded by `max_turns`. | `0xPlaygrounds/rig` → `crates/rig-core/src/agent/prompt_request/mod.rs:390-560` |
| **rig** | The `Tool::call(args) -> output` minimum signature. We don't take the trait — seven tools fit in a `match` — but the call shape is right. | `crates/rig-core/src/tool/mod.rs` |
| **goose** | The `AgentEvent` enum streamed to the consumer. Gives ratatui backpressure and lets us show "tool dispatching..." in the console pane. | `block/goose` → `crates/goose/src/agents/agent.rs:185, 1227` |
| **goose** | On `max_turns` reached, yield a graceful assistant message instead of erroring. A meditation guide should never crash mid-session. | same file, line 1510 |
| **vox** | `SentenceBuffer` — handles "Dr." / "Mr." / decimal points correctly. Worth porting verbatim; our current `llm.rs` splits naively on `.`. | `mrtozner/vox` → `src/streaming_chat.rs:38-110` |
| **kalosm** | The flat `ChatMessage { role, content, tool_calls, tool_call_id }` shape, which is also the OpenAI shape, which is also already what `jhana-llm-server` speaks. | `floneum/floneum` → `interfaces/language-model/src/chat/mod.rs` |

### OpenClaw (https://openclaw.ai/, https://github.com/openclaw/openclaw)

The product framing — "AI that actually does things" — informs the
*shape* of our TUI more than the code. OpenClaw is a multi-channel
local assistant; the design ideas worth borrowing:

- **"AI that actually does things"** (`VISION.md` line 1) — our TUI
  prioritises visible doing-ness over conversational chatter. Every
  tool dispatch surfaces as a TUI event so the user sees the agent
  act, not just speak.
- **"It can render a live Canvas you control"** (`README.md` line 11)
  — the agent updates a real-time display surface. For jhana-rs, the
  meditation pane *is* the canvas: it re-renders based on what tool
  is currently active (say-text appearing large, listen showing a
  recording meter, pause showing a countdown). See `docs/14_TODO.md`
  TUI redesign task.
- **Heartbeats / proactive check-ins** — agent communicates state
  periodically, not just on user input. Maps onto our
  `AgentEvent::ToolStart` → console pane: the user can see the agent
  is alive between tool dispatches.
- **Channels concept** — voice channel (paroli + SenseVoice) is the
  *primary* interaction medium; the TUI is a mirror, not the entry
  point. We deliberately don't accept typed input — the device runs
  on a tty with no shell, voice only.

What we don't take from OpenClaw: the multi-channel messaging
architecture (we have one device, one user, one channel), the
plugin-as-npm distribution model (we're a single Rust binary), the
heavy onboarding wizard (the meditation guide should just work).

### pi (https://pi.dev, https://github.com/earendil-works/pi-mono)

We pivoted away from pi as our harness (see `docs/10_SPECS.md §
Outcome`) but the design lessons stay:

- **Tool catalog as affordances.** What's *callable* defines what's
  *possible*. Pi gives the model `read/write/edit/bash` because it's
  a coding agent. We give the model `say/listen/ring_bell/pause/...`
  because it's a meditation guide. Restricting the affordance
  surface is how an agent stays on-genre — see the NCF callout in
  the Tool catalog section below.
- **Tools as first-class structured objects.** Pi's `ToolDefinition`
  shape — `name + description + parameters + run()` — maps cleanly
  onto our `Tool` enum + `ToolDef` JSON-Schema for the model + match-
  arm dispatcher. We don't take pi's TypeScript-extension boundary
  (we're in-process Rust), but the *interface shape* is the same.
- **Skill vs extension distinction** (`pi_sandbox/docs/10-extensions.md`
  line 1-12) — a skill is markdown-as-instructions the model loads on
  demand; an extension is code-as-runtime-capability. We currently
  collapse both into the 7-tool catalog, with `read_meditation` being
  the closest thing to a skill (markdown loaded on demand). Future
  tool growth might re-split this.

### Natural Conversation Framework (Robert J. Moore, IBM Research)

Detailed in `docs/15_INTERACTION.md`. NCF applies findings from
Conversation Analysis (Sacks, Schegloff, Jefferson) to conversational
AI. For us, NCF isn't flavouring — it's the **spec**. Every tool
dispatch decision and every prompt instruction maps onto a CA
primitive:

- **Adjacency pairs** (say/listen) — first-pair-part projects the
  shape of the second-pair-part.
- **Pre-sequences** ("are you ready?" before "close your eyes") —
  warm the conversation before the main action.
- **Repair** — recognise "sorry?", "what?", silence on listen()
  output and redo the previous turn.
- **Preference organisation** — accept dispreferred responses
  gracefully; never push back on a "no".
- **Sequence closing** — "Opening Up Closings" (Sacks/Schegloff/Jefferson
  1973): closings are co-constructed, not unilateral. Today's
  `goodnight()` is unilateral; see `docs/14_TODO.md` for the
  collaborative-closing follow-up.
- **Turn-taking systematics** (S/S/J 1974) — TRPs (transition-relevance
  places), backchanneling, barge-in. The prompt-level "listen every
  2-3 says" mandate is a workaround; true turn-taking infrastructure
  is in `docs/14_TODO.md`.

## Explicitly NOT borrowing

- **rig's typestate** (`PromptRequest<Standard|Extended, M, P>`,
  `PhantomData`, `PromptHook`). All of it exists to support a generic
  multi-provider framework. We have one provider (rkllm), one model
  (Qwen3-1.7B), one purpose.
- **goose's MCP host, `ExtensionManager`, `PermissionManager`,
  `RetryManager`, `compact_messages`, `subagent_handler`,
  `ToolConfirmationRouter`.** Goose is a production multi-agent coding
  harness; the surface area is 30× our budget.
- **kalosm's `ModelConstraints` / `StructuredChatModel`** — grammar-
  constrained sampling. Useful for forcing strict JSON outputs but
  Qwen3 produces clean `<tool_call>` blocks without it; parsing the raw
  text is more flexible.
- **candle's `LogitsProcessor` and sampler.** rkllm-rs does sampling on
  the NPU; we don't need a Rust-side sampler.
- **`tokio` / `async`.** The agent loop is single-threaded synchronous.
  Adding tokio just to use `BoxStream` is not worth the dep tax.

## Data types

All defined in `src/openai_types.rs` (new shared module — see Phase 3b
below) so the agent loop and the HTTP shim use the same shapes.

```rust
pub enum Role { System, User, Assistant, Tool }

pub struct ChatMessage {
    pub role: Role,
    pub content: String,                 // empty on pure tool_call turns
    pub tool_calls: Vec<ToolCall>,       // on Assistant turns only
    pub tool_call_id: Option<String>,    // on Tool turns only
}

pub struct ToolCall {
    pub id: String,                      // "call_<uuid>"
    pub name: String,
    pub arguments: serde_json::Value,
}

/// Events streamed to the TUI thread.
pub enum AgentEvent {
    Sentence(String),                    // assistant prose, ready for TTS + UI
    ToolStart { name: String, args: serde_json::Value },
    ToolResult { name: String, ok: bool, snippet: String },
    Done,
    Error(String),
}
```

`Role` and `ChatMessage` serialise to the OpenAI wire shape so the same
type set works for the HTTP shim's request/response parsing.

## Tool catalog

Fixed `enum`, dispatched in one `match`. No trait, no boxed `dyn`.

```rust
pub enum Tool {
    Say,                  // { text: String }            → tts::Speak
    Listen,               // { seconds?: u32 = 7 }       → stt::listen_blocking
    RingBell,             // {}                          → tts::Bell
    Pause,                // { seconds: f32 }            → thread::sleep
    ListMeditations,      // {}                          → fs::read_dir(prompts/meditations/)
    ReadMeditation,       // { name: String }            → fs::read_to_string
}
```

> **NCF — the say/listen pair as the unit of dialogue.** `say` followed
> by `listen` is the implementation of an *adjacency pair* — the
> first-pair-part (agent's question, request, invitation) projects
> the constrained second-pair-part (user's answer, acceptance,
> refusal). Almost every NCF sequence is built from chains of these.
>
> **`say` blocks until TTS playback finishes — deliberately.** CA's
> turn-taking rule is *one speaker at a time*. If `say` returned
> before paroli finished synthesising and PA finished playing, the
> agent could call `listen` while it was still mid-sentence — that's
> turn-overlap, which violates the model's understanding of "I have
> spoken, now you speak." The synchronous semantics aren't a
> performance compromise; they're an NCF correctness requirement.
>
> **`pause` is not a no-op.** CA treats silence as *meaningful* — a
> 5-second pause projects "now is your turn to breathe", not "the
> agent isn't paying attention." Separating `pause` from `say` lets
> the model schedule structured silences rather than padding spoken
> turns with `... ... ...`. The model decides *when* silence happens,
> what kind of silence (breath-paced, reflective, closing), and how
> long.
>
> **`ring_bell` is a phatic / framing turn.** In CA, certain short
> tokens ("okay", "right", a click) signal *transitions between
> sequences* without being part of the substantive content. The
> meditation bell does that job — it punctuates phase boundaries
> (opening / closing). The model is free to decide when to use it as
> a frame, but it's not part of the conversation's information flow.

```rust
impl Tool {
    /// Map model-emitted tool name → enum. Unknown names are an error
    /// rather than a silent no-op; we want the LLM to learn from the
    /// failure ("unknown tool: X") rather than think it succeeded.
    fn from_name(name: &str) -> Option<Tool> { /* match */ }

    fn name(&self) -> &'static str { /* match */ }

    /// JSON-Schema-ish definition emitted into the Qwen3 system prompt
    /// so the model knows the tool exists.
    fn definition(&self) -> serde_json::Value { /* match */ }
}
```

All six tools' schemas are static — no runtime configuration — so
`definition()` can return a `Value` built from a `serde_json::json!`
literal per arm.

## The loop

```text
fn run_agent(handle, history, events, cancel, max_turns) -> Result<(), AgentError>:

  for turn in 0..max_turns:
      if cancel.load(): return Err(Cancelled)

      prompt = render_chatml(history, ALL_TOOLS)
      sentence_buf = SentenceBuffer::new()
      raw = run_completion(handle, prompt, |tok| {
          for s in sentence_buf.push(tok):
              events.send(AgentEvent::Sentence(s))
      })

      // flush any trailing partial sentence
      if let Some(last) = sentence_buf.finish():
          events.send(AgentEvent::Sentence(last))

      (visible_text, calls) = parse_assistant(raw)
      history.push(Assistant { content: visible_text, tool_calls: calls })

      if calls.is_empty():
          events.send(AgentEvent::Done)
          return Ok(())

      for call in calls:
          events.send(AgentEvent::ToolStart { name, args })
          match dispatch_tool(call):
              Ok(result):
                  events.send(AgentEvent::ToolResult { ok: true, ... })
                  history.push(Tool { tool_call_id: call.id, content: result })
              Err(e):
                  events.send(AgentEvent::ToolResult { ok: false, ... })
                  history.push(Tool { tool_call_id: call.id, content: error_string(e) })

  // max_turns hit — yield a graceful close, don't error
  events.send(AgentEvent::Sentence("Let's pause here. Thank you for sitting."))
  events.send(AgentEvent::Done)
  Ok(())
```

About 30 LoC of actual Rust once `?`-propagation tightens it up.

> **NCF — the loop is a sequence of turns, terminated by a closing.**
> Two CA patterns are baked in:
>
> 1. **The break condition `if calls.is_empty(): Done`** maps directly
>    onto CA's *turn-allocation*: the agent has finished its turn and
>    projects no follow-up action, so the sequence is complete.
>    Without tool calls there's nothing further for the agent to do,
>    and we hand back to the user (or close, if it's the final turn).
> 2. **The graceful close on `max_turns`** is a *sequence-closing
>    sequence* — CA's term for the wind-down that participants
>    co-construct at the end of an interaction ("okay…", "well…",
>    "thanks for…"). We synthesise one rather than crashing because
>    abrupt termination is itself a turn — and a hostile one. The
>    placeholder `"Let's pause here. Thank you for sitting."` is a
>    pre-closing token; the model can override it with a richer one if
>    its own emitted close was richer.

## Streaming versus turn-level

Tokens stream **inside one turn** for live sentence-by-sentence TTS — the
`run_completion` callback fires per token, `SentenceBuffer` accumulates,
each complete sentence goes immediately to TTS via `AgentEvent::Sentence`.

Tool calls dispatch **at turn boundaries** — they only appear after the
model finishes its generation for that turn. This matches how every
function-calling model works (the `<tool_call>` block is part of the
output; you can't dispatch it mid-token-stream).

> **NCF — sentence-level streaming preserves TCU boundaries.** CA's
> *turn-construction unit* (TCU) is the building block of a turn — a
> complete phrase or clause that could in principle end the turn. By
> streaming TTS one full sentence at a time (not one token, not one
> whole-turn block), the agent's voice respects prosodic phrase
> boundaries: a listener can interject at a TCU boundary without
> talking over the agent. Token-level streaming would garble prosody;
> turn-level batching would delay all speech until generation ends.
> Sentence-level is the sweet spot. (vox's `SentenceBuffer` is what
> makes this robust — see Influences table.)

## Cancellation

`std::sync::atomic::AtomicBool` shared with the main loop. Checked at
the top of every turn. The user pressing BACK on the hardware flips it;
the agent finishes whatever sentence/tool is in flight and returns
`Err(Cancelled)`. We don't try to cancel mid-token — rkllm-rs's blocking
`run()` doesn't expose interrupt, and a half-spoken sentence is worse
UX than letting it finish.

> **NCF — letting in-flight turns complete is the right CA stance.**
> Cutting off a TCU mid-word violates the listener's expectation of
> turn-completeness. A user who presses BACK is initiating
> *withdrawal* from the conversation, but CA distinguishes polite
> withdrawal (let the current TCU finish, then close) from rude
> withdrawal (hang up mid-sentence). We default to polite: the agent
> finishes its sentence and exits at the next turn boundary.
>
> *Future:* we may want a distinction between BACK (polite withdrawal,
> let agent close gracefully) and a HARD-STOP gesture (e.g. holding
> the button — immediate halt). For v1, a single graceful path covers
> 99% of real use.

## How the existing pipeline rewires

```
                  before                                           after
┌────────────────────────────────┐                ┌────────────────────────────────────┐
│  main.rs                       │                │  main.rs                           │
│   spawns:                      │                │   spawns:                          │
│    - stt thread                │                │    - stt thread (unchanged)        │
│    - tts thread                │       →        │    - tts thread (unchanged)        │
│    - llm thread (start_stream) │                │    - agent thread (run_agent)      │
│   pipes LlmOutput → TTS        │                │   pipes AgentEvent → TTS+UI        │
└────────────────────────────────┘                └────────────────────────────────────┘
```

`src/llm.rs` keeps `get_or_load_model()` and the rkllm callback handler;
its `start_streaming` + `ChunkParser` get *replaced* by the agent loop
calling rkllm directly with a fresh callback per turn. `ChunkParser` is
gone — its `[BELL]` / `[N]` inline-marker logic doesn't apply when the
model emits structured `<tool_call>` blocks instead.

`src/main.rs` changes are minimal: instead of `llm::start_streaming(...)`
it calls `agent_loop::run_agent(...)`. The TUI's `LlmOutput`-handling
match arms get renamed to handle `AgentEvent` variants but the visual
behaviour is the same — sentences appear in the meditation pane, tool
calls appear in the console pane.

## Prompt and few-shot changes

The system prompt and meditation few-shots get rewritten for the
tool-driven world — same general idea as before, different output
format:

- `prompts/system.txt`: instruct the model to use tools. Start every
  session with `say("Hello?")` then `listen()`. Don't run a meditation
  unprompted.
- `prompts/meditations/*.txt`: rewrite from `[BELL]` / `[N]` inline
  markers to tool-call traces: e.g.
  `say("Close your eyes.")` `pause(15)` `ring_bell()`.

This is a separate commit from the agent loop itself — prompt work
follows runtime work because we want to test the runtime in isolation
first with a trivial system prompt before tuning the meditation
behaviour.

> **NCF — the prompt is where the conversation grammar lives.** The
> tool definitions tell the model *what's possible* (affordances);
> the system prompt + few-shots tell it *how to combine them
> conversationally*. Specifically, the prompt should encode:
>
> - **Summons-answer**: open every session with `say("Hello?")` +
>   `listen()`. The ENTER button is the doorbell; the agent's first
>   action is the answer-to-summons. (See
>   [15_INTERACTION.md § Summons-answer](./15_INTERACTION.md#summons-answer-how-a-session-opens).)
> - **Pre-sequence**: before starting a meditation, the agent asks
>   `"Are you ready?"` and waits. This is a *type 2 pre*, a
>   preparatory turn that tests the ground before the main action.
> - **Repair recognition**: when `listen()` returns "what?", "sorry?",
>   or an unintelligible string, the agent's next turn is a *redo*
>   of the previous turn, not a continuation. Repair-vocabulary list
>   lives in `15_INTERACTION.md` and gets surfaced in the system
>   prompt.
> - **Preference handling**: if the user declines a longer meditation
>   ("no thanks"), the agent accepts gracefully (`"Of course."`)
>   rather than pushing back. Dispreferred second-pair-parts get
>   mitigated, not contested.
> - **Closing**: the meditation ends on a recognisable closing form
>   (`say("May all beings everywhere be happy.")` →
>   `ring_bell()` → `pause(10)` → no more tool calls), not when
>   `max_new_tokens` runs out.
>
> Few-shot meditations should be written as *exemplars of the NCF
> sequence*, not as inline-marker prose. The model learns the grammar
> from the trace.

## Phases (this branch)

1. **3a — design doc**: this file. Done when committed.
2. **3b — shared types**: extract `ChatMessage`, `ToolCall`, etc. from
   `src/bin/jhana-llm-server.rs` into `src/openai_types.rs`. Update the
   shim to use them. Pure refactor; verify build + the existing
   `tool-call-test`-style curl still passes.
3. **3c — agent loop**: write `src/agent_loop.rs` against these types,
   wire it into `src/main.rs` behind a feature flag or a config flag
   (`active_harness: "ratatui" | "agent"`) so we can switch back if
   something breaks. New tools dispatch to existing `stt::*` / `tts::*`
   functions.
4. **3d — prompts**: rewrite system prompt + few-shots for tool calls.
5. **3e — end-to-end test on Rock**: bring up the agent harness, press
   ENTER, verify the model says hello + listens + can run a small
   meditation through tool calls.

3b–3e land as separate commits. The fallback path (ratatui + inline
markers + Llama-3.2-3B) stays intact and selectable via
`config/jhana.json` throughout.
