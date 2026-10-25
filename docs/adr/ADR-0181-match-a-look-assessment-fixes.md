# ADR-0181: Fixing the failures the deep Match a look assessment found

Date: 2026-10-25. Responds to
[the deep assessment](../reports/AURA-reference-look-colour-deep-assessment-2026-10-25.md) and
builds on ADR-0161 (a look learned from its original) and ADR-0180 (scene-prior tone). No frozen
contract changes; no migration; the `reference_grade_v1` and `calibrated_look_v1` shapes are
unchanged.

## Context

The assessment ran the real desktop app on 20 photographs with six declared looks and found:

1. a smooth desert sky turned into cyan-to-yellow bands (collection route, teal/orange reference);
2. a pale studio floor given green-cyan blotches (collection route, cool/cyan forest reference);
3. reference subject colours confused with its treatment (forest-to-fruit recolouring);
4. the native acceptance and independent colour metrics disagreeing;
5. a refused save leaving "Matching the selected reference to this photo..." on screen;

and that the two entry points used different fitting policies, that the smoothness guard only
looked at pixels darker than 96/255, and that some "accepted" results did not change the photo.

## Decision

**Unrelated scenes are matched as a look, not as a colour distribution** (failures 1-3). The
collection route (`gallery_fit/adaptive.rs`) compared Natural and Palette candidates by a distance
in which colour-distribution resemblance carries half the weight, so on an unrelated scene the
candidate that repainted the scene in the reference's colours won. For a scene that is neither
registered nor structurally corresponding it now fits Natural only (with ADR-0180's scene-prior
tone) and scores it without the palette term. Registered and corresponding scenes keep the full
search. On the known-look harness Palette-style transport reproduces a look far worse than Natural
(13.6 against 3.7), so this is not a trade of fidelity for safety.

**The smoothness guard checks every lightness, perceptually, at two scales** (failures 1-2, the
P0 "dark-only blind spot"). `contours.rs` samples 8,192 starting pixels and keeps pairs that were
smooth in the original (OKLab x100 below 1.0 adjacent, below 1.5 six pixels apart) at any
lightness. A pair fails when it becomes visible (3.0 adjacent, 6.0 apart) and grew at least
threefold; more than 0.5 % failing pairs at either scale refuses the candidate. Adjacent pairs
catch bands; pairs six pixels apart catch patches whose inside is smooth. Real edges and texture
were never smooth and are never examined. Every native review uses it: the panel's (`matcher`),
the collection route's adaptive review, and the calibrated-look application.

**A fitted transform may not stretch nearby colours more than twofold** (failure 1-2, the P0
"constrain the composed transform's local amplification"). `transfer::stretch` measures, on the
function rather than on noisy pixels, how far the fitted grade pushes apart two colours a hair
apart, at up to 4,096 of the photograph's own colours (99th percentile of the largest gain over six
`OKLab` directions). On the 20 assessment pairs, looks learned from each reference's own original
stretch by at most 3.2, Natural fits by about 1.1, and unconstrained Palette fits by 3.6 to 42 -
the tone curve to about 4, the chroma and per-hue gains to 9-36. `limit_stretch` brings a fit over
the limit back by shrinking, a quarter at a time, whichever component (tone, chroma, saturation,
hue, cast) lowers the stretch most, so the look is weakened only where it is steep. Limits of 4,
2.5 and 2 left new bands in 10, 3 and 2 of 20 palette results (none without the limit: 17), and
moved the median error against the true look from 24.2 to 13.0, 9.4 and 9.0; 2 ships. A same-scene
fit (structural similarity 0.8 or more) recovers a real edit and is exempt; learned looks are
calibrated rather than fitted and never pass through it. A pixel-only smoothness rule was tried
first and could not separate a band from a strong look amplifying a noisy shadow: it refused the
true warm-punch look on the dim studio portrait while passing every banded palette result.

**A refused save ends the panel's work as a refusal** (failure 5). PhotoStudio's `write` resolves
to the reason it did not save (a refusal, or another edit still saving) instead of swallowing it,
and the panel's `save` helper turns that into its own terminal error: progress cleared, the reason
shown in the panel, controls re-enabled. A save skipped because another was running was also a
silent success before; it is now a stated refusal.

**The panel learns the look from the reference's own original** (ADR-0161's discovery, extended to
the per-photo panel). An uploaded reference's path is remembered; on analysis the photo's
collection is searched once for its original (file name or same-second capture time, confirmed by
pixels), rendered as the panel renders targets. When found, the edit is learned pixel for pixel
and applied as a calibrated look; only Amount scales it, up to 100 %. Skin and clipping may move as
far as the photographer's edit moved them on its original, and the "closer to the reference's
histogram" requirement does not apply, because a correct edit of a different scene need not
satisfy it. The smoothness guard still applies. The search is cached per reference and collection
and repeated only when a photo is added or an original is edited. On the known-look harness this
left an error of 0.58 where the reference alone left 3.70. A cropped or retouched export is refused
with a sentence saying which photo looked like the original and why it could not be used.

