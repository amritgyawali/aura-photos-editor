# PHASE-15 exit report - Exposure AI & White Balance AI (mixed lighting mastery)

**Branch:** `feat/phase-15-exposure-white-balance-ai` · **Gate:** `aura-cli verify --phase 15`
exits 0 · **Status:** implemented **conditionally**, on the seven conditions in section 8.

## 1. What shipped

One frozen contract, one module of twelve files, one migration, one IPC surface, two panels,
two ADRs, two signed models, two product documents and a gate.

`aura-core::contract::tone` freezes the shape. `ToneEstimate` is section 5's struct with eight
recorded spellings (ADR-0031 section 2), plus `SkinLocus`, `ReferenceFrame`, `ToneOutline`,
`ToneOverride` and `ToneService` - none of which section 5 names and all of which sections 6.2
to 6.4 need. **There is no field anywhere in it for an ideal skin value**, and that is the
phase's central design decision rather than a courtesy.

`aura-brain-photo::tone` decides. `targets.rs` loads 22 argued-over scene rows and refuses a
broken file. `stats.rs` reads the pixels once. `neutrals.rs` finds what is supposed to be
white. `skin_locus.rs` accumulates what each person's skin actually looks like across this
wedding. `illuminant.rs` generates the hypotheses, classifies the light and splits a
mixed-light frame in two. `wb.rs` scores each hypothesis against skin and neutrals. `solve.rs`
picks the light and decides how much of it to remove - a twenty-step linear scan, because with
two people in frame the satisfying set is not an interval. `exposure.rs` moves the exposure and
then clamps it against clipping and shadow noise. `reference.rs` picks each chapter's anchors.
`analyse.rs` composes them. `store.rs` and `store/codec.rs` own migration 15. `api.rs` is the
frozen service and the resumable walk. `fixtures.rs` is the synthetic ground truth.

Migration 15 adds `image_tone_estimate`, `identity_skin_locus`, `segment_reference_frames` and
`v_tone_coverage`. **There is no skin-target column, no tone-category column and nowhere to put
one**, and the gate scans for one on every run.

The IPC surface is seven commands (ADR-0032); the Basic panel shows two confidences, the
protected dot, the mixed-light note and the coverage caveat; the review queue groups
low-confidence frames by scene and accepts a whole scene in one action.

## 2. Acceptance criteria (section 13)

| Criterion | Status |
|---|---|
| Exposure and WB set automatically with reasons and confidence | **met** - every frame, two confidences, up to six typed reasons |
| Faces correctly exposed in dark receptions without flattening the mood | **met on fixtures** - `mood_preserved` aims at the bottom of the band rather than its middle |
| Skin colour believable across skin tones, measured and published | **measured on synthetic reflectances** (mean 0.110 dE00, spread 0.159); published in `docs/skin-fairness.md`. **Not measured on photographs** - C1 |
| Coloured stage lighting survives editing | **met** - 106 % of a coloured light's cast survives across the fixture set, and 3 of 5 frames now say so. The flag's remaining gap is a threshold rather than a mechanism - C5 |
| Mixed-light frames flagged rather than badly corrected | **met** - 8 frames marked, both regions stored, `idx_tone_mixed` is phase 18's query |
| Every segment has reference frames ready | **met** - 5 anchors from 43 candidates; a chapter with too few gets none rather than bad ones |

## 3. What the section 10.1 gates measured

