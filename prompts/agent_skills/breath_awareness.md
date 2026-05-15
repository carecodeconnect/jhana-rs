# Breath Awareness (Anapana) — Skill Reference

Read this with `read_skill("breath_awareness")` when the user asks for
a breath meditation, mindfulness of breathing, or anapana. Use as
structural / stylistic guidance — paraphrase, don't quote verbatim.

## Shape of a 5-10 minute session

Anapana is the foundation. The goal is *access concentration* —
sustained attention on a single physical sensation (the breath at the
nostrils, the rise and fall of the abdomen) without being pulled
into thought. The instruction is short; the **practice is repetition
and gentle return**.

### Sequence (tool-call shape)

```
ring_bell()
say("Find a comfortable seat. Let the body settle.")
pause(15)

say("Notice the breath, just as it is. Don't try to change it.")
pause(10)

say("Feel the air moving at the nostrils — cool on the way in, warm on the way out.")
pause(15)

say("If a thought pulls you away, that's natural. Just notice, and come back to the breath.")
pause(20)

say("How is it landing? Are you settled?")
listen(7)

# Depending on the reply: continue, slow down, or shift focus.
# If user reports a busy mind: shorten the pauses, add more guidance.
# If user reports calm: lengthen the pauses, say less.

say("Stay with one full breath in.")
pause(6)
say("And one full breath out.")
pause(6)
say("Just the next breath. That's all.")
pause(30)

say("If you notice the mind has wandered, gently return.")
pause(15)
say("This returning is the practice. There's nothing to do but come back.")
pause(30)

say("How is it now?")
listen(7)

# (Continue for as long as feels appropriate; close when natural.)

say("Stay with the breath for a few more moments on your own.")
pause(45)

ring_bell()
say("Take a moment to notice how you feel.")
pause(8)
say("Whenever you're ready, you can open your eyes.")
listen(7)

say("Be well.")
goodnight()
```

## Key teaching points (use sparingly)

- The breath is **not changed**, only **noticed**. Don't tell the
  user to breathe deeply, slowly, or in a special way. Their breath
  is fine as it is.
- Thoughts are **not the enemy** — they're the prompt to return. The
  pattern *thought → notice → return* IS the practice, not a failure
  of it.
- Anchor sensations: at the nostrils (subtler, more concentrative)
  or the rise/fall of the abdomen (broader, more grounding). Let the
  user pick — or if asked, suggest the abdomen for beginners.

## NCF-aware notes

- Frequent short check-ins (`listen()`), longer pauses between them.
  Anapana is mostly silence — the model shouldn't fill it.
- If the user says they're "lost" or "can't focus", **acknowledge
  and continue** — don't problem-solve. The instruction is always
  the same: notice, return.
- If the user reports physical discomfort (back pain, restlessness),
  briefly invite them to adjust posture without making a fuss of it.
