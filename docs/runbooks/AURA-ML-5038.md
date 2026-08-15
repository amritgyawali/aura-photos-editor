# AURA-ML-5038 - Stored emotion scores are a version behind

**Severity / recovery:** see `crates/aura-core/errors.toml` for the registered values.

## What the photographer sees

The registered sentence, and a wedding that carries on working. Every stored score is still readable, the moment browser still sorts, and the Emotion card still explains itself. What has changed is that this build would produce different numbers, and the background pass is replacing them.

## The sixth version-drift code, and why there is one per phase

`AURA-ML-5015` for embeddings, `AURA-ML-5018` for faces, `AURA-ML-5022` for scenes, `AURA-ML-5028` for moments, `AURA-ML-5033` for technical verdicts, and this one for emotion. They all exist for the same reason: comparing a number produced under one version with a number produced under another returns a **plausible** answer that means nothing, and the only defence is to make the comparison impossible rather than discouraged.

The failure this prevents is specific and quiet. A wedding half-scored under weight table 1 and half under weight table 2 sorts into an order that is neither. Nothing looks wrong. The photographer culls from the top of it.

## Which of the three moved, and what it costs

The message names all three because the first question is *which*, and the answer changes what has to be redone.

| Column | What it invalidates | Cost of a bump |
|---|---|---|
| `model_ver` | every expression value and every interaction strength | the whole pass: two model calls per frame |
| `analysis_ver` | the gaze, the peak curve, the reaction links, the feature vector | the whole pass, but no model call is cheaper than the stored one |
| `weights_ver` | `emotion_score` alone | arithmetic over stored readings: the cheapest of the three |

There is deliberately **no fourth column for the ranker**. Its coefficients ship inside `emotion_weights.toml` beside the scene and tradition weights, so `weights_ver` covers both, and two columns that invalidate exactly the same thing never exist. That is phase 09's third inherited rule applied in the direction that removes a column rather than adding one.

## What AURA does automatically

Nothing is deleted and nothing is recomputed in the foreground. `EmotionStore::pending` is a query rather than a journal - "which photographs have no score at *these* versions" - so the background pass picks the stale rows up without anybody triggering anything, and a killed run continues where it stopped. Invariant 5.

`EmotionOutline` reports the **lowest** version present rather than the newest, which is the honest direction: a mixed project should describe itself by its oldest row.

Two things survive a re-score, because they are the photographer's rather than the machine's: `moment_peak.user_chosen` and every row of `emotion_preferences`.

## Operator steps

1. Read the three stored numbers and the three current ones out of the message.
2. Leave it alone. The pass runs on its own and the wedding is usable while it does.
3. If it has not cleared, check the Problems list for `AURA-ML-5040` - a photograph that fails to score every time is a stuck row, not a stale one.
4. `just phase-10-verify` includes a stale-version check that plants an old row and asserts the pass heals it.

## When this is not the problem

A photographer who thinks the *ordering* is wrong is not hitting this. That is a conversation about `emotion_weights.toml`, and `docs/emotion-and-moments.md` is where it starts.

## Related

* `AURA-ML-5033` - the same shape for technical verdicts, and the code this one is modelled on.
* `docs/adr/ADR-0021-emotion-taxonomy-and-moment-ranking.md` section 5 - why three columns and not four.
