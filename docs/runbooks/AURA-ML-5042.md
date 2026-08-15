# AURA-ML-5042 - A moment's strongest frame could not be separated from its neighbours

**Severity / recovery:** see `crates/aura-core/errors.toml` for the registered values.

## What the photographer sees

The registered sentence, and a moment in the browser with no peak marker on it. Every frame is still there, still scored, and still selectable.

## This is usually the correct answer, not a failure

A moment is what the photographer shot once. Fourteen frames of a bouquet toss at 10 fps genuinely have an apex; fourteen frames of a bracketed detail shot of the rings genuinely do not, and neither do six frames of a family lined up smiling for the same three seconds.

`MomentPeak::MIN_MARGIN` is 0.04. Below it, `(top - runner_up) / top` says the two strongest frames are within four percent of each other on the action curve, and the product stops pretending it can tell them apart. The kind becomes `flat`, `EmotionOutline::peaked` does not count it, and `peak_rate` reports the truth about the wedding rather than a number that always looks good.

The alternative - always naming a peak - is worse in a specific way: phase 29 builds album spreads around peak frames, and a peak chosen by a rounding error is a spread built around a rounding error.

## When it *is* worth looking at

`EmotionOutline::peak_rate` is the number to read, not the individual warnings.

* **Below about 0.2 across a whole wedding.** Either the expression head is returning near-constant values - which is what an untrained head does, and phase 10's condition C1 says so - or the wedding really was shot in brackets. Check the Emotion card on a moment you remember: if every face reads 0.5 on every channel, it is the head.
* **High on the ceremony and zero on the reception.** Usually a face pass that covered one and not the other. `EmotionOutline::face_aware` is the number that says so, because seven of the nine ranker terms come from faces.
* **Every moment flat, on one camera only.** A second body whose clock is out by more than the moment window: the frames grouped, but the action curve is being built over frames that are not in the order they were shot. Phase 01's timeline alignment is the conversation.

## What AURA does automatically

Stores the peak row with `kind = 'flat'` and the measured margin, rather than storing nothing. A moment that was examined and had no apex is different from a moment nobody examined, and migration 10's sixth property applies here as much as it does to a missing score.

`peak_proximity` on every frame of a flat moment lands close to 1.0, which is the honest reading: if nothing separated itself, every frame is as near the peak as every other.

## Operator steps

1. Read `peak_rate` in the Emotion panel header rather than chasing individual moments.
2. If it is implausibly low, check `face_aware` in the same header first.
3. A photographer who disagrees with a peak that *was* chosen picks their own; `set_peak` is unbeatable and survives every re-analysis.

## When this is not the problem

A moment with no peak row **at all** is a moment that was never scored - `AURA-ML-5040`, or a pass that has not reached it.

## Related

* `AURA-ML-5030` - phase 08's implausible-grouping warning, the other "this number is worth a second look" code.
* `docs/emotion-and-moments.md` - what a peak means, in the photographer's language.
