# Changelog

All notable changes to AURA. One entry per phase, newest first.

## Phase 13 - Explain My Edit, confidence calibration and the decision ledger

Every decision the product makes can be opened up - why, how sure, what it looked at - and
every one of them is written to a ledger that cannot be rewritten. A correction is a new
entry pointing at the old one, and nothing in the product can update a row that says what
happened.

### Added

- **`aura-explain`**: the ledger, with append-only semantics the database enforces and a
  compaction policy that cannot remove a photographer's own decision; a decision builder
  whose canonical JSON and inputs hash exist so a replay compares the question rather than a
  rounding difference; isotonic and temperature calibration with ECE, Brier and reliability
  bins; the autonomy policy; the reason registry; the grounded summariser; the replay port;
  and the anonymised support bundle.
- **Migration 13**: `decisions`, `decision_reasons` and `calibration_models`, one trigger
  that aborts every `UPDATE`, one coverage view and three indexes. `reason_count` is a
  denormalised column with a CHECK, which is how invariant 2 becomes something SQLite
  refuses to break.
- **`autonomy_bands.toml`**: section 6.4's bands verbatim, five per-kind rows each with a
  written reason, and a loader that refuses a row with no reason or thresholds that do not
  descend. `irreversible` is read from the enum and never from the file.
- **`docs/reason-codes.md`**: 93 codes across five vocabularies, generated from the registry
  so the public reference cannot disagree with the product.
- **`docs/how-confidence-works.md`**: what the number means, what it does not mean yet, and
  what AURA is allowed to do at each level.
- **The Explain panel and typed IPC surface**: eight commands, six tabs, evidence crops, the
  alternative comparison with both score breakdowns, and a confidence badge that says plainly
  when nothing has been calibrated.
- **`aura-cli replay`**: re-derives a stored decision from the catalog as it stands now and
  says whether the answer moved - and if it did, whether that is an upgrade or a determinism
  defect.
- **`ExplainSummary`**: the one cloud call this phase may make. No images, no field a new
  reason could go in, and a validator that refuses any number absent from the input.
- **`ml/eval/calibration_report.py`**: the same arithmetic as the Rust side, plus a
  reliability diagram written as SVG with no plotting dependency.
- Six error codes, `AURA-ML-5054` to `AURA-ML-5059`, each with a runbook.

### Changed

- `aura-core` freezes `contract::ledger` and gains `DecisionId` in the frozen `ids.rs`,
  alongside phases 06, 07 and 08's ids. ADR-0027 records the five spellings that differ from
  the phase document.
- `contracts.lock` covers `ledger.rs`, `ids.rs`, migration 13, the IPC surface and
  `ui/src/ipc/types.ts`.

### Known limits

- **Nothing is calibrated.** Every model is the identity map at version 0, the ECE gate is
  measured against synthetic predictors whose error is authored, and `AURA-ML-5058` says so
  once per run. While that is true, every decision is raised one band toward review - so
  nothing in this build acts unattended, and phase 28 cannot ship until a calibration does.
- **Every decision recorded here was made from placeholder heads**, because phases 06, 09, 10
  and 11 all ship them. The ledger records those decisions faithfully; none of them is a
  claim about a photograph.
- **The cloud summary has a cassette and no live provider.** The paragraph a photographer
  sees today is the deterministic template, which is correct by construction.
- **The pixel opt-in of section 2.1 was deliberately not built.** It would be the one code
  path in the product that can put a photograph into a file which is then emailed.

## Phase 12 - Autonomous culling engine, story coverage guard and gallery sizing

A wedding becomes a gallery. Every photograph on both sides of the line carries a reason,
twelve parts of the wedding are guaranteed against every threshold in the product, and
nothing is deleted: a rejection is a row, and it is one click from being overturned.

### Added

- **`aura-cull`**: score fusion as a weighted geometric mean, so no signal can rescue
  another; three hard vetoes read off phase 09 measurements rather than re-derived; a
  moment pass whose keeper count follows how much the moment varied; chapter quotas with a
  bounded local search that trades a second keeper for an unrepresented moment; the
  coverage guard, run twice; three sliding-window diversity caps; and a gallery-size model
  with a reconciliation that adds runner-ups rather than lowering the bar.
- **Migration 12**: `cull_run`, `selection`, `rejections`, `coverage_report` and
  `cull_override`, two views, and three provenance versions plus a digest of the two
  configuration files. The photographer's own keeps and removals live in their own table
  because a re-selection rebuilds every other one.
- **`cull_weights.toml`**: 22 scene rows and three mode rows, every one with a written
  rationale, and a loader that refuses a row weighting framing above whether the photograph
  worked.
- **`coverage_rules.toml`**: twelve declarative guarantees, per-identity minimums, nine
  chapter bands, the diversity caps and the veto policy. An unknown must-have slug is a
  refusal rather than a default, and a table that lists the kiss as a posed scene - which
  would let AURA veto it for closed eyes - is refused outright.
- **The cull view and typed IPC surface**: coverage, gallery, one photograph's decision, run,
  resize, mode switch and a three-valued override. The three coverage states are rendered as
  words rather than colours, and an unanalysed photograph offers no override at all.
- **`docs/how-aura-culls.md`**: what the engine does, what it guarantees, what every reason
  code means, and the one number to check before delivering.
- **Gates**: `aura-cli verify --phase 12`, a 24-test harness, a self-testing Python
  agreement harness, four checked-in keeper label files and two asserted budgets.

### Changed

- `aura-core` gained the frozen `cull` contract; `CullService` is now the only way any
  phase may ask what is being delivered.
- The catalog schema version is 12.

### Known limitations

- Every sub-score underneath every decision comes from a placeholder head (phases 06, 09,
  10 and 11). The arithmetic is real and tested; the numbers it works on are not yet claims
  about photographs. Condition C1 in `docs/progress/PHASE-12-EXIT.md`, and it closes with
  phase 05's C10.
- The per-scene calibration ships as the identity map, and the gallery-size regression is
  authored rather than trained on real delivered galleries.
- The blind photographer study of section 13 does not exist; agreement is measured against
  four synthetic weddings with documented labels.
- The optional cloud tie-breaker was not built. Its trigger is two scores within 0.02 of
  each other, and with placeholder heads underneath that is noise rather than a tie - so
  every call would be a paid question about nothing. Condition C6.

## Phase 11 - Composition and aesthetic AI

