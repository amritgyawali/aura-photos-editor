# ADR-0022 - The emotion IPC surface

**Status:** accepted
**Date:** 2026-08-15
**Phase:** 10 - Expression, Emotion & Moment Ranking AI
**Supersedes:** nothing. **Amends:** nothing.

---

## 1. Context

Phase 10 produces a number per photograph that orders a whole wedding. Every previous
phase's surface was easy to keep on the right side of section 2.2's boundary, because
nothing they produced looked like a decision. This one does: **an ordering by
`emotion_score` looks exactly like a shortlist**, and the distance between "show me the
strongest moments" and "keep these" is one button nobody meant to add.

So this ADR is mostly about what the surface refuses to do, and about how that refusal is
structural rather than remembered.

Phase 09's surface was six commands, five reads and one dismissal. Phase 10's is seven,
five reads and two overrides.

---

## 2. The seven commands

| Command | Kind | What it is for |
|---|---|---|
| `emotion_status` | read | the panel header: coverage, face-awareness, peaks, links, versions |
| `image_emotion` | read | one photograph's reading, for the Emotion card |
| `moment_peak` | read | one moment's peak, for the browser's indicator |
| `reactions_of` | read | what reacted to this frame, for the pair viewer |
| `ranked_by_emotion` | read | a project's frames, strongest first |
| `prefer_frame` | write | "I would deliver this one" |
| `set_moment_peak` | write | "this frame is the one" |

Both writes are the photographer telling the product it is wrong. Neither decides a
photograph's fate.

---

## 3. Decision: `ranked_by_emotion` returns an ordering and nothing else

**The decision.** The command returns `[{photoId, emotionScore}]` and takes a limit. It
has no threshold parameter, no `keep` flag, no set semantics and no way to ask "how many
would you deliver".

**Why it exists at all.** Section 0's headline feature is "ranks every frame by emotional
value". Refusing to expose the ordering would be refusing to ship the phase.

**Why it stops there.** Section 2.2 puts final selection in phase 12 and album sequencing
in phase 29. An ordering is evidence; a selection is a decision, and the two live in
different phases for the same reason phase 05's distances and phase 08's groupings do.

**How that is kept.** Three things rather than one comment:

* the command's return type carries a score and an id, and there is no field a
  selection could be expressed in;
* `MomentBrowser` renders "An ordering, not a shortlist. AURA has not chosen anything
  here" in its own header, and has no checkbox, star or export;
* `EmotionCard.test.tsx` asserts that no score label contains `keep`, `reject`, `deliver`
  or `cull` - so a future edit that softened the language into a recommendation fails a
  test rather than a review.

---

## 4. Decision: a `null` reading is rendered as "nobody looked"

**The decision.** `image_emotion` returns `null` when a photograph has no row, and the
card says so in those words. `moment_peak` returns `null` for an unscored moment and a row
with `resolved: false` for a moment that was examined and had no apex.

**Why.** Migration 10's sixth stated property, carried out to the interface: "not scored"
is not "no feeling". Phase 12 reads a low score as evidence, and a card that drew a zero
would tell a photographer AURA looked and found nothing happening. Phase 09's card makes
the same distinction for the same reason, and this is the second time it has been worth
the extra state.

**The peak needs three states, not two.** A moment can have a clear best frame, can have
been examined and found flat, or can not have been scored. Only the first is a peak. A
browser that drew the second as one would point at a rounding error, and phase 29 builds
album spreads around what this panel points at.

---

## 5. Decision: the derived booleans are computed in Rust and sent

**The decision.** `EmotionReasonDto::caveat`, `FaceExpressionDto::readsAsCrying`,
`FaceExpressionDto::posedSmile`, `InteractionDto::milestone` and `MomentPeakDto::resolved`
are computed on the Rust side and put on the wire, rather than derived in TypeScript from
slugs and thresholds.

**Why.** The same argument `IntegrityReasonDto::exoneration` made one phase ago, plus one
that is specific to this phase. Three of the twenty reason codes are caveats about the
reading rather than statements about the photograph, and a UI that worked that out from a
list of slugs would work it out wrong exactly once.

