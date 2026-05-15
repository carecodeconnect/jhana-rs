# Loving-Kindness (Metta) — Skill Reference

Read this with `read_skill("loving_kindness")` when the user asks for a
loving-kindness or metta meditation. Use as a structural / stylistic
exemplar — paraphrase, don't quote verbatim.

## Shape of a 5-10 minute session

The traditional metta progression is **four widening circles of
intention**: self → benefactor → neutral → all beings. Each circle
gets a short stretch of repeated phrases, with pauses for the user to
internalise.

### Phrases

The classical phrases are:

> May you be happy.
> May you be safe.
> May you be free from suffering.
> May you be at peace.

You can vary the wording — "may you be well," "may you find ease,"
"may your heart be light." Keep them simple and short.

### Sequence (tool-call shape)

```
ring_bell()
say("Find a comfortable seat. Let the body soften.")
pause(15)

say("Bring to mind yourself, just as you are right now.")
pause(5)
say("And quietly offer the wish — may I be happy.")
pause(8)
say("May I be safe.")
pause(8)
say("May I be free from suffering.")
pause(8)
say("How does it feel to offer this to yourself?")
listen(7)

say("Now bring to mind someone you love. Picture them in front of you.")
pause(5)
say("And wish them the same — may you be happy. May you be safe.")
pause(8)
say("May you be free from suffering.")
pause(8)

say("Now bring to mind someone you don't know well — a neighbour, a stranger you saw today.")
pause(5)
say("And offer them the same wish.")
pause(15)
say("How is this landing for you?")
listen(7)

say("Finally, let the wish widen — to everyone in this room, in this town, on this earth.")
pause(15)
say("May all beings everywhere be happy. May all beings be at peace.")
pause(10)
ring_bell()
say("Take a moment to notice how you feel.")
pause(8)
say("Whenever you're ready, open your eyes.")
listen(7)

say("Be well.")
goodnight()
```

## NCF-aware notes

- After offering the wish to **self** and **neutral**, check in
  (`listen()`). Both are the hardest steps for new meditators — self
  often brings up resistance, neutral feels arbitrary.
- If the user reports difficulty (resistance to "I am happy",
  blankness toward strangers), don't push. Acknowledge: *"That's
  natural. Let's stay with the body for a moment."* Then continue.
- Do NOT mechanically rotate through all four phrases on every
  circle — vary the pacing based on the user's responses.

## When not to use this

- If the user is grieving or recently bereaved, start with self-
  compassion phrases rather than imagining a loved one (that turn
  can re-open the wound).
- If the user is brand new to meditation, breath awareness is a
  gentler entry — read_skill("breath_awareness") instead.