Every photograph now carries an explainable framing reading: whether a reliable horizon
is level, what the edge cuts, how the subject is placed, whether visual weight is balanced,
and which measured background regions compete for attention. It is evidence for culling
and geometry phases, not a crop or a selection.

### Added

- **`aura-brain-photo::composition`**: rho-coherent horizon measurement with intentional
  dutch-angle handling; pose/face-aware headroom and crop auditing; thirds, centre,
  negative-space and balance measures; background edge energy, bright regions, head merges
  and colour competition; a bounded aesthetic term; stable reasons, evidence rectangles,
  crop hints, persistence, dismissal, resume, telemetry, and relative-within-moment score.
- **Migration 11**: `image_composition`, one review-queue index, coverage and flag views,
  three provenance versions, compact evidence JSON, and photographer dismissals that
  survive re-analysis.
- **`composition_rules.toml`**: a neutral fallback and 22 scene-conditioned rows with
  rationales, including explicit allowances for centred details, deliberate close crops,
  and intentional tilt.
- **Two signed architecture fixtures**, `pose_keypoints` and `aesthetic_head`, with model
  cards; guarded training/evaluation/export tools in `ml/models/composition/`.
- **The Composition card and typed IPC surface**: project status, one-photo reading,
  flagged review queue, one-note dismissal, resumable analysis, and normalised evidence
  overlays. The card explicitly distinguishes clean, exonerated, unavailable, and
  unanalysed states.
- **Five error codes and runbooks**, `AURA-ML-5043` to `AURA-ML-5047`; ADR-0023 for the
  rules/contract and ADR-0024 for the application boundary.
- **`aura-cli verify --phase 11`** and a composition performance/storage suite. The
  algorithm evaluation contains 37 authored synthetic regression tests.

### Changed

- Horizon confidence now requires a coherent line in both angle and offset, preventing a
  repeated diagonal texture from being called a strong horizon.
- Neutral or white subjects can still receive a colour-competition reading from saturated
  background energy, and subject colour is sampled from the dominant head before using a
  coarser body region.
- Mid-limb crop severity and unlocated reference poses now agree with the crop gate and
  placement semantics instead of silently falling just below the flag boundary.

### Not built, deliberately

This phase does not crop, straighten, remove a distraction, keep, reject, or order a
gallery. Crop hints are advisory data for phase 23. Generic background measurements do
not claim to recognise an exit sign, bin, mirror, or rubbish; semantic re-validation waits
for phase 18 and removal belongs to phase 24.

### Known limits

Both checked-in heads are untrained deterministic placeholders, so the analyser does not
claim their output is learned. All quality numbers are against authored synthetic frames
or reference geometry, not the three reference weddings or a photographer panel. No GPU
backend or three-machine CI is available here, so the two GPU budgets retain ADR-0007's
waiver. Calibration, demographic/cultural slices, the 300-frame perceptual audit, semantic
background categories, and the real-wedding demo remain explicit conditions in
`docs/progress/PHASE-11-EXIT.md`; the placeholder-model condition is a Sev 2 trigger.

## Phase 10 - Expression, emotion and moment ranking

The app finds the moments that matter - genuine smiles, laughter, tears, hugs, kisses,
reactions and ritual peaks - and ranks every frame by emotional value. Phase 09 decided
what is *acceptable*; this decides what is *worth delivering*, and the two are separate
numbers that a later phase combines.

The whole phase is shaped by one risk, and like phase 09's it is not a technical one: an
emotion model built somewhere else learns that a moment is a big smile, and delivers a
Hindu ceremony as an empty gallery. So composure is a **positive** reading rather than the
absence of one, in the four ceremony scenes it is weighted at or above a smile, three
traditions raise it further, and the file that does all of that is a table a person can
read with a written reason on every row.

### Added

- **`aura-brain-wedding::emotion`**: eight continuous readings per face from an aligned
  crop; gaze measured from phase 06's eye landmarks rather than predicted; nine
  interactions from the whole frame with a person-prior plane; a smoothed peak curve per
  moment that refuses to name an apex when there is not one; reaction linking across
  cameras inside a four-second window; and a nine-feature Bradley-Terry ranker whose
  coefficients are a list somebody can argue with.
- **Migration 10**: `image_interaction`, `face_expression`, `moment_peak`,
  `reaction_links`, `emotion_preferences` and two coverage views. 733 bytes per image
  against a 900-byte budget.
- **`emotion_weights.toml`**: 22 scene rows, 5 tradition rows, 9 ranker coefficients and 2
  calibration tables. The loader refuses eight things, including a row with no rationale
  and a calibration map that would reorder frames.
- **Two signed models**: `expression_head` (112 px crop, eight sigmoids, int8 forbidden)
  and `interaction_head` (160 px frame in four planes, nine sigmoids, int8 permitted).
  Both untrained; both carry cards that say so at the top.
- **`MomentSignificance`**, the one cloud call this phase may make: six 768 px thumbnails,
  anonymised role handles, at most 25 calls a wedding, and a validator that refuses a
  reason containing any of twenty appearance or psychology words.
- **The Emotion card and the moment browser**: face crops with eight bars each, interaction
  chips, a three-state peak indicator and a reaction pair viewer. Seven IPC commands, five
  of them reads.
- **Five error codes** with runbooks, `AURA-ML-5038` to `AURA-ML-5042`, and two ADRs.
- **`docs/emotion-and-moments.md`**, whose first section is titled "AURA describes
  photographs. It does not read minds."

### Changed

- **Phase 09's third eye-intent rule now fires.** `IntegrityPass::with_emotion` fills
  `IntentInput::tears` through `aura-core`'s frozen trait, so a tearful closed-eye
  photograph carries `EYES_CLOSED_OK` instead of `EYES_CLOSED`. This closes condition C4 of
  the phase 09 exit report; `analysis_ver` went from 1 to 2, which makes every stored
  technical verdict pending so the background pass re-measures.
- **The 112 px two-point face warp moved into `aura-vision`.** Phase 10's expression head
  became its second consumer, and two copies of a warp is two crops that drift apart while
  looking identical. Phase 09's 26 eval gates and 11 calibration tests pass unchanged.
- `Interaction::from_str` is spelled `from_slug`, because a `from_str` that is not
  `FromStr` is a method that gets called by accident.

### Not built, deliberately

Final selection is phase 12 and album sequencing is phase 29, so nothing here keeps,
rejects, delivers or builds a gallery - `EmotionService::ranked` returns an *ordering*, the
moment browser says "An ordering, not a shortlist" in its own header, and a test asserts no
label in it says keep, reject, deliver or cull.

