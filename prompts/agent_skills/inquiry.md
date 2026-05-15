# Post-Meditation Inquiry — Skill Reference

Read this with `read_skill("inquiry")` AFTER a meditation closes but
BEFORE you call `goodnight()`. Based on Crane, Stanley, Rooney et al.
(2015), *"Disciplined Improvisation: Characteristics of Inquiry in
Mindfulness-Based Teaching"* ([Mindfulness 6:1104-1114](https://link.springer.com/article/10.1007/s12671-014-0361-8)).

Inquiry is the *teacher's response to the student's reported experience*.
It's a structured dialogue with three layers — start at Layer 1, only
move to Layers 2 and 3 if the user has settled into the first.

## Three Layers

### Layer 1 — Direct Experience

The teacher reflects what the user *noticed* in sensory or affective
terms. **No interpretation. No advice.** Use the user's own words.

```
agent: say("So, what did you notice?")
agent: listen(15)

listen() → "my shoulders kept feeling tight"

agent: say("Tight shoulders. You stayed with that for a while?")
agent: listen(10)
```

The first reflection just *receives* what the user said. Resist the
urge to fix, explain, or pivot.

### Layer 2 — Dialogue and Exploration

After the user has been received, you can open a *probe* — a specific
question shape that invites further noticing.

- "What's that like in the body?"
- "What came before that?"
- "Is there anything else there alongside it?"
- "Where do you feel it most strongly?"

```
agent: listen() → "my shoulders kept feeling tight"
agent: say("Tight shoulders. You stayed with that for a while?")
agent: listen() → "yeah it didn't really go away"
agent: say("What was it like to keep returning to that tightness?")
agent: listen(10)
```

Probes deepen the noticing without dictating what should be there.

### Layer 3 — Linking

Connect the noticed experience to the broader frame of practice and
daily life. Use this **only** when the user has settled into Layers 1
and 2 — never lead with it.

```
listen() → "it's the same tightness I always have when I'm stressed"

agent: say("So this is familiar. The same body, the same response.")
agent: pause(5)
agent: say("Knowing it well doesn't make it go away. But noticing it as it forms — that's the practice.")
agent: listen(10)
```

Layer 3 is a *gentle* link, not a lesson. The user often does the
linking themselves; the teacher just confirms.

## Opening the inquiry

Always open with the same first-pair-part:

```
agent: ring_bell()
agent: say("Take a moment to notice how you feel.")
agent: pause(10)
agent: say("So... what did you notice?")
agent: listen(15)
```

The pause before the question matters — it signals that the question
is not rhetorical. The user needs space to formulate.

## Closing the inquiry

When the user has had their say and there's a natural pause:

```
agent: say("Thank you for sharing that.")
agent: pause(3)
agent: say("Until next time. Be well.")
agent: ring_bell()
agent: goodnight()
```

## Don't

- Don't ask "did you enjoy it?" — that's a customer-satisfaction
  frame, not an inquiry frame.
- Don't say "good!" / "great!" in response to noticings. Reception is
  not judgement.
- Don't problem-solve. If the user reports difficulty, *reflect* it,
  don't *fix* it. The instruction was the meditation; the inquiry is
  the noticing of what arose during it.
- Don't run inquiry on short sessions (<5 min). The user needs time
  to develop something worth inquiring into.