`cargo test -p aura-brain-photo --test tone_eval` - 24 gates, all green. (22 at the phase
gate; C5's two regression tests are the 23rd and 24th.)

| Gate | Threshold | Measured |
|---|---|---|
| White balance within 200 K and the tint tolerance | >= 85 % | **91.3 %** (42/46 correctable frames) |
| Exposure lands in band or names the constraint that stopped it | >= 85 % | **met** |
| Clipping never exceeds the scene tolerance | exact, every frame | **met** |
| Skin dE00 mean | <= 3.0 | **0.110** |
| Skin dE00 spread across five tone buckets | <= 1.0 | **0.159** |
| Coloured light preserved | >= 50 % of cast | **106 %** |
| Reference frames per segment | >= 3 | **5** |
| Determinism | byte-identical | **met** |

Four of the twenty-two gates exist to prove the harness can fail: a do-nothing exposure solver,
a constant 5,500 K white balance, a neutralised dance floor and a one-bucket fairness report are
each asserted to be *rejected*.

**Every one of these numbers is about synthetic frames.** The illuminant, the subject luminance
and the skin reflectance were chosen, painted into the pixels and read back through the real
pipeline. That proves the arithmetic. It is not evidence about a photograph.

## 4. Benchmarks

| Row | Section 11 | This build |
|---|---|---|
| Estimation per image | <= 25 ms (GPU) | **waived** - no GPU backend (ADR-0007). Processor path guarded at 500 ms/unit in release |
| 4,000 images | <= 100 s | **waived** - extrapolated at ~4,590 s from a debug build |
| Extra storage per image | <= 600 B | **806.9 B - not met.** See section 8, C4 |

## 5. Telemetry (section 11)

`tone.estimated` (images, ms, mean_ev, mean_cct, mixed_light_ratio), `tone.low_confidence`
(count, scene_histogram) and `tone.user_override` (param, delta) are emitted by
`TonePass::emit`. `tone.untargeted` is a fourth, for a scene with no target row.

## 6. Invariants

1. **Never mutate a RAW.** No path column, no file operation anywhere in this phase.
2. **Confidence and reasons.** Two confidences, and migration 15's `reason_count` CHECK refuses
   a row with none.
3. **Three-tier compute.** Tier 2, the 2048 px proxy - a cache hit, because phases 06, 09 and 11
   already read it.
4. **Determinism.** Asserted by the harness and by the gate on the assembled path.
5. **Resumability.** `ToneStore::pending` is keyed on the three version columns.
6. **Local-first.** No cloud call in this phase, as section 7 requires.
7. **Scene-conditioned everything.** 22 scene rows; a scene with no row is recorded and reported.
8. **Colour discipline.** The solve is in CIE 1976 `u'v'`, never in kelvin - a distance in kelvin
   is not a distance in colour. This was asserted before it was true: the preserve-mood
   correction interpolated a temperature until C5 was worked, and
   `the_correction_between_two_lights_is_walked_in_chromaticity` is what now holds it.
9. **No silent failure.** Six codes, `AURA-ML-5060` to `5065`, each with a runbook.

## 7. Rollback

Migration 15 is reversible and recomputable: four `DROP`s and one `DELETE` return the catalog to
schema 14, and every row is derived from pixels, phase 06's faces and phase 07's scenes. The one
exception is the usual one - `user_exposure_ev`, `user_temperature_k` and `user_tint` are not
derivable from anything, and the runbook says to export them first. They also live in
`edit_recipes` and in the sidecars, which is the second copy that makes the loss survivable.

Feature flag: the pass is only reached through `estimate_tone`. Models are pinned by digest and
roll back on a failed first use.

## 8. Conditions carried out of this phase

**C1 - Both heads are untrained, and no number here is a claim about a photograph.** `Sev 2.`
Section 8 step 1 asks for RAW plus expert edits across traditions and lighting types; there is
no such dataset here, there are no camera files and there is no photographed ColorChecker.
`WB_HEAD_TRAINED` and `EXPOSURE_HEAD_TRAINED` are both false, so **neither head is ever
consulted** and no frame in this build is white-balanced by a random projection. Everything
measured above is the *solver*. This closes with phase 05's C10 and phase 02's camera files
rather than separately. **No later phase may claim an exposure, colour or fairness result that
depends on these weights until it closes.**

**C2 - The fairness gate is measured on five reflectances, not on five people.** `Sev 2.`
`SKIN_TONES` is five points on a line through the region human skin occupies. Passing a spread
test on them proves the arithmetic has no lightness-dependent term. It says nothing about a real
person in a real reception. Section 9's QAIQ deliverable - 600 frames reviewed blind across
lighting types with systematic bias catalogued - has not been done. `docs/skin-fairness.md`
states this in the product's own words rather than only here.

**C3 - The 600-frame expert audit does not exist.** Section 9 budgets QAIQ four days for it. No
perceptual A/B against Imagen, Aftershoot or Lightroom Auto has been run, so section 10.2's
60 % preference bar is unmeasured.

**C4 - Section 11's 600 B storage row is not met; the measurement is 806.9 B.** The figure is
recorded in `perf/budgets.toml` with the decomposition and with the four reductions that were
considered and rejected - dropping the alternatives, numbering the reason codes, dropping the
evidence rectangles, and sharing one illuminant per moment. Each of those saves 90 to 105 B and
costs something the phase document asks for. At the measured figure a 4,000-image wedding costs
about 3.2 MB, against about 48 MB for phase 14's recipes. **PERF + CTO waiver, recorded here as
section 14 of the phase document requires.**

**C5 - A coloured light is kept, and now mostly labelled. Closed as a mechanism; open as a
threshold.** `ANALYSIS_VER` 1 -> 2.

The original defect was that the preserve-mood branch keyed on the *chosen hypothesis* being
saturated, and the winner changes with how much of the wedding has been analysed: white-patch
reads the purple wash at a chroma of about 0.063 before any skin loci exist, and the
skin-anchored answer reads the same wash at about 0.041 after. One room, one light, two
readings on opposite sides of `SATURATED_ABOVE` - so a project's first dance-floor frame was
labelled and its four-hundredth was not.

Two changes close that. `illuminant::ambient` asks the two generators that measure the light
*falling on the frame* rather than the one that best explained the subject, so the answer stops
depending on project progress; and the correction between two lights now walks in `u'v'` rather
than in kelvin. The second was found by the first: a coloured light is off the Planckian locus
by definition, so interpolating its *temperature* walked every candidate back onto the locus,
none of them ever satisfied a skin constraint the wash had pushed off it, and the scan fell
through to the full correction while recording `SkinLocusConstrained` - the code that means the
mood was lost - on frames whose mood was in fact kept. That was invariant 8 being violated in
the one place it was load-bearing, and section 11's own "the solve is in CIE 1976 `u'v'`, never
in kelvin" was not true of this function.

The ambient witness is deliberately narrower than the chosen one: it fires only on a scene phase
07 marked `STAGE`, and only when the light's *kind* classifies as intentional. Both generators it
reads are the two confounded by a strongly coloured **surface**, and the red mandap clears twice
the saturation threshold on its own - without the scene-attribute test it preserved a red cast on
all five ritual frames and cost four of them the white-balance gate. That number is measured, not
argued: it is why the guard is there.

Measured on the fixture wedding: coloured-light frames labelled went from **0 of 5 to 3 of 5**,
white balance held at **42/46 (0.913)**, the surviving cast held at **106 %**, skin dE00 and its
spread did not move, and determinism holds.
`the_coloured_light_note_does_not_depend_on_how_much_of_the_wedding_is_analysed` and
`the_correction_between_two_lights_is_walked_in_chromaticity` are the two regression tests.

**What remains open, and why it was not tuned away.** The other two of the five stay unlabelled
because the fixture's own light sits at a duv of **0.0406**, below `Illuminant::SATURATED_ABOVE`
of 0.055 - so by the product's current definition that wash is not saturated enough to be a
creative choice, and the frames that *do* label only clear the bar because white-patch
over-reads them. Lowering the constant, or making it scene-conditioned as invariant 7 would
argue for, is a change to a frozen contract and needs an ADR. It also needs a photograph: tuning
a perceptual threshold until a synthetic fixture flags would be fitting the number to a duv
somebody picked when writing the fixture, which is condition C1's failure mode exactly. It waits
for real camera files.

**C6 - The three-OS CI matrix does not exist.** Phase 02's condition, inherited for the fifth
time. Determinism is asserted within one build on one machine.

It stopped being theoretical during this phase's completion pass. Phase 08's
`the_label_files_and_the_rust_fixtures_agree` located its ground-truth files by string-editing
the *Windows* spelling of a path suffix out of `CARGO_MANIFEST_DIR`; on a Linux runner the
`replace` matched nothing, the test looked for `crates/aura-brain-wedding/tests/fixtures/labels/`
and the phase 08 gate failed. It is fixed - it now joins its way up from the crate root as every
other fixture path in the workspace already did - but a green suite on one operating system is
exactly the assurance this condition says the product does not have, and this is what that costs.

**C7 - No demo recording.** Section 14 asks for the feature running on a real 3,000-image
wedding. There is no such wedding here.

## 9. What was deliberately not built

**The learned hypothesis is generated by nothing.** Section 6.2 lists a learned CNN prediction
as one of four generators, and the model ships, signed, with a card. It is never asked for a
prediction while it is untrained, because a random projection of a thumbnail offered with a 0.60
prior would *win* on exactly the frames where the honest estimators disagree - which is the set
of frames that most need a right answer. Nothing is stubbed for it; flipping
`WB_HEAD_TRAINED` is the whole change.

**Nothing corrects a mixed-light frame locally.** Section 2.1 puts that in phase 18 and this
phase marks the frames for it. There is no mask, no region correction and no field that could
hold one.

**No skin locus reaches the wire.** ADR-0032 section 4. The panel gets counts.

## 10. Two rules this phase adds and every later phase inherits

**`ToneService` is the only way to ask what colour the light was.** Eleventh service of its kind.
Phase 16 grades on top of these values, 17 shifts them, 18 corrects locally against them, 25
normalises a gallery toward them, 26 matches two cameras with them and 27 checks them. Two
answers to "what temperature was this room" is an album that does not match the gallery.

**A skin target is measured, never assumed - and the schema cannot express an alternative.**
There is no ideal-skin constant in the contract, the migration, the config or the code. Section
6.3's argument is that a fixed target is how an editor lightens dark skin while believing it is
correcting a cast; the defence is that nothing in this code path has a constant it could compare
a person against, and the gate checks for one on every run.

## 11. Four decisions worth remembering because they will be re-argued

**The white-balance confidence is built on agreement, not on a cost gap.** It was built on a cost
gap first, and that was wrong in a way that took a whole synthetic wedding to see: two independent
estimators landing on the same chromaticity is the strongest evidence available, and a cost gap
collapses to zero in exactly that case. The wrong version scored every frame below the
contribution threshold, so no skin sample was ever taken, so no locus was ever built, so section
6.3's hard constraint bound on nothing - silently, while every unit test passed. A confidence model
that cannot bootstrap the mechanism it gates is worse than none, because it fails invisibly and
looks careful.

**The correction is a linear scan rather than a bisection, and it walks in `u'v'`.** With two
people whose loci differ, the set of corrections that satisfies both is not an interval. A
bisection returns an arbitrary point in it; a scan returns the first, which is the least
correction that works. The *space* half was got wrong first and is the more interesting error:
the scan interpolated a colour temperature, which walks along the Planckian locus, which is the
one path guaranteed to miss an off-locus light - so the mechanism built to preserve a coloured
light could not land on any coloured light. It failed silently, into a reason code that said the
opposite of what had happened, and every test passed. Two of the four decisions on this list are
now the same lesson: a mechanism that cannot reach the case it exists for is worse than an absent
one, because it reports.

**The store keeps both answers.** A row with `user_edited = 1` still carries AURA's own numbers.
That is what lets the review queue show a disagreement and phase 30's learning loop read one - and
it only works because something can read the other side, which is why `ToneStore::override_of`
exists beside the frozen service.

**The evidence lives at the precision it can survive.** A temperature and a tint are written into
`aura_recipe::Global`, whose fields are `u32` and `i16`; a reason's numbers are printed with
`{:.0}`. Storing more than that is storing noise, and here it was storing noise against the
tightest per-image budget in the product.