The specific one is `readsAsCrying`. Its threshold is
`aura_core::contract::emotion::TEARS_CERTAIN`, which is 0.85, and **three unrelated places
read it**: this crate, the panel, and phase 09's third eye-intent rule in a crate that
cannot see either. Section 12's fourth failure mode is a false tear. A second copy of that
number in TypeScript is a second copy that can drift, and the drift would be silent.

---

## 6. Decision: the channel names travel with the reading

**The decision.** `EmotionDto::channelNames` is an array of eight strings sent on every
reading, and `FaceExpressionDto::channels` is a parallel array of eight numbers.

**Why not eight named fields.** The card draws them as eight bars in a fixed order, and
that order **is the model's output order** - renumbering it relabels a trained head. Eight
named fields would put that order in three places (the head, the store, the interface) and
make adding a ninth a wire change in all three. One array plus its names puts it in one.

The cost is eight strings per reading on the wire, which is about 90 bytes on a call that
already carries a face crop.

---

## 7. Decision: a preference is recorded and applied to nothing

**The decision.** `prefer_frame` writes a row to `emotion_preferences` and changes no
score in this build.

**Why.** A ranker that refitted itself while somebody was culling would reorder the grid
under their hands, and invariant 4 - same inputs, same versions, same output - would stop
being true the moment it did. Section 6.4 says the mechanism is "later reused for per-user
personalisation in Phase 30", and phase 30 is where those rows start moving the nine
coefficients, deliberately and with a `weights_ver` bump.

**Why it is collected now anyway.** Ten thousand comparisons is nine days of somebody's
work, and the rows a photographer generates while using the product are free. Starting the
collection two phases before the loop that consumes it is the difference between phase 30
having data and phase 30 starting a data collection.

**The refusal that matters.** A comparison across two weddings is refused with
`AURA-ML-5041`. It is not a comparison: the ranker is calibrated per scene and the two
frames were never candidates for the same delivery.

---

## 8. Decision: `set_moment_peak` is unbeatable, `prefer_frame` is not a decision

**The decision.** `moment_peak.user_chosen` is checked inside the upsert a re-analysis
performs. A recorded preference has no such protection because it changes nothing to
protect.

**Why.** Fifth phase, fifth unbeatable photographer decision -
`identities.user_locked`, `segments.user_locked`, `moments.user_locked`,
`image_integrity.dismissed`, and now this. The check is *inside* the statement rather than
read-then-branch, which is what closes the window a background pass could otherwise write
into.

The shape is phase 09's rather than phase 08's: a chosen peak *replaces* the machine's
choice for that field and leaves everything else - the margin, the confidence, the reasons -
re-measured, exactly as a dismissed flag does not stop a frame being re-measured.

---

## 9. What is on the wire and what is not

**On the wire:** ids, eight numbers per face, nine interaction strengths, a score, a
confidence, reason codes with their sentences, and normalised rectangles.

**Not on the wire, at all:** a template, a landmark array, an embedding, a name, a
tradition, or any free text a caller wrote. The `moment_type` field on the cloud task's
*output* is the one free-text field in the phase and it is shown in the Explain panel and
stored nowhere.

---

## 10. Consequences

**Good.** The panel cannot disagree with the harness about which reasons are caveats or
which faces read as crying. The tear threshold exists once. The ordering is available and
the selection is not. A photographer's peak choice survives every re-analysis.

**Bad.** Seven commands is one more than phase 09's surface, and `ranked_by_emotion` is a
command that will be asked to grow a `limit`-shaped selection every time somebody builds
something on top of it. The refusal is written down in three places and it will need
defending.

**Ugly.** `EmotionEvent` is typed on both sides and emitted by nothing, for the fifth
phase running: the Tauri shell has not been launched on the development machine, so an
emitter would be code nobody has run.

---

## 11. Related

* `docs/adr/ADR-0021-emotion-taxonomy-and-moment-ranking.md` - the taxonomy, the ranker
  and the cultural rules
* `docs/adr/ADR-0020-integrity-ipc-surface.md` - phase 09's surface, which this one is
  shaped after
* `crates/aura-app/src/emotion_commands.rs` - the seven commands
* `ui/src/components/explain/EmotionCard.tsx` and `MomentBrowser.tsx` - the two panels