**Both entry points now share the contract** for unrelated scenes (a natural look with
scene-prior tone), for originals in the collection (learned and pixel-exact), and for safety (one
smoothness guard).

**A refusal is shown in its own words.** The panel's and collection preparation's own sentences
("Another edit is running...", the editor's save refusal, "Stopped before preview verification...")
were thrown as plain exceptions and passed through `asIpcError`, which turns anything that is not a
backend error into "AURA hit an unexpected problem and stopped safely". The final desktop run showed
that sentence under a deliberate safety refusal. They are now `Refusal`s (`ui/src/components/look/
refusal.ts`), shown as written; any other exception still gets the generic sentence, so a bug never
shows a raw stack message. The tests had hidden this because their mock of `asIpcError` passed every
message through; the mocks now behave like the real function, and the desktop harness fails a
refusal that the panel does not explain.

**A collection match that changed nothing says so.** Collection preparation reported "Matched to
... at 80 % strength" for a photo whose reference stage changed no setting (the desktop run's
target 13). It now says the photo was not matched and why: no version of the look passed the
gradient, skin and highlight checks, or the photographer's own settings left nothing to change.

**Saved palette matches are refitted.** The Palette engine is now `reference-palette-transport-v8`,
so a palette match saved before the stretch limit is not reused as current; the collection route
and the panel both refit it.

**Three pieces of the parallel quality-fix work are merged in.** The collection route checks the
full-strength fit first and then the requested strength, so 50 % means half of a fit that was safe
at 100 % rather than a separately backtracked fit. An edited photo captured as a reference is
measured at its native size rather than as a 512-pixel thumbnail (the stored preview is still
bounded). And the desktop harness compares every saved result with the untouched original photo
and lists any case where the output is pixel-identical to it.

**A result nobody can see is reported as one.** Every native check also measures the mean `OKLab`
difference between the photograph as it was and the result, at 4,096 fixed pixels. Below 0.45 -
about one CIE76 unit, calibrated on the 80 desktop results, where `OKLab` x100 ran at a median 0.41
of CIE76 - the panel says the photo looks almost the same as before, why that happens, and what
gives a stronger match, instead of "Matched". The value is stored with the saved match and shown
again on reopening. A first threshold of 0.8 (assumed, not measured) told 16 of 20 Natural results
they were invisible when 13 of them were not; the calibration replaced it.

## Measured result

The assessment's own desktop harness (`scripts/test-main-match-twenty.py`), extended with
visible-change, new-band and learned-original measurements, on the same 20 photographs, the same
six references and the same 80 % strength, through the real window. "Before" is the assessment's
run (`run-988159e9`) re-measured with the same new metrics; "after" is this build (`run-f25aae97`). Error is mean
CIE76 to the declared look on that photograph.

| workflow | closer to the true look | median error | photos with new bands | refused |
|---|---|---|---|---|
| Collection | 12 → **18** | 8.10 → **6.10** | 3 → **0** | 0 → 0 |
| Natural | 14 → **17** | 6.09 → **5.91** | 0 → 0 | 3 → **1** |
| Palette | 10 → **16** | 9.84 → **5.99** | 6 → **0** | 0 → 0 |
| Learned (new: each reference made from another photo of the collection) | **19 of 20** | **0.71** (from 7.62) | **0** | 1 |

The learned workflow found the reference's original in 19 of 20 cases and came within the close
threshold (mean ≤ 3, p95 ≤ 8) in 14. Every integrity check held: native preview equals export, all
80 deliveries verified and equal to the saved render, originals and references untouched, undo and
redo exact, no page errors, and the visible change reported honestly in every panel case.

Remaining, and stated: Natural on an unrelated reference still moves a photo only a little when the
reference's look is mild or the guards limit it (now said in the panel); one learned look (target
17, a dim studio frame learned from a bright one) and one Natural match were refused rather than
saved with artefacts; and a learned look generalises less well from an original whose colours do
not cover the target's (targets 16 and 20, learned from dark frames, were only partial). The unit
test `a_batch_matched_to_one_reference_converges_on_one_look` and the accuracy gate at line 403 of
`tests/reference_grade_transfer.rs` failed before this work and still fail: both ask one unrelated
reference to remove more of a known look's error than one unrelated reference can identify.

## Not done, and why

* **Semantic region correspondence** (match sky to sky, skin to skin) and **a trained
  reference-conditioned predictor** (the report's P1 and P2). Without palette transport on
  unrelated scenes the region confusion that motivated them no longer shows in the 20-photo run,
  and the predictor needs training data this repository does not have.
* **Explicit Palette mode keeps distribution transport.** A photographer who chooses "Reference
  palette and tone" asks for it; the new smoothness guard now limits the bands and patches it can
  produce, and the panel says how far it had to be reduced.
* **Native distances and independent metrics can still disagree** (failure 4). They measure
  different things; the panel reports which was checked rather than presenting either as
  photographic correctness.