Any claim about a person's inner emotional state is out of scope permanently. The twenty
things this phase can say about a photograph are a closed list, call sites do not write
sentences, and the cloud task's output has no field a description of somebody could go in.

### Known limits

Both heads are placeholders with the right architecture and no training, so every number in
section 10.1 is measured against synthetic frames whose answer is painted into the pixels.
The ranker is fitted on eight authored comparisons rather than ten thousand photographers'
ones, and four of its nine coefficients are unidentifiable from that data and set by
argument instead. Gaze is head direction rather than eye direction. The per-scene
calibration ships as the identity. The four named peak kinds are derived from the scene and
the interaction rather than trained. All five are in `docs/progress/PHASE-10-EXIT.md`
section 5, and the first is a Sev 2 trigger.

## Phase 09 - Frame integrity: focus, motion, exposure, noise and eye state

Every frame gets an honest technical verdict where it matters. Not "is this photograph
sharp" - a soft background is usually the point - but **is the right subject sharp**, was
the blur a decision, can the exposure be brought back, how noisy is it for this kind of
photograph, and are the eyes that matter open.

The whole phase is shaped by one risk, and it is not a technical one: a product that
throws away a frame it should have kept is a product a photographer stops using. So two of
the fourteen technical marks describe something *right* with a photograph, eight of the
twenty-one reason codes withdraw a claim rather than making one, and the learned focus
head is allowed to exonerate a frame and forbidden from convicting one.

### Added

- `aura-brain-photo`: a new crate, and the first that judges pixels rather than reading
  rows. Subject-aware sharpness from three classical measures over eye, face, body and
  background regions; motion intent from a structure tensor, because motion blur is
  directional and defocus is not; recovery-aware exposure with a specular-highlight
  exclusion, so a candle flame is a light source and a blown dress is a loss; noise
  measured in flat regions and expressed against what the scene tolerates; eye state with
  section 6.4's four intent rules.
- **A camera calibration table for twenty bodies.** "Sharp" means sharp *for this gear*: a
  61 MP body and a 24 MP body produce different edge detail in the preview AURA reads, and
  without the division the more expensive camera would win every comparison. A body with
  no row is judged more cautiously and the panel says so.
- **Closed eyes are often the photograph.** A kiss, a prayer, a first look, somebody crying
  at a toast - `EYES_CLOSED_OK` marks those as right rather than wrong, and only the people
  a photograph is *about* have their eyes judged at all.
- Migration 9: `image_integrity` and `face_eye_state`, plus a coverage view and a flag
  histogram. Nothing in the schema can reject a photograph, and "not checked" is
  deliberately distinguishable from "clean".
- Two signed models with cards - `focus_head` and `eye_state` - and the training,
  evaluation and export scripts in `ml/models/integrity/`.
- The integrity IPC surface (ADR-0020): six commands, five of which are reads. The
  Integrity card shows the crop that caused each penalty; the filter chips offer soft,
  blinked, blown and noisy, and read their names from the backend rather than keeping a
  second copy of the flag list.
- `docs/frame-integrity.md`: every mark in the words the product uses, and a build that
  fails if a reason code is added without one.
- `aura-cli verify --phase 09`, eleven checks, exit 0.

### Changed

- `FaceRef` gains a bounding box and the two eye landmarks. Phase 09 cannot measure an eye
  region or show the crop behind a closed-eye mark without them; the nose and mouth
  corners stay out, which is what keeps that type's promise that it carries nothing a
  recogniser needs. ADR-0019 section 3.
- The moments view's error toasts said `undefined`. Five call sites read a field the wire
  type does not have; fixed here because it was a one-word change.

### Known limitations

- Both learned heads ship **untrained**. Every accuracy figure in this phase is measured
  against images whose answer was known in advance, which proves the arithmetic and says
  nothing about photographs.
- The twenty calibration rows are derived from published specifications rather than
  measured from bodies, because there are still no camera files in this repository.
- Clipping is measured on the preview rather than on the RAW histogram.
- The "there are tears here" intent rule needs phase 10 and is wired through as always
  false. A tearful closed-eye frame may reach a review queue; it will not reach a delivery
  decision, because this phase makes none.

## Phase 08 - Smart burst grouping and duplicate detection

Three thousand loose files become a few hundred moments. From this phase onward the
product works on **moments** rather than on files, and the difference is not efficiency:
rejecting a burst is a moment lost, whereas rejecting individual frames is tidying, and
phase 12's coverage guarantees are written against the first of those.

The first phase since 02 that ships no model. Grouping is arithmetic over phase 05's
vectors and phase 01's timestamps, which is why three of section 11's four budget rows
are met by two to three orders of magnitude rather than by a margin.

### Added

- `aura-brain-wedding::moments`: seven modules that turn a timeline into a two-tier
  structure. A **moment** is one thing that happened; a **burst** is one press-and-hold of
  the shutter inside it. Fourteen frames of a bouquet toss are one moment, and the six
  that came off at 10 fps as it left her hand are one burst inside that.
- An adaptive cadence estimator, per camera. The burst window is
  `clamp(2.5 x median_interval, 0.7 s, 8 s)` over a rolling 60-second neighbourhood, so
  a 10 fps burst and a ceremony shot in ones and twos are both handled by the same rule.
  Two photographers interleaved on one timeline have a combined median of roughly half
  of either's, which would halve the window for both - so cadence is estimated per body
  and the merge happens later, where it can be justified rather than inferred from an
  arithmetic accident.
- A time-windowed similarity graph, never all pairs. A 4,000-frame wedding has eight
  million pairs and about sixty thousand candidates, and only the second number gets
  scored - which is the whole of why 4,000 images group in ten milliseconds against a
  six-second budget.
- **Time proximity became evidence rather than only a gate.** Section 2.1 lists it first
  among the grouping signals and section 6.2's four-term score has no time term at all;
  without one, a ceremony shot at one frame every eight seconds chains into a single
  moment for as long as the photographer keeps shooting, because every consecutive pair
  is inside the eight-second clamp and every consecutive pair looks alike - the altar has
  not moved. The four documented weights are untouched and their sum is scaled by a
  proximity factor. ADR-0017 section 3.
- Scene-conditioned grouping thresholds in `moment_profiles.toml`, a sibling of
  `scene_profiles.toml` with the same rules: no rationale, no load. Ten scenes are argued
  over and twelve take the defaults, and the file names which twelve so a reader can see
  what was actually decided. `dance_floor` groups at 0.60 and `family_portrait` at 0.76,
  because two consecutive family groups are visually almost identical and are two
  different deliverables.
