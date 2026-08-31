# ADR-0054 - The camera matching IPC surface: eleven commands, the honest report, and the three things a photographer can say

**Status:** accepted · **Date:** 2026-08-31 · **Phase:** 26 · **Supersedes:** nothing

[ADR-0053](ADR-0053-camera-matching-and-appearance-distance.md) records what the matching engine
decides. This records what the window can ask it, and what it deliberately cannot.

## 1. Context

Phase 26 section 4 asks for one panel: `CameraMatchPanel.tsx`, with "reference camera choice,
per-camera report, before/after pairs". Section 9 gives SFE "camera match panel, reference chooser,
per-camera report, matched-pair viewer".

This is the first surface in the product whose subject is a **device**. Every panel since phase 14
answers a question about a photograph, and phase 25's answers one about a wedding; this one answers
"what is AURA doing to everything my second shooter's body produced, and on what evidence".

That last clause is the whole design problem. A camera correction is invisible in any single frame -
the frame just looks right, or does not - and it is applied to hundreds of photographs at once from
evidence a photographer never sees. A surface that reported only *what* was corrected, without
saying *what it was inferred from*, would be asking for trust it had given no grounds for.

## 2. Decision: eleven commands

| Command | Reads or writes | What it is for |
|---|---|---|
| `camera_status` | read | The project header: bodies, pairs, reference, source mix, whether skin was measurable |
| `camera_transforms` | read | Every body's correction, with its bounds, its source and its reasons |
| `camera_fingerprints` | read | What each body's colour response was measured to be, and from how many samples |
| `camera_reports` | read | The per-camera sentence a photographer reads, assembled in Rust |
| `camera_pairs` | read | The matched pairs behind one body's correction, including the rejected ones |
| `camera_shooter_bias` | read | Each shooter's measured exposure habit and how much of it was corrected |
| `camera_pass` | write | Run the matching pass over a project |
| `set_camera_reference` | write | Choose which body everything else is matched toward |
| `disable_camera` | write | Leave one body out of matching entirely |
| `set_camera_override` | write | Record what the photographer set instead, on one body |
| `camera_reason_codes` | read | The panel's legend, from the frozen enum |

Six reads, one pass, three decisions and a legend. `camera_pairs` and `camera_fingerprints` are
separate from `camera_transforms` rather than nested in it, for phase 25's reason: a wedding can have
thousands of pairs, and a panel that listed the bodies would otherwise pull every one of them to draw
a header.

## 3. Decision: the report is assembled in Rust and sent as a sentence

`CameraReportDto.summary` arrives as finished prose:

> *2 cameras. 34 matched from photographs where two cameras overlapped at this wedding, 1 partly, 1
> from what AURA knows about the brand alone. Skin was not measured at this wedding, so no claim is
> made about how skin from the different cameras compares.*

That is `report::summarise`, not a template the panel fills in. Three reasons.

**The QC agent and the delivery report say the same thing.** Phase 27 reads these rows and phase 30
prints them; a sentence assembled in TypeScript would be a second version that drifts from the Rust
one within two releases. Phase 25 made the same call for `Outlier::describe`.

**The counts have to be described together or not at all.** "34 matched, 1 partly, 1 from a baseline"
is one fact about how much of this wedding's matching rests on its own evidence. Three numbers in
three boxes invite a reader to look at the largest and stop.

**The skin clause has to be able to disappear.** When phase 18's segmenter is untrained the honest
sentence is *"skin was not measured at this wedding, so no claim is made"* - not a zero, not a
dash, and not a dE00 figure computed over nothing. A panel choosing whether to render a field cannot
be relied on to make that judgement every release.

## 4. Decision: the fingerprints are on the wire, and so is the sample count beside them

`CameraFingerprintDto` carries `samples` and `confidence` next to every measured value.

A fingerprint from 40 frames and one from 4,000 are different kinds of claim, and the panel shows
which. This is phase 05's coverage rule at the grain of a device: report the number *and* what it
was measured from, because a body the second shooter used for eleven frames has a colour response
this product has essentially guessed at.

## 5. Decision: the rejected pairs are on the surface

`camera_pairs` returns pairs whose `verified` is false as well as true.

More than half the interesting cases in this phase are refusals - a body matched from a brand
baseline in a wedding both cameras shot all day is the support question this feature will generate -
and the answer is always in the rejected pairs: they were too far apart in time, or their backgrounds
disagreed, or they straddled the flash boundary. A surface that returned only the accepted ones could
not answer it.

Phase 24 made the same call about blocked cleanup candidates, and phase 17 about rejected style
pairs. Third time.

## 6. Decision: three things a photographer can say, and no fourth

Choose the reference. Disable a body. Set a body's correction by hand.

**There is no strength slider and no way to raise a bound.** `CameraOverrideInput` carries the same
movements the contract bounds, and a value outside its bound is refused rather than clamped - phase
21's rule, applied to the fourth surface in a row.

**There is no per-frame override here.** A camera transform is a statement about a *body*, and a
photographer who wants one frame different is looking for phase 15's tone override or phase 25's
gallery override. A per-frame control on this panel would be a fourth place to change the same
number, and the three that exist already have to be kept from disagreeing.

**Choosing a reference is not choosing a look.** The reference body is the one everything else is
matched *toward*; it receives the identity transform and is not itself corrected. That is why the
command is `set_camera_reference` rather than `set_camera_target`: a photographer picks which of
their own cameras is right, and the product does not offer an abstract ideal to match instead.

## 7. Decision: the pass runs to completion and returns what it did

Like phase 25's, and for the same reason: the transforms of a wedding are solved together - the
reference choice, the pair discovery and the held-out split are all project-scoped - so there is no
partial state a reader could make sense of. `CameraPassDto` returns bodies, pairs, solved, blended,
baseline-only and the elapsed time.

Section 11 budgets 25 s for the whole pass, which is a wall-clock figure rather than an interactive
one.

## 8. What is deliberately not on this surface

**No pixels.** No thumbnails, no before-and-after crops. The matched-pair viewer shows the two
photographs through the existing preview command, and everything this surface returns is a number or
a sentence. Phase 13's rule that evidence can never be a pixel.

**No apply.** Nothing here writes a recipe. `aura-brain-gallery::api::collect_frames` folds a camera
transform into the frames phase 25 solves over, and `aura_recipe::schema::merge` is what eventually
writes anything - the ordering ADR-0053 section 8 makes a data dependency.

**No baseline editing.** A studio may not add or change a brand baseline through the window.
`assets/camera_baselines/` is a bundled, attributed asset like phase 23's lens profiles, and a
photographer who could edit one could change what an already-delivered photograph looks like under an
identical hash - phase 23's argument for putting lens coefficients in the recipe, read from the other
side.

**No cloud anything.** Section 7: the gateway stays idle in this phase.
