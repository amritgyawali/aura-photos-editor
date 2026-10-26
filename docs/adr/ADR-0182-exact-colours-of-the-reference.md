# ADR-0182: The reference's exact colours

Date: 2026-10-26. Builds on ADR-0180 (scene-prior tone) and ADR-0181 (assessment fixes). No frozen
contract changes and no migration: the stored `reference_grade_v1` and its palette displacement
field are used as they are; `MatchIntent` gains a variant.

## Context

Natural copies the part of a reference that is an edit, estimated against ordinary photographs, and
on another scene it can only estimate that edit (ADR-0180, ADR-0181). A photographer asked for the
reference's colours themselves: "it must match color, color tone and look 100%". Asked to choose
between copying the reference's colours (repainting a different scene in them) and copying only
the edit, they chose the colours. That is a different goal, so it is a third matching goal beside
Natural and Palette rather than a change to either.

## Decision

**Exact colours of the reference** (`MatchIntent::Exact`, engine `reference-exact-colours-v1`)
gives the photograph the reference's colour distribution in `OKLab`:

* **Tone**: lightness goes to the reference's lightness of the same rank. The map depends on
  lightness alone, so tone never reverses (`tone_never_reverses`).
* **Colour**: iterative distribution transfer in the colour plane (Pitié, Kokaram and Dahyot,
  2007), twelve rounds of two turning directions, separately in four lightness bands blended
  smoothly, so the reference's shadows colour the photograph's shadows and its highlights its
  highlights (a split tone survives).
* **Skin**: with people in both photographs skin goes to the reference's skin and everything else
  to everything else; with people only in the photograph a face keeps its colour, because a
  reference of water or leaves has no skin to give (`a_face_keeps_its_colour_when_the_reference_has_no_people`);
  without people in the photograph everything goes to everything.
* Every one-dimensional map is learned from samples with its slope bounded (2.5 for colour, 4 for
  tone), so a narrow range is never stretched into a band, and the best-fitting shift puts the
  bounded map back over the reference's quantiles.

The reference's colours are kept when it is imported (8,192 samples of its full-resolution pixels,
`colour_reference_exact_v1:`); a reference imported earlier uses its stored palette samples.

The transfer is written into the stored reference grade the renderer already applies
(`transfer/exact_grade.rs`): its tone map as the 33-node tone curve (within 0.6 of the identity and
slope 4, wider than a look's 0.35 and 4 because a high-key reference can brighten a darker
photograph that far), the colour each output lightness takes on average as the neutral axis, and
the rest as the 33 x 33 palette displacement field, spread over the colour plane, smoothed and
filled toward its neighbours where the photograph has no colours. The stretch limit is 4.0 for this
goal (2.0 for a look). Because it is a stored grade, every strength slider applies, Undo restores
the photograph exactly, and the Lightroom export carries it as any other grade.

The native checks review it as they review every match, with a larger skin allowance (twice the
look limits, `EXACT_SKIN`), clipping allowed up to the reference's own share of clipped pixels plus
one per cent, and the distance to the reference required to fall. A session keeps its allowance so
Apply checks a strength by the limits analysis used.

**The band guard sees near-black steps** (`contours.rs`). Near black an 8-bit file's levels are a
unit or more apart in `OKLab`, so a dark gradient never counted as smooth and was never examined;
lifting a near-black JPEG backdrop into the midtones then showed its 8 x 8 blocks and passed. Adjacent
dark pairs within two levels on every channel are now examined by their own rule: they fail when
the step reaches 2.0, grew by at least 1.5, and sits between flat colour (both neighbouring pairs
below 1.5), so revealed texture - bark in a lifted shadow - is not a band, and a near-black
difference a match left unchanged is never counted. Light pairs keep their rule unchanged: with
flatness required everywhere the assessment's blotchy floor would have passed.

