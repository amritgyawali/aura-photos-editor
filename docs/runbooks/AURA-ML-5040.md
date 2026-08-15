# AURA-ML-5040 - One photograph could not be scored for expression or interaction

**Severity / recovery:** see `crates/aura-core/errors.toml` for the registered values.

## What the photographer sees

The registered sentence, one entry in the Problems list, and a wedding that is otherwise unaffected. One frame in three thousand has no emotion score.

## What "no score" means, and what it must never be read as

**Nothing is written for that frame.** No row in `image_interaction`, no rows in `face_expression`. That is deliberate and it is migration 10's sixth stated property: a score of 0.0 would look like a completed reading of a flat photograph, and phase 12 reads a low score as evidence. A frame with no row is a frame nobody looked at, the coverage view counts it against `v_emotion_coverage.scored`, and the next pass retries it.

The alternative - writing a zero and moving on - is the silent failure invariant 9 exists to forbid, and here it would quietly demote a frame out of a gallery.

## What actually happened

Almost always one of four things, in descending order of likelihood.

1. **The 2048 px proxy would not decode.** The overwhelming majority. `docs/runbooks/previews.md` is the conversation, and the cause is usually a truncated file on a card that was pulled mid-write. Transient, and retrying is right.
2. **The expression head or the interaction head refused the tensor.** A manifest disagreement, which is `AURA-ML-5007` underneath. Permanent until the model pack is fixed; `just models-check` says so in one command.
3. **The frame has no faces and the interaction head found nothing.** This is *not* an error - it produces a real score with `EmotionCode::NoFaces` in the reasons - and it is listed here only because it is what people expect this code to mean and it is not.
4. **The run was cancelled between the read and the write.** `AURA-ML-5011` underneath, and the next pass picks the frame up.

## What AURA does automatically

Counts it, logs it with the photograph's id, and carries on. One unreadable frame must not end a four-thousand-image pass. The work remaining is a query, so nothing has to be remembered between runs.

## Operator steps

1. Look at how many. One is a bad file; four hundred is a broken model pack or a preview cache that cannot be written.
2. For a handful, open one of the named photographs in the grid. If the thumbnail is also broken, this is a phase 02 problem and not a phase 10 one.
3. For a lot, run `just models-check`. An unsigned or digest-moved model pack fails there in one line.
4. Re-run the pass. The frames with no row are exactly the frames it picks up.

## When this is not the problem

A photograph that scores *low* is not this. A low score is a reading, it has reasons attached, and `docs/emotion-and-moments.md` explains what they mean. This code is only ever about a frame with no reading at all.

## Related

* `AURA-ML-5035` - phase 09's identical shape, for a frame that could not be checked technically. The two usually fire together, because they fail on the same unreadable proxy.
* `AURA-ML-5027` and `AURA-ML-5032` - the same shape for scenes and moments.
