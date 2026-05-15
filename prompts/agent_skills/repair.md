# Repair Sequences — Skill Reference

Read this with `read_skill("repair")` when you're unsure how to handle
a `listen()` reply that doesn't fit cleanly into "yes" / "no" /
substantive answer. Repair in CA terms is how a conversation fixes
mis-hearings, misunderstandings, and breakdowns — it's part of normal
talk, not a failure mode.

## Repair vocabulary on listen() output

The transcribed text from `listen()` is your evidence. Watch for these
signals — they are **repair-initiators** (Schegloff, Jefferson, Sacks 1977):

| Signal in transcript | Type | Your response |
|---|---|---|
| Empty / silence | Withholding | Wait a beat, repeat the previous turn slower |
| "What?" / "Sorry?" / "Huh?" | Generic repair-initiator | Repeat your last `say()` slower and clearer |
| "I didn't catch that" | Specific repair-initiator | Repeat your last `say()` |
| "Can you say that again?" | Specific repair-initiator | Repeat your last `say()` |
| "What do you mean?" | Understanding-check | Explain the previous turn in different words |
| Mostly punctuation / garbled | STT noise | Repeat your last `say()` |
| Affirmative but off-topic | Mis-fit | Acknowledge briefly, then re-orient |

## Pattern: redo, not advance

If you detect a repair-initiator, your next turn is a **redo of the
previous turn** — same content, slower, clearer wording — NOT a
continuation of the script.

```
agent: say("Bring your attention to the breath at the nostrils.")
agent: listen()
listen() → "sorry?"

# WRONG: continue
agent: say("Notice the rise and fall of the abdomen.")   # ← advances; user didn't hear the previous turn

# RIGHT: redo
agent: say("I said: bring your attention to the breath, where it touches the nostrils.")
agent: listen()
```

## Pattern: three strikes, graceful close

If three consecutive `listen()` calls return repair-initiators or
unintelligible output, the audio link is probably broken. Close the
session gracefully rather than persist:

```
agent: say("It seems we're having trouble hearing each other. Let's pause here for now.")
agent: say("Feel free to start again whenever you're ready.")
agent: ring_bell()
agent: goodnight()
```

## Pattern: clarification request from user

If the user asks for clarification on a meditation instruction
("what do you mean by 'soften the body'?"), give a concrete answer
in plain language, then return to the instruction:

```
listen() → "what do you mean by soften the body?"

agent: say("Let your shoulders drop, unclench your jaw. Just let go of any holding you notice.")
agent: pause(3)
agent: say("Try that now.")
agent: pause(10)
agent: say("How does that feel?")
agent: listen()
```

## Don't

- **Don't apologise for the model's mistakes**. The repair belongs to
  the conversation, not to a faulty agent. *"Sorry, my bad"* is jarring
  in a meditation context.
- **Don't repeat verbatim**. Each redo should be slightly clearer —
  shorter sentence, simpler vocabulary, slower pacing.
- **Don't advance the script** until the repair completes.
