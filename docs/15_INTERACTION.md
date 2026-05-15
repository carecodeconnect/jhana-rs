# 15: Interaction Design (Natural Conversation Framework)

This doc captures the **concrete tool catalog and conversation patterns**
the agent uses to drive a meditation session. The theoretical grounding —
why NCF, what Conversation Analysis gives us, the CA primitives — lives
in [`10_SPECS.md` § Interaction design](./10_SPECS.md#interaction-design-moores-natural-conversation-framework).
This doc is the *implementation plan* derived from that.

The design target: a meditation that *feels like a real conversation*,
not a monologue. The user never types. The model drives turn-taking
using a small catalog of structured tool calls, executed by the
[`jhana-pi-extension`](#tool-catalog) TypeScript extension that pi
loads at startup.

## How the layers fit

> **History note:** an earlier draft of this doc routed everything through
> pi (Node) + `jhana-llm-server` (HTTP shim). We pivoted to a bespoke
> Rust agent loop after pi proved a poor fit for a meditation guide
> (see [10_SPECS.md § pi as agent harness — Outcome](./10_SPECS.md#outcome-2026-05-15-pivoted-to-bespoke-rust-agent-loop)).
> The HTTP shim stays available as a debug endpoint but isn't on the hot
> path any more.

```
┌─────────────────────────────────────────────────────────────┐
│  jhana-rs (single Rust binary, ratatui TUI on tty1)         │
│                                                             │
│  ┌──────────────────────────────────────────────────────┐   │
│  │ agent_loop.rs (NEW)                                  │   │
│  │   - holds chat history (Vec<ChatMessage>)            │   │
│  │   - calls llm::run_with_tools()                      │   │
│  │   - parses <tool_call> blocks from raw output        │   │
│  │   - dispatches to existing modules:                  │   │
│  │       • say        → tts::Speak                      │   │
│  │       • listen     → stt::listen_blocking            │   │
│  │       • ring_bell  → tts::Bell                       │   │
│  │       • pause      → std::thread::sleep              │   │
│  │   - appends tool_result message, loops               │   │
│  └──────────────────────────────────────────────────────┘   │
│        ▲                                                    │
│        │ in-process function calls — no HTTP, no Node       │
│        ▼                                                    │
│  llm.rs / stt.rs / tts.rs / ui.rs (already exist)           │
└─────────────────────────────────────────────────────────────┘
```

The model issues a `<tool_call>` block in its raw output → the agent
loop extracts and dispatches it → the result is appended to the chat
history as a `tool` role message → another LLM turn runs with that
context. All Rust, single binary, exact tool catalog (no surprise
`bash`).

## Tool catalog

Each tool is a `ToolDefinition` registered by `jhana-pi-extension/jhana.ts`.
Names and shapes are stable contracts — once shipped, renaming or
re-shaping is a breaking change for any prompts that reference them.

| Tool | Purpose | Arguments | Returns |
|---|---|---|---|
| `say` | Speak a sentence to the user. Blocks until TTS playback finishes so the agent's "speaker turn" is genuinely sequential. | `{ text: string }` | `{ status: "spoken" }` |
| `listen` | Open the mic, record for `seconds`, transcribe via SenseVoice. Plays the cached "Speak now" cue first. | `{ seconds?: number }` (default 7) | `{ transcript: string, confidence?: number }` |
| `ring_bell` | Play the meditation bell tone once. | `{}` | `{ status: "rung" }` |
| `pause` | Silent gap. Just sleeps — no audio. The model uses this to space breaths. | `{ seconds: number }` | `{ status: "paused" }` |
| `list_meditations` | Return the names of available meditation templates from `prompts/meditations/`. | `{}` | `{ names: string[] }` |
| `read_meditation` | Return the body of a template so the model can use it as a few-shot example. | `{ name: string }` | `{ body: string }` |

## Voice registers: system vs agent

A core CA-aligned UX decision: **the user should hear two distinct
voices**, and *who is speaking* maps onto *what role they occupy*.

| Voice | Engine | Used for | CA role |
|---|---|---|---|
| **System** | espeak-ng (deliberately robotic) | Boot status: "loading", "ready", brief cues like "listening" | Operator / announcer — *about* the conversation, not part of it |
| **Agent** | paroli (NPU, warm, natural) | Everything the meditation guide says: greetings, instructions, meditations, repair turns | Speaker in the conversation — *Jhana herself* |

The distinction matters because in CA every utterance is *attributable*.
If the same voice tells you the system is loading AND guides you
through a meditation, you cannot tell whether the device is *announcing*
or *engaging*. Two voices = unambiguous attribution.

Implementation note: the engine swap is per-utterance, not global. Today
the TTS thread reads `tts.engine` from `config/jhana.json` once. To
support per-utterance register, `TtsCommand::Speak` gets a `register`
field (or split into `Speak` vs `Announce`), and `Announce` always
routes to espeak-ng regardless of the configured agent engine.

## Summons-answer: how a session opens

The hardware ENTER button is **not** a "begin meditation" trigger. In
CA terms it's a **summons** — equivalent to ringing a doorbell, picking
up a phone receiver, knocking on a door. A summons projects an
*answer-to-summons*, which is always the same form: an availability
display (`"Hello?"`, `"Yes?"`).

```
user:    [presses ENTER]               # summons
system:  "ready"                       # operator confirms line is open
agent:   say("Hello?")                 # answer-to-summons — Jhana available
user:    "hi"                          # return-greeting completes the pair
agent:   say("Hi, would you like to do a meditation today?")  # first-pair-part
user:    "yes please"
agent:   say("Great. How long do you have?")
...
```

The agent **never** opens with a meditation. She opens with
availability and waits. Only after the user has reciprocated does she
project a first-pair-part of her own ("would you like to..."). This is
how real conversations begin and it's also a robustness feature: if
the user pressed ENTER by accident, an unanswered `"Hello?"` costs
nothing; an unattended meditation that auto-rings the bell and pauses
for 30 seconds is jarring.

Concretely, this means the **first agent utterance is a tool call by
the model**, not a hardcoded welcome line. The system prompt instructs
the model: "When the session starts, your first action is `say('Hello?')`
followed by `listen()`. Do not begin a meditation until the user has
reciprocated and indicated they want one."

The old hardcoded `ui.welcome_lines` in `config/jhana.json` collapses
to a single system-voice announcement (`"ready"` or a brief chime) and
everything else moves into the model's tool-call flow.

## Conversation Analysis primitives → tool sequences

NCF builds on CA's units of dialogue. Each primitive translates into a
recognisable sequence of tool calls.

### Adjacency pair (question-answer, request-acceptance)

The unit of NCF. A first-pair-part *projects* a constrained second-pair-part.

```
model: say("How long would you like to meditate?")
model: listen({ seconds: 5 })   → { transcript: "ten minutes" }
model: say("Ten minutes. Let's begin.")
```

### Pre-sequence

A turn that *warms up* before the main action — checks the ground.

```
model: say("Are you ready to begin?")
model: listen()                  → { transcript: "yes" }
model: say("Close your eyes.")
model: ring_bell()
model: pause({ seconds: 30 })
```

The pre-sequence (`Are you ready?`) is critical: it converts the
session from "agent ambushes user with a bell" to "agent invites user
to opt in." This is the difference between a chatbot and a guide.

### Repair (other-initiated)

The user signals they didn't catch the previous turn. The agent
recognises the repair-initiator and re-does the last turn, optionally
clarified or louder/slower.

```
model: say("Bring your attention to the sensation of breath at your nostrils.")
model: listen()                  → { transcript: "sorry?" }
# Model detects "sorry?" as a repair-initiator (NCF repair vocabulary list)
model: say("Notice the breath where it enters your nose.")
```

Repair-initiator vocabulary the model is prompted to recognise:
`"what?"`, `"sorry?"`, `"can you repeat?"`, `"say again"`, `"huh?"`,
silence (`transcript === ""`), unintelligible (`transcript` is noise).
Implementation: prompt-level for v1; could be promoted to a strict
post-processor on `listen()` results if the model under-recognises.

### Preference organisation

Some second-pair-parts are *preferred* (accept), others *dispreferred*
(refuse). Dispreferred turns are mitigated, delayed, accounted for.

```
model: say("Would you like to extend to fifteen minutes?")
model: listen()                  → { transcript: "no, ten is enough" }
# "no" is dispreferred — model accepts gracefully, doesn't push back
model: say("Of course. Ten minutes it is.")
```

Counter-example to avoid:

```
# BAD
model: say("Would you like to extend?")
model: listen()                  → { transcript: "no" }
model: say("Are you sure? Studies show longer sessions...")  # ← pushback, breaks NCF
```

### Sequence closing

Sessions end on a recognised closing form, not when `max_new_tokens`
runs out.

```
model: say("May all beings everywhere be happy.")
model: ring_bell()
model: pause({ seconds: 10 })
model: say("Take a moment to notice how you feel.")
model: say("Whenever you're ready, open your eyes.")
# Model emits no further tool calls; pi's agent loop exits naturally
```

## Implementation phasing

The interaction design lands in three slices so each is testable on its own.

### Phase 4a — audio endpoints (Rust)

Add to `jhana-llm-server`:

- `POST /v1/audio/transcriptions` — accepts `{ seconds }`, records via
  the same `arecord` + `ffmpeg` + SenseVoice path `src/stt.rs` uses,
  returns `{ text }`. OpenAI shape so any client can hit it.
- `POST /v1/audio/speech` — accepts `{ input: string }`, synthesises
  via paroli (matching `src/tts.rs` engine selection), plays through
  PA, returns `{ status: "spoken" }`. We don't return the WAV bytes
  — playback happens server-side because the audio device is on the
  same machine. (A `?return_audio=1` query could be added later if a
  remote client ever needs the bytes.)
- `POST /tools/bell` — convenience: play the pre-rendered bell WAV.
- `POST /tools/pause` — `{ seconds }`; sleeps then 200 OK.

These reuse the existing Rust modules — no logic duplication, just
new HTTP routes wrapping the same `stt::*` / `tts::*` calls.

### Phase 4b — TypeScript extension

`./pi-extension/` directory in this repo:

- `package.json` with `pi.extensions: ["./jhana.ts"]` and a single
  dep on `@earendil-works/pi-coding-agent`.
- `jhana.ts` with the six tools from the catalog. Each is ~10 LoC of
  `fetch` to the local Rust server.
- Install with `pi install -l /home/ubuntu/jhana-rs/pi-extension`
  (path-local rather than published npm — this code is repo-coupled
  and not useful outside).

Total surface: ~80 LoC of TypeScript, no build step (pi has runtime TS).

### Phase 4c — prompt + few-shot

`prompts/system.txt` is rewritten for the tool-driven world:

- Removes the inline `[BELL]` / `[N]` instructions (those were the
  ratatui-era workaround).
- Tells the model: "You guide a meditation by calling tools. Begin
  every session with a pre-sequence. Use `pause()` between breaths.
  Ring the bell exactly at the start and end. Recognise repair
  initiators on `listen()` output and re-do the previous turn."
- Few-shot in `prompts/meditations/*.txt` is rewritten as
  *tool-call traces* rather than `[BELL]` / `[N]`-annotated prose. e.g.
  `say("Close your eyes.")` `pause(15)` `ring_bell()` etc.

The model learns the tool grammar from the few-shot. Pi sees structured
`tool_calls` in the response and dispatches them through the extension.
The ratatui flow on `main` is untouched — that branch keeps the inline
`[BELL]` / `[N]` mechanism. Only `pi-port` adopts the tool-driven design.

### Phase 4d — repair + preference handling (future)

Once the basic catalog is shipped, refine:

- Repair: capture `listen()` results with low confidence or matching
  the repair-vocab list, auto-route to a "re-do previous turn" prompt
  rather than relying on the model to recognise them.
- Preference: detect dispreferred replies (`"no"`, `"not really"`,
  `"actually I'd prefer..."`) and bias the next turn toward accepting
  + mitigating rather than persisting.
- Closing: a stricter end-of-session detector so we don't run past the
  natural close.

These are quality-of-life refinements — the core NCF pattern (pre-sequence
→ adjacency pairs → closing) works without them; they just make it
robust to model under-performance.

## Open questions

- **Barge-in.** Today the user can't interrupt the agent's `say()`
  mid-sentence. NCF treats overlap as legitimate; ours doesn't.
  Real fix is a half-duplex VAD on the mic while TTS is playing, but
  that's a Phase 6+ concern.
- **Multi-turn memory.** Pi keeps full conversation context per
  session; what should we *prune* so a 20-minute meditation doesn't
  blow past Qwen3's 4096-token context? Open.
- **Latency budget.** Each tool round-trip is: model → extension →
  HTTP → server → action → response. Sub-50 ms locally, but it adds
  up. Measure once Phase 4 is shippable.