- Duplicate classification as a **conjunction** of three independent tests, not a
  disjunction: a difference hash within four bits, an embedding distance within 0.03, and
  the faces in the same places. A hash is blind to a blink, an embedding is blind to a
  stop of exposure, and the face overlap is blind to everything else - three blind tests
  that must all agree is a far stronger claim than one confident one, which is what
  section 10.1's demand for 0.98 recall at 0.95 precision actually asks for.
- Cross-camera merging on temporal overlap above 60 % and medoid distance under 0.12,
  measured against the *shorter* of the two spans - a two-second burst inside a
  forty-second sequence overlaps their union by 5 % and is entirely inside it. The merged
  moment keeps its per-camera bursts intact, so a bad merge is split back along the line
  it was joined on.
- Migration 8: `moments`, `moment_images`, `duplicates` and `moment_edits`, with three
  version columns because they invalidate three different things.
- Nine IPC commands, a stacked moments grid and a side-by-side duplicate review panel.
- Five error codes, `AURA-ML-5028` to `AURA-ML-5032`, each with a runbook.

### Fixed

- **AURA could not see a burst at all on a real camera file.** EXIF's `DateTimeOriginal`
  has whole-second resolution, so fourteen frames of a 10 fps burst carry one timestamp
  between them; the fraction lives in `SubSecTimeOriginal`, which phase 01 stores
  separately in `photo.sub_sec`. Every unit test passed and the phase gate failed.
  Reconstructing the fraction took grouping accuracy from 0.000 to 1.000 on two of the
  five regression patterns. It is the most consequential thing found in this phase and no
  synthetic fixture would ever have exposed it.

### Changed

- `catalog.count` accepts the four new tables.
- `photo.camera_serial` is now a documented fallback when a `camera` row does not exist
  yet, so a project part-way through import does not look like a single-body wedding.

### Known limits

- **The embedding underneath is a placeholder** (phase 05 condition C10) and it is the
  largest term in the grouping score. Every number in this phase is measured against
  authored ground truth, and none of them is a claim about a real wedding's pixels. This
  is condition C1 in `docs/progress/PHASE-08-EXIT.md` and it is a Sev 2 trigger.
- Phase 06's two face signals are not wired in. `PeopleService` has no bulk accessor for
  either, and adding one is a phase 06 contract change. Every resulting degradation is in
  the safe direction: a skipped face test makes a near-identical claim *harder*.
- Extra storage per image is 319 bytes against a 200-byte budget, waived at 340 by PERF
  and CTO in ADR-0017 section 8. Four schema decisions took it down from 720; the
  remaining gap is 40-character text ids and the reasons invariant 2 requires.
- Nobody has looked at a moment stack for a wedding they attended.

### Not built here, deliberately

Choosing the winner of a burst. That is phase 12, and the boundary is structural rather
than remembered: no `culled` column, no rank, no rejection anywhere on the IPC surface,
and `keep_hint` spelled *hint* in the contract, the schema, the wire and the panel.

## Phase 07 - Wedding scene AI and story timeline segmentation

The app reads the wedding as a story. Every photograph gets a scene label, fourteen
attributes and a confidence; the day is split into ordered chapters with boundaries,
counts and durations. From this phase onward no threshold in the product is global - a
dark dance frame and a formal family portrait are judged by different rows of the same
table, which is invariant 7 finally becoming a lookup instead of a promise.

### Added

- `aura-brain-wedding`: the scene half and the story half of the wedding brain. Nothing
  in it opens a pixel. The classifier is a small adapter on the *frozen* phase 05
  embedding - section 6.1's design - which is why scene inference for four thousand
  images fits in eight milliseconds of arithmetic where phase 06's face pass needs twelve
  minutes for the same wedding.
- A 22-class scene head and fourteen independent attribute sigmoids on one adapter. The
  abstention is deliberately **not** a softmax slot: a model cannot usefully be trained to
  say "I am not sure" through an output that competes with the classes it is unsure
  between, so `SceneId::Unknown` is a decoder decision from the top-1 margin - and the
  margin, not the confidence floor, is what actually rejects.
- Four of the fourteen attributes are **decided rather than predicted**. `flash`,
  `night`, `tungsten` and `indoor` are recorded exactly by the camera or by the phase 05
  luminance statistics, and where a measurement exists it beats the head. A trained model
  will still be wrong about flash on a frame lit by a window at 1/200th; the EXIF will
  not.
- A tradition-conditioned ritual head with **two** abstention mechanisms, because they
  answer different questions. Slot 0 is "no rite" and competes in the same softmax as
  every rite; the margin handles the case where the head has correctly identified a fire
  circumambulation and cannot tell whether to call it `saptapadi_pheras` or `saat_phera`.
  Naming either at 0.36 would put a Nepali wedding's rites under Hindu names in a
  client-facing timeline.
- Forty-eight rites across five traditions - Hindu, Nepali, Christian, Muslim, civil - in
  editable config files, with `docs/adding-a-tradition.md` as the procedure a
  photographer's consultant can follow without a compiler. The rite's authored id **is**
  the model's output slot, which is why a duplicate is refused rather than resolved.
- HMM smoothing over nine chapters before segmentation rather than after. A single
  misclassified frame in the reception is a wrong label; fed to a change-point detector it
  is a two-frame "Getting Ready" chapter between the speeches and the cake, and by then no
  amount of smoothing helps.
- PELT change-point detection over a three-term fused signal, with the penalty **searched
  in log space** rather than fixed. A penalty tuned on a ten-hour Hindu wedding gives two
  chapters for a registry office and forty for a three-day Nepali wedding; the search is
  what makes section 10.1's 6-to-20 chapter band hold on all three.
- `scene_profiles.toml`: twenty-two scenes, each with tolerances, weights, an editing
  intent, a coverage flag and **a written rationale**. The loader refuses a profile
  without one. That friction is the point - somebody who cannot write a sentence
  explaining why the dance floor tolerates three times the ceremony's noise has not
  finished deciding it.
- Migration 7: `image_scenes`, `segments`, `segment_images` and `scene_profiles`, plus two
  views. The user-override guards are inside the statements that would overwrite them, not
  around them: a read-then-write leaves a window in which a photographer loses a race with
  a background pass.
- Nine IPC commands and the story timeline. Chapter cards are sized by **duration**, not
  by frame count - a ninety-minute dinner and a six-minute cake cutting with forty
  photographs each are not the same shape of event. Moving a boundary locks both chapters
  either side of it, because a boundary is shared.
