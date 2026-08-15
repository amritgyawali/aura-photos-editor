# AURA-ML-5041 - A preference or a peak-frame choice was refused

**Severity / recovery:** see `crates/aura-core/errors.toml` for the registered values.

## What the photographer sees

The registered sentence and a product that has changed **nothing**. The comparison was not recorded; the peak frame is where it was.

`ask_user` rather than `retry`, for `AURA-ML-5029`'s reason: every refusal here is a statement about what was asked for - a photograph that is not in that moment, two frames from different weddings - and repeating it changes nothing.

## What actually happened

Two commands can raise it.

**`prefer`** - "which of these two would you deliver?" - refuses when:

* either photograph is not in this catalog;
* the two are the same photograph;
* the two belong to different projects. A pairwise comparison across two weddings is not a comparison: the ranker is fitted per scene and the two frames were never candidates for the same delivery.

**`set_peak`** - "this frame is the one" - refuses when:

* the moment is not in this catalog;
* the photograph is not one of that moment's frames. This is the common one, and it is almost always a stale panel: the moment was split or regrouped in the background and the browser is still holding the old frame list.

## What a recorded preference actually does, which is less than people expect

**It is collected, not applied.** This build ships the ranker coefficients that `ml/models/emotion/train_ranker.py` produced; a photographer's own comparisons are stored in `emotion_preferences` and phase 30's learning loop is where they start moving the numbers.

That is a deliberate decision rather than an unfinished one. A ranker that refitted itself while somebody was culling would reorder the grid under their hands, and invariant 4 - same inputs, same versions, same output - would stop being true the moment it did.

A recorded **peak** choice, by contrast, takes effect immediately and is unbeatable: `moment_peak.user_chosen` is checked inside the statement a re-analysis would overwrite it with, exactly as `moments.user_locked` is.

## Operator steps

1. Refresh the moment browser. A stale frame list is most of these.
2. If it persists, check whether the moment still exists: a regrouping that dissolved it took its peak row with it, by way of `ON DELETE CASCADE`.
3. Preferences and peak choices are both exportable before a schema rollback - see migration 10's rollback comment. They are the two things in this phase that cannot be recomputed.

## When this is not the problem

A photographer who disagrees with the ranking generally, rather than with one moment, is not hitting this. Nothing refused; the numbers are the argument.

## Related

* `AURA-ML-5029` - phase 08's grouping-edit refusal, the shape this one copies.
* `AURA-ML-5034` - phase 09's dismissal refusal, the other one.
