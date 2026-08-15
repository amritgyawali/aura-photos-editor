# AURA-ML-5039 - The emotion weight table was refused

**Severity / recovery:** see `crates/aura-core/errors.toml` for the registered values.

## What the photographer sees

The registered sentence, and a product that has **not** scored anything. This is the one phase 10 code that halts.

## Why it halts, when nothing else in this phase does

For `AURA-ML-5024`'s and `AURA-ML-5031`'s reason, two phases on, and here the argument is sharper than it was for either of them.

`emotion_weights.toml` is not only a threshold table. It is **the file that decides whether a composed Hindu ceremony scores like a smiling Christian one**. Section 12's first failure mode is "cultural bias toward Western expressiveness", and the mitigation is tradition-aware weights in this file. A *half-loaded* weight table applies the tuned weights to some scenes and the defaults to others - silently, and in precisely the direction the phase exists to prevent.

Every other failure in this phase degrades into a wedding that is still usable: a frame with no score, a moment with no peak, a refused choice. A silently altered weight table is different in kind, so the loader refuses and leaves the previous table in place.

## What actually happened

One of eight rules refused, and the detail names the file, the key and the rule in that order - which is the order somebody fixes them in.

1. **"has no rationale"** - the rule with the most friction and the most value, inherited from `scene_profiles.toml` and `moment_profiles.toml`. A weight nobody can explain is a magic number, and somebody who cannot write a sentence saying why a photographer would agree with it has not finished deciding it. Nine characters minimum.
2. **"is not valid TOML"**.
3. **"has a channel weight outside 0..2"**.
4. **"has every channel weight at zero; nothing in this scene could ever score"** - a whole scene silently ranked at the floor, which reads to a photographer as "the product dislikes this kind of photograph".
5. **"names a channel this build does not know"** - the eight are fixed by `FaceExpression::CHANNEL_NAMES` and a typo would apply a weight to nothing.
6. **"names a tradition with no ritual taxonomy in config/rituals/"** - the list the loader checks against is the eight the ritual head can emit, which is the list of things that can actually be selected at run time. A tradition weighted here that the head cannot emit is a weight nobody will ever notice is wrong. Five of the eight carry a row in the shipped file and three do not: `sikh` has no taxonomy, so nobody here has established what its rites look like or how composed a couple are during them; `mixed` and `unclear` are abstentions. All three fall back to `[default]`, which is set from the balanced fixture set rather than from the Christian one.
7. **"has a ranker coefficient outside -8..8"** - the Bradley-Terry utility is a linear form over nine features in `0..1`; a coefficient past this makes the logistic saturate and every frame in the wedding scores 0.00 or 1.00.
8. **"has a calibration knot sequence that is not monotone"** - the per-scene isotonic map. A non-monotone map reorders frames, which is the one thing a calibration must never do.

## What AURA does automatically

**For an installation override** at `<catalog>/config/emotion_weights.toml`: falls back to the shipped baseline, keeps the refusal for the Problems list, and carries on. The baseline is known good and falling back to it keeps the product open.

**For the embedded baseline**: halts. A build whose own weight table will not parse is a broken build and it must not open a project - which is why `crates/aura-brain-wedding/tests/config.rs` loads it in CI rather than only at run time.

## Operator steps

1. Read the file, the key and the rule from the message.
2. For an installation override, the fastest fix is to delete it: `<catalog>/config/emotion_weights.toml`. The shipped baseline takes over and the product is immediately correct, if untuned.
3. For rule 1, write the sentence. It is expected to say *why a photographer would agree*, not to restate the number. "In a Hindu ceremony the couple are composed by convention and a smile is not what the moment asks for" is a rationale; "composure weighted higher in ritual" is not.
4. Bump `version` after any change. It is written into `image_interaction.weights_ver` and `AURA-ML-5038` depends on it moving.
5. `just phase-10-verify` loads the table as its second check, before anything is built on it.

## When this is not the problem

A photographer whose ranking is merely *wrong* is not hitting this: the table loaded, and the numbers in it are the argument. `docs/emotion-and-moments.md` is where that conversation starts.

## Related

* `AURA-ML-5024` and `AURA-ML-5031` - the same rule for scene profiles and grouping thresholds.
* `docs/adr/ADR-0021-emotion-taxonomy-and-moment-ranking.md` section 4 - why the weights, the ranker and the calibration are one file rather than three.