- The `SegmentNaming` cost policy: at most sixteen calls per wedding, least-confident
  first, locked chapters never priced, and phase 04's rule enforced - a cloud answer may
  not overrule a local decision at 0.90 or above without citing visible evidence, and the
  conflict is logged.
- Two signed placeholder models with cards, six error codes with runbooks, two ADRs, and
  four training and evaluation scripts under `ml/models/scene/`.

### Changed

- `aura-people` now receives real scene labels. **Half of phase 06's condition C3
  closes**: the couple contest's getting-ready, ceremony and portrait terms turn on,
  `RoleOutcome::scene_starved` is false on a classified wedding, and
  `SCENELESS_CONFIDENCE_CEILING` stops capping the couple decision at 0.62.
- `xtask` learned that a model can take a feature vector rather than pixels. The two scene
  heads declare `[N, 528]` and `[N, 536]` with an `NC` layout and an `unbounded` range, so
  the runtime's shape check passes and the manifest documents a normalisation nobody
  performs.
- The catalog's countable-table list gained the four new tables.

### Fixed

Three bugs the evaluation harness found in code that read correctly, recorded because each
one is an argument for building the fixtures before the gates.

- The penalty search never reached its own range. Linear bisection of `0.0005..40` spends
  its first ten steps between 40 and 0.04, and one fixture's answer is 0.008 - so it fell
  back to gap-only segmentation and produced three chapters against a six-chapter floor.
- Masking the ritual head by tradition made it abstain *more*, not less. Zeroing another
  tradition's slots without renormalising left the distribution summing to under one, so
  establishing the tradition made naming a rite harder rather than easier.
- The per-image storage estimate was 25 % low: 330 bytes claimed, 410 measured, against a
  400-byte budget. Writing the top-3 as pairs rather than as objects closed it - the words
  "scene" and "score" repeated three times per photograph were a fifth of the budget.

### Known limitations

Both models are **placeholders with no training**, which is condition C1 of
`docs/progress/PHASE-07-EXIT.md` and a Sev 2 trigger. Every number in section 10.1 is
measured against synthetic ground truth whose answer is known by construction: that proves
the algorithms and says nothing about the weights. No later phase may claim a quality
result that depends on scene classification being accurate until it closes.

One phase 06 budget - `identity_cluster_skeleton` - does not reproduce on the development
machine: 21.7 s against a 12 s budget where the phase 06 report records 2.1 s. It was
ruled out as a phase 07 effect by measurement and is recorded in section 4 of the phase 07
exit report for PERF to resolve against phase 06.

Per-tradition accuracy is **not published and not approximated** - condition C5, the
second Sev 2 trigger. The disparity this phase risks is cultural rather than demographic,
which is precisely the gap section 1 claims as a competitive moat, and an unmeasured
version of that claim is one the product cannot support.

## Phase 06 - Face detection, recognition and people intelligence

The app learns who matters at this wedding: it finds every face, groups them into
identities, and ranks the couple, close family and VIPs by evidence rather than by
guesswork. Every later decision gets a subject hierarchy, so sharpness on the bride's face
outranks sharpness on a stranger's elbow.

### Added

- `aura-vision::face`: one decoded frame in, everything phase 06 needs out. Detection with
  a letterbox rather than a centre crop - the faces the tiled pass exists to recover are
  the ones at the edges of a wide ceremony frame - three output strides from one forward
  pass, and faces and bodies predicted by the same anchor, which is why the phase ships
  three models and not four.
- A conditional 2x2 tiled pass that fires on wide-angle frames with several small
  detections, and on frames where bodies were found and faces were not. Its cost is
  recorded per frame in `face_scan.tiled` and reported by `ScanReport::tile_ratio`, because
  "tiled detection doubles cost" is a failure mode to measure rather than assume.
- A bokeh gate that works by geometry rather than by score: a blurred highlight has no
  landmark structure, so its five points collapse towards its centre.
  `Detection::landmark_spread` measures that, which lets the objectness threshold stay low
  and keeps small-face recall.
- ArcFace alignment: a closed-form Umeyama similarity transform onto the published 112 px
  layout, never affine, because an affine fit to five points can shear and a sheared face
  is a different face to a recogniser. Head pose is estimated from the same five landmarks.
- A quality gate that decides which faces may vote on identity: four measured factors -
  sharpness, occlusion, pose, exposure - combined as a weighted **geometric** mean, so a
  perfectly exposed, perfectly frontal, completely out-of-focus face cannot score 0.75 and
  vote, plus two hard cut-offs where the evidence genuinely runs out. A face below the gate
  is detected, stored and displayed; it just does not vote.
- Identity clustering with **exact** average linkage computed from running sums: for unit
  vectors the mean pairwise cosine distance between two clusters is one minus the dot
  product of their unnormalised means, so exact average linkage costs one dot product per
  cluster pair rather than `|A| x |B|`.
- Relative-cohesion verification, which is what actually prevents the chain merge. Two
  looks of one person sit about 1.7 times their own internal spread apart; two siblings sit
  at three times it. A wedding of near-lookalikes records refusals rather than producing
  one identity for six people.
- Sub-centroids for an identity whose members span two looks - the outfit and hairstyle
  change - so a face from either look still matches.
- Role inference from photographic evidence only. **Automation never assigns `bride` or
  `groom`**: the evidence identifies a pair, which of two people is the bride is not a
  photographic fact, and the couple may be same-sex. Confidence is capped at 0.62 while
  scene labels are missing, and the reason string says why.
- Prominence scoring with a versioned weight file, scene-conditioned tables, and
  `subject_focus_score` - the prominence-weighted sharpness phases 09 and 12 use instead of
  naive global sharpness.
- `aura-people`: the sealed biometric store. Templates, centroids and 112 px crops are
  encrypted with a key derived from a per-project secret in the operating system's
  credential store, using BLAKE3 encrypt-then-MAC with a **synthetic nonce** - so
  re-scanning after a model change cannot reuse a keystream, and sealing stays
  deterministic.
- Migration 6: `face_vault`, `face_scan`, `identities`, `faces`, `identity_links`,
  `person_boxes`, `cooccurrence`, and two views. `face_scan` is new in kind: "no faces in
  this frame" is a legitimate result, so the resumability ledger records the *look* rather
  than the finding.
