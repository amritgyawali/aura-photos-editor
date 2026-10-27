# ADR-0180: The tone of a look, estimated against ordinary photographs

Date: 2026-10-25. Extends ADR-0160's distribution-transfer engine (`color_match::transfer`)
without changing the stored `reference_grade_v1` shape, the renderer or any frozen contract.
The number is chosen clear of ADR-0160, -0161 and -0170, which other branches use.

## Context

Match a look fits the reference's colour and tone onto a photograph. When the reference is a
different scene - the common case, an edited photograph of another couple - Natural mode
copied the neutral axis and shared colour evidence and left tone essentially alone, because
the reference's lightness histogram is mostly its scene. Palette mode copied that histogram
and the native checks then cut it back to nearly nothing. Neither carried a matte black lift,
an airy brightening, a moody darkening or a contrast curve.

A known-look harness measured where the error is
(`crates/aura-app/src/color_match/transfer/tests/looks.rs`, 18 known looks, every target scored
against the same look applied to it, which no method sees). On 57 photographs (720 pairs):

| method | total | tone | colour |
|---|---:|---:|---:|
| leave the photograph alone | 4.23 | 2.92 | 2.22 |
| Natural as shipped | 3.72 | 3.06 | 1.41 |
| Palette as shipped (before native backtracking) | 13.55 | 12.60 | 3.17 |
| learned from the reference's own original (ceiling) | 1.80 | 1.21 | 1.02 |
| Natural colour + the ceiling's tone | 2.07 | 1.16 | 1.40 |
| the ceiling's colour + Natural tone | 3.55 | 3.11 | 1.04 |

(Mean `OKLab` distance x100.) Tone is the bottleneck: perfect colour would gain 0.17, perfect
tone 1.65.

## Decision

Estimate the tone of a look by Bayes' rule over tone curves (`transfer/tone_prior.rs`):

* a **scene prior**: a Gaussian over nine lightness landmarks (0.2 to 99.8 per cent) of
  ordinary finished photographs, shipped as the measured landmarks of 37 Pexels photographs
  (numbers only; no photograph ships);
* an **edit prior**: a five-node monotone curve, with probability 0.8 of no edit and node
  spread 0.1 otherwise;
* evidence tempered to half weight, because 37 photographs make a Gaussian more certain than
  photographs are; no change-of-variables term, which favoured flat curves and measured worse;
* the posterior mean, over a fixed low-discrepancy set of 6,144 candidate curves.

The same estimate is made for the target photograph, and the tone applied is
`look ∘ own⁻¹`: an ordinary photograph receives the look as estimated; one flatter than a
finished photograph is first given a finished range. It applies to Natural fits of a reference
whose scene does not correspond; corresponding scenes keep the direct quantile fit and Palette
keeps its distribution transport. The Tone match slider scales it like any other tone.

Natural mode now runs the same native backtracking Palette did, so the strength offered at
100 % is one Apply accepts (clipping, smooth-gradient, per-person skin and distance checks).

The report carries `toneEstimate` and the panel says, in one sentence, what tone the look
implies and whether the photograph was finished first.

## Measured result

Mean `OKLab` distance x100 to the true look on the target, all 18 looks; the last column counts
reference-target pairs the method left farther from the true look than doing nothing. "Before" is
the Natural fit as it was (the harness's `shipped` method), "after" this decision, "ceiling" the
look learned from the reference's own original. Flat sets hand every method a flat render of the
target (blacks lifted to about 0.16, whites lowered to about 0.84 in encoded sRGB) and score
against the look applied to the finished photograph.

| set | pairs | leave alone | before | after | ceiling | made worse, before → after |
|---|---:|---:|---:|---:|---:|---|
| finished, 26 Pexels (tuning) | 1,872 | 4.34 | 3.89 | **3.70** | 1.89 | 652 → 575 |
| the same, flat | 1,872 | 9.23 | 9.03 | **8.47** | 8.38 | 745 → 242 |
| held out: 11 Pexels + 27 wedding frames after Auto edit | 2,736 | 4.77 | 3.87 | **3.75** | 2.26 | 717 → 691 |
| the same, flat | 2,736 | 8.29 | 7.78 | **7.22** | 7.11 | 750 → 141 |
| mixed 57 (FiveK, wedding, Pexels), every sixth reference | 720 | 4.23 | 3.72 | **3.52** | 1.92 | 256 → 248 |

The built-in prior's 37 photographs include the 26 tuning photographs and the 11 held-out Pexels
ones, so only the 27 wedding frames are fully unseen by it; in the harness each reference's prior
is rebuilt without that reference's own photograph. The gain is real and modest on finished
photographs (5 %), larger on flat ones (6-7 %, and four to five times fewer photographs made
worse), and nowhere near the ceiling: most of what is left needs the reference's original.

Two refinements were measured on the way and are part of the decision: an estimate that moves
tone by less than 0.01 anywhere is the identity, and it *replaces* the small histogram-based tone
the earlier fit gave monochrome and toned references (3.83 without the replacement, 3.70 with it).

## Rejected after measuring

* **Explaining the reference against a bank of photographs** (each bank photograph fitted to the
  reference by full distribution matching, explanations weighted by effort and averaged into one
  look applied to every target): 6.03 against Natural's 3.21 on four looks - worse than leaving
  the photograph alone. A full distribution fit onto a different scene learns the scene
  difference, and averaging does not cancel it.
* **A Gaussian edit prior without a no-edit mass**: 4.78 against 3.89 on finished photographs. Any
  reference that is not the average photograph is read as edited.
* **The change-of-variables term**: 4.63 against 3.81 without it.
* **Believing the target's own departure more readily** (no-edit 0.3, or black and white points
  only): flat targets improve further (to 6.1-6.9 from 9.03) but finished targets get worse
  (3.87-4.46 against 3.89), and painted test scenes with soft ends were "finished" wrongly.
* **A two-round adaptive importance sampler**, which estimates the stated posterior more precisely
  when the evidence is strong: 3.81 against 3.68 for the single round. The priors are
  approximations, and the coarser estimate stays closer to no change; the settings were chosen
  with the single round, which is what ships.

## Consequences

The change is visible but subtle. On three wedding frames matched to one Pexels portrait under
airy, moody, pastel and warm-matte looks (`write_look_examples`), the result moves in the right
direction and stays far from the true look; a bright portrait on a white backdrop is an ordinary
photograph to the prior, so most of an airy or pastel lift is read as the scene. The paired
workflow (the reference's original beside it) remains the route to a close match.

One reference of another scene still does not identify its edit; this is the estimate with the
least expected error under two stated priors, and it moves tone only where the reference is
clearly unlike an ordinary photograph. Tests that asserted tone must never move across
unrelated scenes now assert that an ordinary reference leaves tone close to unchanged and a
clearly edited one moves it toward the look. Analysis renders up to six more native frames in
Natural mode when the fitted endpoint fails a guard (one more when it passes).