**Apply saves the strongest share that passes.** The native checks are not monotone in strength
on every photograph: a clipped share measured 1.91 % at 90 %, 1.92 % at 80 % and 1.56 % at 70 %
against a 1.91 % limit, so 80 % was refused while both neighbours passed. When the amount asked
for fails, Apply now tries 75, 50 and 25 % of it, saves the strongest that passes, and reports the
amount asked for (`reducedFrom`), which the panel states: "Saved at 60%: 80% failed this photo's
smoothness, highlight or skin-colour check". It refuses, as before, only when no share passes. The
strengths shown afterwards are the ones saved.

**The image-work gate is reentrant on one thread** (`aura-render::work_gate`). The colour-match
job holds a place for its whole run, and capturing an edited photo as a reference renders that
photo inside it; on a machine with one place - every two-core CI runner - the render waited for
its own caller forever. The test that does it hung all three CI test jobs until GitHub's six-hour
limit. A pass that already holds a place now takes it again without waiting; other threads still
wait, so the memory bound is unchanged. With `AURA_IMAGE_WORKERS=1` the whole workspace now
passes in 16 minutes of test time on the development machine.

The panel offers the goal as "Exact colours of the reference" with a sentence saying a different
scene is repainted in the reference's colours, and reports "Exact colours of <reference>: this
photo's colours and tones were moved onto the reference's own".

## Measured result

The 20-photo desktop test (`scripts/test-main-match-twenty.py --workflows exact ...`, run
`run-7352b4b3`, the same 20 photographs and six edited references as ADR-0181, 80 % requested,
through the real window). "Colour distance" is the harness's distance between the result's colour
distribution and the reference's; "steps" counts photographs with new steps between flat colour
above 0.1 % of their smooth pairs (the assessment's banded sky measured 0.77 %).

| goal | median colour distance to the reference | closer to the reference | photographs with steps | refused |
|---|---:|---:|---:|---:|
| Natural | 21.31 -> 18.40 | 19 | 0 | 0 |
| Palette | 21.31 -> 15.75 | 20 | 0 | 0 |
| **Exact** | 21.31 -> **14.45** | 19 | 2 (0.40 %, 0.15 %) | 1 |

Exact moves the most of the way, and on cool cyan, pastel and teal-orange references it lands
close (target 2: 12.9 -> 4.6; 10: 7.2 -> 2.2; 20: 10.3 -> 4.1; 9: 34.8 -> 16.0). It is not a 100 %
copy, and the reasons are in the photographs: where the reference's colours would clip a bright
backdrop, move skin past the allowance or lift a near-black JPEG backdrop into blocks, the native
checks stop it short - warm punch on a dark studio portrait keeps 6 % of the look (target 17), and
target 13 is saved at 20 % with the sentence that says so. Target 18 (a monochrome reference on a
low-contrast photograph) is refused: no share passes. Every integrity check of the harness held for
all five workflows, and Natural, Palette, the collection route and the learned route measured as in
ADR-0181.

On smooth synthetic scenes, where nothing limits it, the transfer closes 96 % of the distribution
gap (`the_exact_goal_gives_a_photograph_the_reference_colours`: 12.32 -> 0.50).

## Rejected after measuring

* **The transfer learned as a calibrated look** (the learned-original route's cubic plus 9 x 9 x 9
  residual lattice). The lattice fit is robust to outliers, which is right for a photographer's
  before/after pair and wrong for a transported copy: four of five photographs were refused as
  "cannot be explained by a global grade". Weighting every pixel let the cubic extrapolate into
  tone reversals. The stored reference grade - a monotone tone curve, an axis by lightness and a
  displacement field - represents the transfer without either failure.
* **Three-dimensional rotations in `OKLab`** (lightness mixed into the turning directions): tone
  can reverse along the grey axis, and the fitted looks were refused for it.
* **Tone slope 5 and stretch limit 5**: closer lightness distributions, more steps in smooth
  regions; **tone slope 2 to 3, more smoothing passes over the tone curve, or a slope cap of 1.0
  below lightness 0.3**: the steps in lifted near-black backdrops remained (they come from the
  file's own levels, which no curve shape avoids) while the match moved much less of the way. The
  band guard now refuses the strength at which those steps appear.