- Merge, split, rename, mark-couple and an importance slider, all undoable, all recorded in
  an append-only journal, and all replayed onto a fresh grouping **by face set rather than
  by identity id** - so a photographer's decision survives a full re-analysis even though
  re-clustering produces new ids.
- Biometric erasure that deletes the credential-store entry *first*, so a crash mid-erasure
  leaves unreadable data rather than readable data, then the crops, then the rows, then
  verifies that nothing survived. Culling and edit decisions are untouched.
- `CoupleHint`, the one cloud call phase 06 may make, behind an ambiguity trigger and a
  two-call cap. Candidates are opaque handles, so a model that answers with a description
  of a person - or volunteers a gender - fails validation rather than being stored.
- Three signed models with cards: `face_detect`, `face_embed`, `face_quality`. `int8` is
  forbidden on the detector, because quantising a box regression moves a 40 px face by
  several pixels, and on the quality head, because quantising four sigmoids destroys the
  resolution the 0.4 gate needs.
- The people IPC surface and the People panel, plus `aura-cli verify --phase 06` as the
  gate: thirteen checks, from the migration to an erasure that leaves nothing behind.
- Nine error codes with runbooks: `AURA-ML-5017` to `AURA-ML-5021` and `AURA-SEC-9001` to
  `AURA-SEC-9005`.

### Changed

- `aura-core` gained the frozen people contract - `Role`, `SubjectHierarchy`,
  `ImageSubjects`, `PeopleService` - and two typed ids, `FaceId` and `IdentityId`. It still
  depends on no other workspace crate.

### Known limitations

- **The three shipped models are placeholders.** The detector finds no faces in a
  photograph and the recogniser's templates carry no identity information. Every gate in
  section 10.1 is measured against synthetic ground truth with a known answer, which proves
  the algorithms and says nothing about the weights. Condition C1, a Sev 2 trigger.
- The quality head's trust weight is 0.0, so the gate is four measured factors. Condition
  C2.
- No demographic analysis is published: the fixtures use one skin tone, and a fairness
  number computed from them would describe a renderer. Condition C5, a Sev 2 trigger.
- The two GPU throughput budgets are waived with an expiry condition; a measured
  processor-path row replaces them.

## Phase 05 - Perceptual embeddings and the wedding similarity index

Every image gets a compact perceptual embedding plus a fast similarity index, so
the app can answer "what looks like this?" across a wedding in milliseconds. It is
the shared vector substrate that scene clustering, burst grouping, duplicate
detection, people grouping, reference-frame selection and consistency checks all
reuse - computed once, in one pass.

### Added

- `aura-index`: the frozen `SimilarityIndex` contract and a deterministic HNSW
  graph behind it - `M = 32`, `ef_construction = 200`, `ef_search = 64`, cosine
  distance on L2-normalised fp16 vectors. Levels come from `blake3(image_id)`
  rather than a generator, every tie breaks by `timeline_ts` then `image_id`, and
  the parallel build is batched rather than concurrent, so two machines with
  different core counts produce byte-identical graphs.
- Filtered queries: k-nearest neighbours, radius search, time-windowed search as a
  pre-filter over a sorted timeline (not a post-filter, which is what keeps a burst
  query under a millisecond), camera restriction, exclusion sets, medoids and
  centroids.
- `aura-vision`: one decode, five results. The embedding, a 64-bit difference hash,
  an 8x8x8 HSV histogram, six luminance statistics and an edge-energy summary all
  come out of the same buffer, which is then dropped - a 4,000-image wedding is
  never 4,000 resident proxies.
- A persisted graph snapshot with six named refusals - missing, wrong magic, wrong
  format, wrong graph parameters, wrong model or preprocessing version, failed
  digest - each of which is a warning and a rebuild rather than a failure to open
  the project. A second open of a 4,000-image wedding is a 23 ms read.
- `wedding_embedding` 1.0.0, signed into `models.lock` with a model card. **It is a
  placeholder backbone**: there is no labelled wedding data in this repository and
  no GPU backend, so a ViT-B/16 with a contrastive head cannot be trained or run
  here. Everything around it is real. See ADR-0011 section 3, and condition C10 in
  the phase 05 exit report.
- `ml/models/embed/`: the dataset specification as executable code - wedding-level
  splits, a cross-tradition holdout, positive and hard-negative mining, and an
  augmentation policy that *cannot express* a flip or a heavy crop - plus the
  contrastive loss, the training schedule, the four evaluation gates and an
  exporter that reproduces the shipped model byte for byte.
- Migration 5: `embeddings` and `descriptors`, 1,623 bytes per image against a
  1.6 KB budget, reversible in three statements.
- The similarity IPC surface (ADR-0012): five commands, five DTOs and three
  telemetry events, plus `ui/src/components/SimilarPanel.tsx` - the debug "find
  similar" panel section 8 calls "invaluable for later phases". No command returns a
  vector, and a test enforces that.
- `aura-cli verify --phase 05` and `just phase-05-verify`: two cards of RAW
  fixtures, a cancelled pass that does nothing, a real pass, the index, a
  five-millisecond query, a time window, a camera filter, the snapshot and its
  refusals, an incremental second card, and determinism through the whole path.
- Four error codes with runbooks: `AURA-ML-5013` (unusable vector),
  `AURA-ML-5014` (snapshot rejected), `AURA-ML-5015` (embedding version drift),
  `AURA-ML-5016` (project past the documented in-memory ceiling).

### Changed

- `cosine_distance` accumulates into eight fixed lanes rather than one, so the
  compiler can vectorise it. With borrowed neighbour lists in place of cloned ones
  this took a 4,000-vector build from 13.3 s to 2.74 s. The lane count is fixed, so
  determinism is unaffected.
- `just budgets` and the CI budget lane run with `--test-threads=1`. A budget suite
  whose cases race each other measures the harness.
- `aura-catalog` gains `repo::set_capture_time`, for a body that recorded no clock -
  and for the phase 05 gate, which needs a wedding-shaped timeline over fixtures
  that carry make and model but no capture time, and says so in its output.
- `aura-cli infer` gained `--input wedding` and a stopwatch, so a 384 px model can
  be timed from the command line.

### Known limits

- The embedding carries no wedding semantics yet, so the purity, NMI and retrieval
  gates from section 6.4 are **deferred**, not passed. The duplicate gate is met and
  is not deferred: it is answered by the difference hash, which has no learned
  component. The evaluation harness computes all four and proves it would fail a
  head that learned nothing.
- Section 11's two GPU throughput budgets are waived - there is no GPU backend - and
  the 400 ms cold-build budget is waived for the build and met for the load. Both
  waivers carry expiry conditions in ADR-0011 section 5.

## Phase 04 - Cloud AI gateway and the agentic reasoning runtime

Paste one API key and the app gains a governed reasoning layer. It is a bonus
tier and never a dependency: with the network unplugged a full wedding still
completes, every decision marked `local_fallback`.

### Added

- `aura-cloud`: the frozen `CloudTask` contract and the seven-step gateway -
  policy, render, inspect, cache, govern, call, settle. It is the only crate in
  the product allowed to open a socket, and `scripts/check-banned.sh` enforces
  that the way it already enforces one runtime for models.
- Four providers behind one shape - Anthropic Messages, OpenAI Chat Completions,
  Google `generateContent`, and OpenAI-compatible self-hosted servers - with
  three-tier model aliasing, so a task names a capability and never a vendor.
- Three transports: a hand-written HTTP/1.1 client, a cassette replayer, and an
  offline refusal. The HTTP client does **not** speak TLS, so this build reaches
  `http://` endpoints - a local Ollama, LM Studio or studio gateway - and not the
  public HTTPS providers. The waiver and its expiry condition are in
  `docs/adr/ADR-0009-cloud-ai-policy.md`.
- Keys in the operating system's own credential store, by command invocation
  rather than FFI, with the secret written to the child's **stdin** and never to
  `argv`. A test asserts that for all three platforms' command shapes at once.
- A JSON Schema validator that refuses a keyword it does not implement rather
  than ignoring it, reports every failing rule at once in a stable order, and
  writes its complaint for a model to act on. Exactly one repair round trip, then
  the local answer.
- A payload builder that cannot upload an original: a full-resolution tiled
  decode and a scene-linear buffer are both refused by type, tiles are capped at
  768 px, and the EXIF summary is an allow-list with no GPS, no filename, no
  serial number and no absolute time. Optional pre-upload face blur.
- A cost governor that prices every call **before** it is made, drops a tier
  rather than a decision when the budget runs low, and stops at the cap without
  stopping the gallery.
- A response cache keyed on task, version, prompt hash, image content hashes and
  model, so re-running a wedding is nearly free and produces identical decisions.
- An audit trail with a row for every decision **including the ones that never
  reached a model**, which are usually the ones worth reading.
- Bounded agent primitives - step cap, deterministic tool ordering, structured
  scratchpad, four limits checked before each step, cancel within one step - for
  phases 27 and 29 to build on.
- `SegmentNaming`, the reference task, with section 7's prompt and schema copied
  verbatim and a controlled vocabulary of eighteen scenes, eighteen rituals and
  eight traditions.
- Migration 4: `cloud_calls`, `cloud_cache`, `cloud_budget`. The consent gate
  frozen in phase 01 has its first caller.
- Ten IPC commands and a Settings > AI keys panel: key entry, Check, caps, the
  privacy switches, a live spend meter and the audit viewer
  (`docs/adr/ADR-0010-cloud-ipc-surface.md`). No command returns a key.
- 14 error codes with runbooks: `AURA-CLOUD-6001..6014`.
- `aura-cli verify --phase 04`: sixteen checks, no network.

### Changed

- Budget assertions now run in release. A budget is a claim about the binary a
  photographer runs, and the payload builder is roughly ten times slower
  unoptimised.
- `aura-perf` gained count and cost budget kinds. Not everything worth budgeting
  is a duration or a size.

### Measured

Gateway overhead 0.08 ms per call (budget 15 ms). 75 calls and USD 1.04 for a
3,000 image wedding (budgets 75 and USD 1.50). 100 % cache hit rate on a re-run
(budget 70 %). A total cloud outage costs 9 ms against a 135 s pipeline floor
(budget 3 %).

### Rules every later phase inherits

- **`CloudAiGateway` is the only way to reach a model provider.** No phase may
  open a socket; the lint enforces it.
- **A task without a local fallback does not compile**, and neither does one
  whose answer cannot state its confidence and reasons.
- **Bump `CloudTask::VERSION` on any prompt, schema or ceiling change.** The
  cache key contains it, and a stale answer is worse than no answer.
- **Cloud proposes; deterministic code decides.** A cloud answer may not overrule
  a local decision at confidence 0.90 or above unless it cites contradicting
  visual evidence, and the conflict is logged.

## Phase 03 - Inference runtime and the signed model registry

One local AI runtime behind one frozen interface, and a model registry that
refuses anything it cannot verify. Nothing in phases 01 and 02 calls it yet;
every AI phase from 05 onwards calls nothing else.

### Added

- `aura-infer`: the frozen `InferService`, a hardware probe that measures a
  machine and writes `hardware_plan.json`, execution-provider negotiation with a
  per-machine set-aside list, a session pool, a batch scheduler with a memory
  ledger, cooperative cancellation, and warmup with visible progress.
- A deterministic interpreter over a documented subset of ONNX opset 13:
  nineteen operators, a protobuf reader *and* writer, and three genuinely
  different numeric paths (fp32, fp16, int8). Pure safe Rust. ONNX Runtime is
  **not** linked - see `docs/adr/ADR-0007-inference-runtime.md` for the four
  reasons and for how a backend is added later without touching a caller.
- `aura-models`: `models.lock` verified by ed25519 then sha256 then model card,
  in that order and entirely offline; resumable transfers against a transport
  port; verify-then-rename installs; a pending/active/rejected state machine that
  rolls a model back automatically when it fails its first real use; and the
  `AURADLT1` block delta with its encoder.
- `tools/model-sign`: offline signing. The release key never enters the
  repository or CI.
- Two placeholder models with model cards, and `cargo xtask models` as the CI
  gate that refuses a model without one (Article VI rule M1).
- `ml/export_onnx`: a second implementation of the file format in Python, which
  produces byte-identical files to the Rust generator - and, where onnxruntime
  happens to be installed, compares our interpreter against it (worst difference
  1.6e-7 on the placeholder models).
- Six IPC commands and a Settings > Hardware panel that lists unavailable
  providers *with their reasons* rather than hiding them
  (`docs/adr/ADR-0008-inference-ipc-surface.md`).
- 17 error codes with runbooks: `AURA-GPU-4001..4005`, `AURA-ML-5001..5012`.
- `aura-cli verify --phase 03`: model integrity, probe, warmup, throughput,
  parity, a forced memory squeeze, cancellation, a misbehaving provider and a
  real rollback, in one run.

### Changed

- `Priority` moved to `aura-core` so the runtime does not depend on preview
  infrastructure. Phase 02's copy is untouched, and a test keeps the two in step.
- `Clock` gained `monotonic_us`, because a 0.4 ms budget measured in whole
  milliseconds can only ever read 0 or 1.
- `scripts/check-banned.sh` refuses any use of ONNX Runtime outside `aura-infer`.

### Known gaps

- No GPU backend, so two of the phase's throughput budgets are unmeasurable and
  are waived with an expiry condition in ADR-0007.
- The models are placeholders; the first trained weights arrive in phase 05.
- No network transport: nothing in the workspace opens a socket yet.
- `InferEvent` is typed on both sides and not emitted, like `IngestEvent` and
  `PreviewEvent` before it, because the Tauri shell has never been launched here.

## Phase 02.1 - Proprietary mosaic codecs, X-Trans, and a parallel decode path

A follow-up to phase 02 that closes most of the camera-coverage gap ADR-0004
opened, and narrows the performance waiver it recorded. No frozen contract
changed, so `pipeline_ver` is unchanged and cached previews stay valid.

### Added

- `aura-raw::codecs`: independent safe-Rust implementations of three formats the
  first cut refused - Nikon's compressed NEF (Huffman coding plus the body's
  linearisation curve, read from MakerNote `0x0096`/`0x008C`), Sony's ARW2 block
  coding, and Olympus's adaptive predictive ORF. Each ships with an **encoder**,
  so every decoder is tested by round trip rather than by assertion.
- X-Trans support end to end: the 6x6 array is read from a DNG's `CFAPattern` or
  a RAF's block directory, binning uses a 3x3 block instead of a 2x2 quad, and
  interpolation widens to 5x5 because a 3x3 window on X-Trans can contain no red
  at all. Tiled tier 3 stays bit-identical to a whole-image decode.
- Fujifilm RAF block-directory parsing: sensor dimensions and colour layout, plus
  the uncompressed mosaic.
- `MosaicScheme`: which decoder a mosaic needs, decided once during the container
  walk. A file that declares no compression but stores too few bytes for its own
  bit depth is now recognised as compressed, which is how Olympus marks its
  scheme.
- 16 tests in `crates/aura-raw/tests/codecs.rs`, and the new encodings added to
  the tier-2 equivalence test and to the `verify --phase 02` cycle.

### Changed

- Demosaic, area-average resize, the colour rotation and the mosaic unpack are
  parallel over output rows. Each row writes into its own slice, so output is
  bit-identical whatever the thread count - invariant 4 rules out a parallel
  float reduction here. Small images stay serial.
- `docs/camera-support.md` and ADR-0004 rewritten around the new matrix.

### Known gaps

- Canon CRX (CR3) and Panasonic RW2 are still not decoded, and compressed RAF
  is not either. Reasons per format are in ADR-0004; all three fall back to the
  embedded preview with `AURA-RAW-2007`.
- A compressed NEF whose decode table we cannot read is refused rather than
  rendered through an invented curve.
- Sony's linearisation curve lives in an encrypted sub-directory. When it is not
  reachable the render uses a documented linear expansion.
- The ADR-0004 performance waiver is renewed, not closed: parallelism made tier 3
  2.1x faster and tier 2 1.4x faster at 25 MP, which is not enough to bring a
  45 MP frame inside budget. Measurements and the two remaining routes are in
  the ADR.

## Phase 02 - RAW decode engine and the three-tier preview pyramid

**Shipped:** instant, colour-correct previews for every RAW - the camera's
embedded JPEG for triage, a 2048 px proxy for AI, and on-demand full-resolution
decode for final render.

### Added

- `aura-raw`: container parsers (TIFF/EXIF, JPEG, ISO base media, Fujifilm RAF),
  format sniffing by magic bytes, CFA unpacking for 8/10/12/14/16-bit and
  lossless JPEG (SOF3), half-size and full demosaic, tiled full-resolution
  decode, EXIF orientation, and a per-file watchdog with memory ceilings.
- `aura-raw::colour`: linear Rec.2020 working space, Bradford adaptation, the
  neutral `filmic_lite` preview curve, the camera-profile resolution chain and a
  CIEDE2000 implementation checked against published worked examples.
- `aura-cache`: content-addressed preview cache keyed by BLAKE3 plus
  `pipeline_ver`, with LRU eviction, a hard budget, digest verification on read
  and an index that rebuilds itself by scanning.
- `aura-preview`: the frozen `PreviewService` trait, strict-priority scheduling
  with de-duplication and promotion, a worker pool that leaves one core free for
  the person, and the catalog-backed source.
- IPC: `get_preview`, `prefetch_previews`, `cancel_previews`, `preview_stats`,
  `set_cache_budget`, `purge_cache`, plus the `PreviewEvent` stream.
- UI: real pixels in the grid, an LRU thumbnail store with cancel-on-scroll, and
  a cache settings panel showing "previews use X GB of Y".
- `aura-cli`: `raw-fixtures`, `previews`, and `verify --phase 02`.
- Synthetic RAW fixtures: eight bench bodies, three mosaic encodings and a
  colour chart, so the decoder is tested without a single camera file.
- Docs: `docs/camera-support.md`, `docs/runbooks/previews.md`, ADR-0003
  (colour pipeline), ADR-0004 (decode backend), ADR-0005 (preview IPC).

### Changed

- `aura-catalog`: `preview` table now written and read (`upsert_preview`,
  `preview_row`, `count_previews`, `photos_without_preview`,
  `primary_file_for_photo`).
- `perf/budgets.toml`: phase 02 stage budgets, plus size budgets for the cache
  and for peak resident memory.
- Frozen contracts re-locked for the preview IPC additions (ADR-0005).

### Known gaps

- Proprietary mosaic compressions (compressed NEF and ARW, RW2, Canon CRX,
  X-Trans) are not decoded; those files render tier 2 from the embedded preview
  and are flagged `AURA-RAW-2007`. See `docs/camera-support.md`.
- The scalar CPU decoder misses the per-image budget at 45 MP; waived for this
  phase in ADR-0004 with measurements.
- No GPU path, no HEIF.

## Phase 01 - Foundation, catalog and wedding ingest

Workspace, error taxonomy with runbooks, SQLite catalog with the six-step
refusal chain, idempotent ingest with multi-camera clock alignment, the job
graph with leases, the typed IPC surface, the virtualised grid, fixtures, CI and
budgets. See `docs/progress/PHASE-01-EXIT.md`.
