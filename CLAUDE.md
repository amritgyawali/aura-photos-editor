# CLAUDE.md - operating manual for this repository

The full operating manual, the nine invariants and the phase ritual live in
[docs/plan/CLAUDE.md](docs/plan/CLAUDE.md). Read that first. This file records what
is specific to the checked-out repository.

## Reading order for an agent

1. `docs/plan/CLAUDE.md` - invariants, phase ritual, hard rules for code.
2. `docs/plan/12-ENGINEERING-CONSTITUTION.md` - the binding engineering rules.
3. `docs/adr/` - every recorded decision; the newest ADR wins over older prose.
4. The single phase file you are implementing, and nothing else.

Never load two phase files into one session.

## Where things are

| Concern | Location |
|---|---|
| Error registry | `crates/aura-core/errors.toml` (one runbook per code in `docs/runbooks/`) |
| Pinned model set | `models/models.lock` + `models/manifest.sig`, checked by `cargo xtask models` |
| Model cards | `docs/model-cards/` (template plus one card per shipped model) |
| Frozen contracts | `crates/*/src/contract/**`, `crates/aura-catalog/migrations/0001_init.sql`, `ui/src/ipc/types.ts` |
| Contract digests | `contracts.lock`, checked by `cargo xtask contracts --check` |
| Budgets | `perf/budgets.toml`, asserted by `cargo test -p aura-perf` |
| Phase progress | `docs/progress/PHASE-0N.md` and `PHASE-0N-EXIT.md` |
| Camera coverage | `docs/camera-support.md` (what decodes, what falls back) |
| Preview troubleshooting | `docs/runbooks/previews.md` |
| Hardware troubleshooting | `docs/runbooks/hardware.md` |
| Adding a model | `docs/runbooks/adding-a-model.md` |
| Cloud AI policy | `docs/adr/ADR-0009-cloud-ai-policy.md` |
| Using your own AI key | `docs/using-your-own-ai-key.md` |
| Recorded provider responses | `tests/cloud/cassettes/` |
| Embedding and index decisions | `docs/adr/ADR-0011-embeddings-and-similarity-index.md` |
| People and biometric decisions | `docs/adr/ADR-0013-people-intelligence-and-the-biometric-store.md` |
| Prominence weights (versioned, tunable) | `crates/aura-vision/config/prominence.toml` |
| Embedding evaluation gates | `tests/eval/embedding_eval.rs` + `ml/models/embed/eval_retrieval.py` |
| Identity evaluation gates | `tests/eval/identity_eval.rs` + `ml/models/face/eval_identity.py` |
| Scene and story decisions | `docs/adr/ADR-0015-wedding-scene-taxonomy-and-story-segmentation.md` |
| Scene thresholds (versioned, PM-owned) | `crates/aura-brain-wedding/config/scene_profiles.toml` |
| Ritual taxonomies (extensible) | `crates/aura-brain-wedding/config/rituals/` |
| Adding a wedding tradition | `docs/adding-a-tradition.md` |
| Scene evaluation gates | `tests/eval/scene_eval.rs` + `ml/models/scene/eval_scene.py` |
| Moment and duplicate decisions | `docs/adr/ADR-0017-burst-grouping-and-duplicate-policy.md` |
| Grouping thresholds (versioned, PM-owned) | `crates/aura-brain-wedding/config/moment_profiles.toml` |
| Burst grouping ground truth | `tests/fixtures/labels/bursts_*.json` |
| Burst evaluation gates | `tests/eval/burst_eval.rs` + `ml/eval/burst_eval.py` |
| Moments, bursts and duplicates explained | `docs/moments-bursts-and-duplicates.md` |
| Frame integrity decisions | `docs/adr/ADR-0019-frame-integrity-and-eye-intent.md` |
| Camera calibration (versioned, COL-owned) | `crates/aura-brain-photo/config/camera_calibration.toml` |
| Integrity evaluation gates | `tests/eval/integrity_eval.rs` + `ml/models/integrity/eval_integrity.py` |
| What the technical marks mean | `docs/frame-integrity.md` |
| Emotion and moment decisions | `docs/adr/ADR-0021-emotion-taxonomy-and-moment-ranking.md` |
| Emotion weights (versioned, PM-owned) | `crates/aura-brain-wedding/config/emotion_weights.toml` |
| Emotion evaluation gates | `tests/eval/emotion_eval.rs` + `ml/models/emotion/eval_emotion.py` |
| What the emotion marks mean | `docs/emotion-and-moments.md` |
| Composition and aesthetic decisions | `docs/adr/ADR-0023-composition-rules-and-aesthetics.md` |
| Composition rules (versioned, PM-owned) | `crates/aura-brain-photo/config/composition_rules.toml` |
| Composition evaluation gates | `tests/eval/composition_eval.rs` + `ml/models/composition/eval_composition.py` |
| What composition marks mean | `docs/composition-and-framing.md` |
| Culling, coverage and sizing decisions | `docs/adr/ADR-0025-culling-coverage-and-gallery-sizing.md` |
| Cull weights (versioned, PM-owned) | `crates/aura-cull/config/cull_weights.toml` |
| Coverage guarantees (versioned, PM-owned) | `crates/aura-cull/config/coverage_rules.toml` |
| Culling evaluation gates | `tests/eval/cull_eval.rs` + `ml/eval/cull_agreement.py` |
| How AURA culls, in the product's own words | `docs/how-aura-culls.md` |
| Ledger, calibration and autonomy decisions | `docs/adr/ADR-0027-decision-ledger-and-confidence.md` |
| Autonomy bands (versioned, PM-owned) | `crates/aura-explain/config/autonomy_bands.toml` |
| The public reason-code reference (generated) | `docs/reason-codes.md` |
| Explainability evaluation gates | `tests/eval/explain_eval.rs` + `ml/eval/calibration_report.py` |
| How confidence works, in the product's own words | `docs/how-confidence-works.md` |
| Render pipeline and determinism decisions | `docs/adr/ADR-0029-render-pipeline.md` |
| The published edit recipe schema | `docs/recipe-schema-v1.md` |
| Camera profiles (versioned, COL-owned) | `crates/aura-render/config/camera_profiles.toml` |
| Develop evaluation gates | `tests/eval/render_eval.rs` |
| How colour works, in the product's own words | `docs/colour-management.md` |
| Exposure and white-balance decisions | `docs/adr/ADR-0031-exposure-white-balance-and-skin.md` |
| Exposure targets (versioned, PM-owned) | `crates/aura-brain-photo/config/exposure_targets.toml` |
| Tone evaluation gates | `tests/eval/tone_eval.rs` + `ml/models/tone/eval_tone.py` |
| What the lighting marks mean, in the product's own words | `docs/mixed-lighting.md` |
| The skin fairness statement | `docs/skin-fairness.md` |
| Tone, curve and HSL decisions | `docs/adr/ADR-0033-tone-curves-hsl-and-skin-protection.md` |
| Tone intents (versioned, PM-owned) | `crates/aura-brain-photo/config/tone_intent.toml` |
| Colour evaluation gates | `tests/eval/colour_eval.rs` + `ml/models/colour/eval_colour.py` |
| What AURA changes about how a photograph looks | `docs/tone-and-colour.md` |
| Style learning and personal profiles | `docs/adr/ADR-0035-style-learning-and-personal-profiles.md` |
| Style evaluation gates | `tests/eval/style_eval.rs` + `ml/models/style/eval_style.py` |
| How Teach My AI works, in the product's own words | `docs/style-profiles.md` |
| Semantic masks, matting and quality gating | `docs/adr/ADR-0037-semantic-masks-matting-and-quality-gating.md` |
| Mask evaluation gates | `tests/eval/mask_eval.rs` + `ml/models/mask/eval_mask.py` |
| What a region means, in the product's own words | `docs/masks.md` |
| Local light decisions | `docs/adr/ADR-0039-local-light-sculpting.md` |
| Local light policy (versioned, PM-owned) | `crates/aura-brain-photo/config/local_light.toml` |
| Local light evaluation gates | `tests/eval/local_eval.rs` + `ml/models/local/eval_local.py` |
| What the local light adjustments do, in the product's own words | `docs/local-light.md` |
| Portrait retouch decisions | `docs/adr/ADR-0041-portrait-retouch-and-texture-protection.md` |
| Retouch presets and per-scene limits (versioned, PM-owned) | `crates/aura-retouch/config/retouch_presets.toml` |
| Retouch evaluation gates | `tests/eval/retouch_eval.rs` + `ml/models/retouch/eval_retouch.py` |
| What AURA does to skin, in the product's own words | `docs/retouch.md` |

## Non-negotiables enforced by the build

- `scripts/check-banned.sh` fails on `unwrap()`, `expect(`, `panic!`, `HashMap::new`,
  `SystemTime::now`, `Instant::now` and `any` in UI source, outside tests, benches,
  `xtask` and `main.rs`.
- Every crate root carries the lint block, including `#![forbid(unsafe_code)]`.
- `aura-core` depends on no other workspace crate; a test asserts it.
- Changing a frozen contract requires an ADR and a re-lock, in that order.
- **Every phase ends with a commit and a push**, on its own `feat/phase-NN-<slug>` branch,
  without being asked. Step 9 of the ritual in `docs/plan/CLAUDE.md`. The gate exits 0, the
  exit report is written, and then it is pushed - because until it is, the whole phase
  exists on exactly one disk.

## Building on this machine

The Windows SDK is absent, so the MSVC linker is not available. Use the GNU host
toolchain for everything:

```bash
RUSTUP_TOOLCHAIN=1.97.1-x86_64-pc-windows-gnu cargo test --workspace --all-targets
```

`cargo run --release --package xtask` is the one exception that does not link:
`windows-sys` needs `dlltool` for a release import library and MinGW is not
installed. Run xtask in debug (`cargo xtask ...`, which is what the alias does).

## Current state

Phase 01 is implemented: workspace, error taxonomy, catalog schema v1 with the
six-step refusal chain, idempotent ingest with clock alignment, the job graph with
leases, the typed IPC surface, the virtualised grid, the fixture generator, CI and
the runbooks.

Phase 02 is implemented: `aura-raw` (containers, the three decode tiers and the
colour pipeline, pure Rust with no LibRaw - see ADR-0004), `aura-cache`
(content-addressed, budgeted, self-healing), `aura-preview` (the frozen
`PreviewService`, strict-priority scheduling), the preview IPC surface (ADR-0005),
real pixels in the grid, and `aura-cli verify --phase 02` as the gate. Its exit
report is `docs/progress/PHASE-02-EXIT.md`, which lists three conditions - real
camera files, a photographed ColorChecker, and the CI matrix - before phase 03
starts. Nothing in `docs/plan/phases/PHASE-03-*.md` may be built until then.

A follow-up inside phase 02 (section 7b of the exit report) added the
manufacturer mosaic codecs in `crates/aura-raw/src/codecs/` - Nikon compressed
NEF, Sony ARW2, Olympus compressed ORF - plus X-Trans, and made the decode path
parallel over output rows. Canon CRX, Panasonic RW2 and compressed RAF remain
undecoded. **A new codec must ship with an encoder** in `fixtures.rs`: with no
camera files in the repository, a round trip is the only real proof, and
`tests/codecs.rs` is where it goes.

Phase 03 is implemented: `aura-infer` (the frozen `InferService`, a hardware
probe and plan, execution-provider negotiation with a per-machine set-aside list,
a session pool, a batch scheduler with a memory ledger, cancellation and warmup)
running on a deterministic pure-Rust interpreter over a documented ONNX opset 13
subset - ONNX Runtime is deliberately not linked, see ADR-0007; `aura-models`
(ed25519 then sha256 then model card, offline, in that order; resumable
transfers; verify-then-rename installs; automatic rollback; `AURADLT1` deltas);
`tools/model-sign`; two signed placeholder models with cards; the hardware IPC
surface (ADR-0008) and its Settings panel; and `aura-cli verify --phase 03` as
the gate. Its exit report is `docs/progress/PHASE-03-EXIT.md`.

Phase 04 is implemented: `aura-cloud` (the frozen `CloudTask` contract and the
seven-step gateway; four providers behind one shape; three transports - a
hand-written HTTP/1.1 client, a cassette replayer and an offline refusal; keys in
the OS credential store by command invocation with the secret only ever on stdin;
a JSON Schema subset validator with exactly one repair retry; a payload builder
that cannot upload an original; a cost governor that prices before it calls; a
response cache; an audit trail with a row for every decision including the ones
that never reached a model; bounded agent primitives; and `SegmentNaming` as the
reference task), migration 4, the cloud IPC surface (ADR-0010) and its Settings
panel, and `aura-cli verify --phase 04` as the gate. Its exit report is
`docs/progress/PHASE-04-EXIT.md`. TLS is waived (ADR-0009), so this build reaches
`http://` OpenAI-compatible endpoints and not the public HTTPS providers.

Phase 05 is implemented: `aura-index` (the frozen `SimilarityIndex` contract, a
deterministic HNSW graph at `M = 32` / `ef_construction = 200` / `ef_search = 64`,
filtered queries with the time window as a pre-filter over a sorted timeline, a
persisted snapshot with six named refusals, medoids, and the metrics the gates are
measured with), `aura-vision` (one decode, five results: the embedding, a 64-bit
difference hash, an 8x8x8 HSV histogram, six luminance statistics and an edge
summary), migration 5, `wedding_embedding` 1.0.0 signed into `models.lock` with a
card, the training and evaluation code in `ml/models/embed/`, the similarity IPC
surface (ADR-0012) and its debug panel, and `aura-cli verify --phase 05` as the
gate. Its exit report is `docs/progress/PHASE-05-EXIT.md`.

**The shipped embedding is a placeholder backbone and carries no wedding
semantics.** There is no labelled wedding data in this repository and no GPU
backend, so the ViT-B/16 with a contrastive head that phase 05 section 6.1
specifies cannot be trained or run here. Everything around it is real. This is
condition C10 in the phase 05 exit report, it is a Sev 2 trigger, and **no later
phase may claim a quality result that depends on the vector being
wedding-discriminative until it closes.**

Phase 06 is implemented: `aura-vision::face` (detection with a letterbox and three
strides, faces and bodies from one anchor, a conditional tiled pass, a geometric bokeh
gate, ArcFace alignment and pose, a quality gate that is a weighted geometric mean plus two
cut-offs, exact average-linkage clustering with relative-cohesion verification and
sub-centroids, role inference, prominence with a versioned weight file, and the synthetic
ground truth every gate is measured against), `aura-people` (the sealed biometric store,
the resumable project walk, the co-occurrence graph, timelines, the importance model and
the frozen `PeopleService`), migration 6, three signed models with cards, the `CoupleHint`
cloud task, the people IPC surface (ADR-0014) and its People panel, and
`aura-cli verify --phase 06` as the gate. Its exit report is
`docs/progress/PHASE-06-EXIT.md`.

**The three shipped face models are placeholders.** The detector finds no faces in a
photograph and the recogniser's templates carry no identity information; there is no
consented face data in this repository and no GPU backend. Every gate in section 10.1 is
measured against synthetic ground truth with a known answer, which proves the algorithms
and says nothing about the weights. This is condition C1 in the phase 06 exit report, it is
a Sev 2 trigger, and **no later phase may claim a quality result that depends on face
detection or recognition being accurate until it closes.** Condition C5 - no published
demographic analysis - is the second Sev 2 trigger.

Phase 07 is implemented: `aura-core::contract::scene` (the frozen 22-scene vocabulary, the
fourteen attributes, nine chapters, `SceneProfile` and `StoryService`),
`aura-brain-wedding` (a multi-head classifier that is an adapter on the *frozen* phase 05
embedding and opens no pixels, sixteen context features, an attribute decoder in which
four bits are measured rather than predicted, a tradition-conditioned ritual head with two
abstention mechanisms, the ritual taxonomy loader and the scene-profile registry; then HMM
smoothing over nine chapters, PELT change-point detection with a log-space penalty search,
a merge pass, medoid key frames and `Story` - the one implementation of `StoryService`),
migration 7, 22 scene profiles and 48 rites across five traditions in editable config, two
signed models with cards, the `SegmentNaming` cost policy, the story IPC surface
(ADR-0016) with its timeline, and `aura-cli verify --phase 07` as the gate. Its exit
report is `docs/progress/PHASE-07-EXIT.md`.

**The two shipped scene models are placeholders.** The classifier's posterior over a real
photograph describes a random projection of its embedding, and the ritual head names no
rite. Every gate in section 10.1 is measured against synthetic weddings whose chapter
boundaries are known by construction, which proves the algorithms and says nothing about
the weights. This is condition C1 in the phase 07 exit report, it is a Sev 2 trigger, and
**no later phase may claim a quality result that depends on scene classification being
accurate until it closes.** Condition C5 - no per-tradition accuracy published - is the
second Sev 2 trigger, and the disparity it guards is *cultural* rather than demographic.

Phase 07 half-closes phase 06's condition C3: `PeopleStore::scene_labels` feeds real
coarse labels into the co-occurrence graph, so `RoleOutcome::scene_starved` is false on a
classified wedding and `SCENELESS_CONFIDENCE_CEILING` stops capping the couple decision at
0.62. The other half needs twenty real weddings and a human.

Phase 08 is implemented: `aura-core::contract::moment` (the frozen `Moment`,
`DuplicateSet`, `DuplicateKind`, `CameraId`, `MomentOutline`, `MomentEdit` and
`MomentService`), `aura-brain-wedding::moments` (an adaptive per-camera cadence estimator,
a time-windowed similarity graph with scene-conditioned thresholds and a proximity
multiplier, deterministic union-find with an over-large re-cluster pass whose size cap is
a guarantee, the two-tier burst partition, the three-way duplicate conjunction,
cross-camera merging, the split/merge/lock/undo API and the store), migration 8, ten
argued-over grouping profiles in editable config, five burst regression patterns with
labelled ground truth in two formats, the moments IPC surface (ADR-0018) with its stacked
grid and duplicate review panel, and `aura-cli verify --phase 08` as the gate. Its exit
report is `docs/progress/PHASE-08-EXIT.md`.

**This phase ships no model** - the first since phase 02 - because grouping is arithmetic
over phase 05's vectors and phase 01's timestamps. Its section 10.1 gates are therefore
measurements of *algorithms* rather than of weights, and all of them pass: ARI 1.000 on
five patterns, duplicate recall and precision 1.000. **But the largest term in the
grouping score reads the placeholder embedding**, so no number in this phase is a claim
about a real wedding's pixels. That is condition C1 in the phase 08 exit report, it is a
Sev 2 trigger, and it closes with phase 05's C10 rather than separately.

Phase 08 found and fixed a defect that affects every real camera file: **EXIF's
`DateTimeOriginal` has whole-second resolution**, so `photo.timeline_time` alone cannot
distinguish the fourteen frames of a 10 fps burst. The fraction is in `photo.sub_sec`, and
`moments::moment::sub_sec_ms` reconstructs it. Every unit test passed and the phase gate
caught it.

Phase 09 is implemented: `aura-core::contract::integrity` (the frozen `IntegrityFlags`,
`MotionKind`, `ExposureVerdict`, `EyeOpenness`, `EyeState`, `CropRect`, `ReasonCode`,
`Reason`, `IntegrityResult`, `IntegrityOutline` and `IntegrityService`), `aura-brain-photo`
(a camera calibration table for twenty bodies, subject-aware sharpness from three classical
measures over eye/face/body/background regions, motion intent from a structure tensor and
EXIF, recovery-aware exposure with a specular-highlight exclusion, flat-region noise
normalised by ISO and body and expressed against the scene, an eye-state head with section
6.4's four intent rules, twenty-one reason codes each carrying an evidence crop, a
scene-weighted **geometric** composite, the store, the resumable pass and the synthetic
ground truth every gate is measured against), migration 9, two signed models with cards,
the integrity IPC surface (ADR-0020) with its Integrity card and filter chips, and
`aura-cli verify --phase 09` as the gate. Its exit report is
`docs/progress/PHASE-09-EXIT.md`.

**The two shipped heads are placeholders.** The focus head's three-way distribution over a
real crop describes a random projection of it, and the eye head says nothing about eyelids.
Every gate in section 10.1 is measured against synthetic frames whose answer is known by
construction, which proves the algorithms and says nothing about the weights. This is
condition C1 in the phase 09 exit report and it is a Sev 2 trigger. Two mitigations are
structural rather than promised: **the focus head can only withdraw a softness claim and
never make one**, and a missing eye head produces zero gating faces rather than a clean
verdict.

Phase 09 amended a frozen contract, which had not happened since phase 01: `FaceRef` gained
`bbox` and the two eye landmarks, because phase 09 cannot measure an eye region or show the
crop behind a closed-eye mark without them. ADR-0019 section 3 records the argument, and it
**removes the blocker phase 08's condition C4 named** - wiring phase 08's pass context is
now a small change rather than a contract change.

Phase 09's budget forced four schema decisions worth remembering: **reasons store their
code rather than their sentence** (a stored sentence is copy a release can change, and a
catalog full of English cannot be translated), two indexes that served no query were
removed after being measured, `face_eye_state` is `WITHOUT ROWID` with no `photo_id`, and
the eye rows read their geometry from `faces` rather than copying it. Together those took
the figure from 1,855 bytes per image to 927 against a 1 KB budget. It read "exactly
1,024" until phase 19, and the "exactly" was the tell: it was whole-file `PRAGMA
page_count`, which quantises to 4 KiB, pinned with no headroom in a number that can only
move in 4 KiB steps. **A budget measured with a quantised instrument must not be set at its
own measurement**, and the test now counts `dbstat` payload with the page overhead asserted
separately as a bounded ratio.

Phase 10 is implemented: `aura-core::contract::emotion` (the frozen `GazeTarget`,
`Interaction`, `FaceExpression`, `EmotionCode`, `EmotionReason`, `PeakKind`, `MomentPeak`,
`ReactionLink`, `ImageEmotion`, `Preference`, `EmotionOutline` and `EmotionService`),
`aura-brain-wedding::emotion` (eight continuous readings per face from the aligned crop
phases 06 and 09 already produce, gaze measured from phase 06's eye landmarks rather than
predicted, nine interactions from the whole frame with a person-prior *plane*, a smoothed
peak curve that refuses to name an apex when there is not one, reaction linking across
cameras inside a four-second window, and a nine-feature Bradley-Terry ranker whose
coefficients are a list a product manager can argue with), migration 10, 22 scene rows and
5 tradition rows in editable config, two signed models with cards, the `MomentSignificance`
cloud task, the emotion IPC surface (ADR-0022) with its Emotion card and moment browser,
and `aura-cli verify --phase 10` as the gate. Its exit report is
`docs/progress/PHASE-10-EXIT.md`.

**The two shipped heads are placeholders.** The expression head's eight sigmoids over a
real face describe a random projection of it, and the interaction head says nothing about
what two people are doing. Every gate in section 10.1 is measured against synthetic frames
whose answer is *painted into the pixels* and read back through the real warp, which proves
the algorithms and says nothing about the weights. That is condition C1 in the phase 10
exit report and it is a Sev 2 trigger. A second and different gap is condition C2: the
ranker is fitted on eight authored comparisons rather than the ten thousand photographer
comparisons section 9 asks for, **four of its nine coefficients are unidentifiable from
that data** and are set by argument, and section 13's blind agreement study does not exist.

Two mitigations are structural rather than promised: **a missing head is visible rather
than silent** - `EmotionCode::NoFaces`, a confidence capped at 0.55 and
`EmotionOutline::face_aware` at zero - and **nothing in this phase can cull**, so an
untrained head produces a wrong ordering rather than a wrong deletion.

Phase 10 **closes phase 09's condition C4**. `IntegrityPass::with_emotion` fills
`IntentInput::tears` through `aura-core`'s frozen `EmotionService`, so a tearful
closed-eye photograph now carries `EYES_CLOSED_OK`. The dependency runs through the trait
and not through a crate: `aura-brain-photo` and `aura-brain-wedding` depend on each other
in **neither** direction, which is what keeps "no phase may keep its own blink detector"
and "no phase may keep its own expression model" from becoming a cycle. Phase 09's
`ANALYSIS_VER` went 1 → 2 so every stored verdict is re-measured.

Phase 10 also moved the 112 px two-point face warp into `aura-vision` when it became its
second consumer. Two copies of a warp is two crops that drift apart while looking
identical; phase 09's 26 eval gates and 11 calibration tests pass unchanged after the move.

Phase 11 is implemented conditionally: `aura-core::contract::composition` freezes the
flags, crop evidence, reason vocabulary, result, coverage and service; `aura-brain-photo`
measures horizon geometry, joint and limb cuts, headroom, thirds, balance, background edge
energy, bright regions, head merges and colour competition, then combines them with a
bounded scene-conditioned aesthetic reading. Migration 11 stores one judgement per photo,
22 measured scene rows live in editable config, two model artifacts are signed with cards,
the Composition card renders reasons and normalised evidence overlays, and
`aura-cli verify --phase 11` is the executable gate. Its exit report is
`docs/progress/PHASE-11-EXIT.md`.

**Both phase 11 heads are placeholders.** `pose_keypoints` has an architecture fixture
whose global pooling cannot recover real spatial keypoints, and `aesthetic_head` is an
untrained deterministic projection. The analyser therefore must not describe their output
as learned; it exposes the reference aesthetic and an unavailable-head caveat until trained
provenance exists. The 37 composition evaluation tests use authored synthetic pixels and
reference geometry. They prove the deterministic arithmetic and regression guards, not
real-wedding accuracy, photographer agreement, demographic fairness, or model quality.
That is condition C1 and a Sev 2 trigger.

Phase 12 is implemented conditionally: `aura-core::contract::cull` freezes three autonomy
modes, twelve must-haves, three coverage states, twenty-four typed reason codes, the keep,
rejection, coverage-report, outline and service shapes; `aura-cull` fuses four sub-scores as
a weighted **geometric** mean so no signal can rescue another, applies section 6.1's three
hard vetoes, picks per-moment winners with a keeper count driven by how much the moment
varied, allocates chapter quotas with a bounded local search, runs the coverage guard twice,
enforces three sliding-window diversity caps, and reconciles the gallery to a predicted or
requested size. Migration 12 stores the run, the keepers, the rejections, the coverage
report and the photographer's own overrides, 22 scene weight rows and twelve guarantees live
in editable config, the cull view renders coverage, size, gallery and reasons, and
`aura-cli verify --phase 12` is the executable gate. Its exit report is
`docs/progress/PHASE-12-EXIT.md`.

**Every sub-score underneath every decision comes from a placeholder head.** Phase 06's
detector finds no faces, phase 09's focus head is a random projection, phase 10's expression
head says nothing about faces, and phase 11's aesthetic head is untrained. The arithmetic in
this phase is real, measured and tested against four synthetic weddings whose right answer
is known by construction; the numbers it works on are not yet claims about photographs. That
is condition C1, it is a Sev 2 trigger, and it closes with phase 05's condition C10 rather
than separately. Two further gaps: the per-scene isotonic calibration ships as the identity
map at `calibration_ver = 0` (C2), and the gallery-size regression is authored rather than
fitted on sixty real delivered galleries (C3).

**The cloud tie-breaker of section 7 was deliberately not built** (C6). Its trigger is two
candidates within 0.02 `keep_score` of each other, and with four placeholder heads
underneath, a 0.02 difference is noise rather than a tie - so every call would spend a
photographer's money asking a vision model to arbitrate between two random projections and
would then record the answer as though it meant something. Section 7's own offline fallback
is what ships. Nothing is stubbed for it; adding `CullTieBreak` later touches no frozen
shape in this phase.

Phase 13 is implemented conditionally: `aura-core::contract::ledger` freezes the reason, the
decision, six decision kinds, four subjects, four evidence variants, four autonomy bands,
three sources, the outline and the service, and `ids.rs` gains `DecisionId`; `aura-explain`
holds the append-only ledger, the decision builder with its canonical encoding and inputs
hash, isotonic and temperature calibration with ECE and Brier, the autonomy policy, the
reason registry, the grounded summariser, the replay port and the anonymised support bundle.
Migration 13 stores three tables, one trigger and one view, section 6.4's bands live in
editable config with a written reason per row, `ExplainSummary` is the one cloud call, the
Explain panel renders six tabs with evidence crops and the alternative comparison, and
`aura-cli verify --phase 13` is the executable gate. Its exit report is
`docs/progress/PHASE-13-EXIT.md`.

**Nothing in this build is calibrated.** Every model is the identity map at
`calibration_ver = 0`; the ECE estimator, the isotonic fitter and the CI gate are all real and
are all measured against synthetic predictors whose error is authored. While that is true,
`uncalibrated_raises` moves every decision one band toward review - so **nothing in this build
acts unattended**, and phase 28 cannot ship until a calibration does. That is condition C2 and
a Sev 2 trigger. Condition C1 is the other: every decision this ledger records was made from
placeholder heads, and it closes with phase 05's C10 rather than separately.

Phase 14 is implemented conditionally: `aura-recipe` freezes edit recipe schema v1 and owns
the canonical form, the hash, the migration framework, the XMP and AURA sidecars, the undo
history and migration 14's four tables; `aura-render` freezes the render API and executes it -
highlight recovery before white balance, one linear Rec.2020 working space, 23 stages in one
ordered array, a tiler whose output is bit-identical to a whole-frame render, and an output
transform that is the only place tone is baked. Migration 14 stores the recipe, its history,
its snapshots and what was exported; eight synthetic camera profiles live in editable config;
the Develop panel renders the protected dot and the caveats; and `aura-cli verify --phase 14`
is the executable gate. Its exit report is `docs/progress/PHASE-14-EXIT.md`.

**This build links no `wgpu` backend** (ADR-0029 section 4), so four of section 11's five
performance rows are waived and the interactive budget of 60 ms is not met: the processor
reference path renders a 2048 px proxy in about 210 ms in release. That is condition C1 and a
Sev 2 trigger. Condition C2 is the second and it is about colour rather than speed: there are
no camera files and no photographed ColorChecker in this repository, so the golden suite runs
over authored synthetic pixels and eight *synthetic* bench profiles. It is a determinism and
regression gate, not a claim about colour accuracy, and **no later phase may claim a colour
result that depends on a camera profile being measured until it closes.** Every real camera
body renders through the neutral reference profile and says so (`AURA-RENDER-8008`).

Five rules that phase 14 adds and every later phase inherits:

- **`RenderService` is the only way to turn a recipe into pixels.** Sixth service of its kind
  and the first that produces an *output* rather than a judgement. Phases 15 to 17 decide
  parameters, 18 fills masks, 20 to 22 add operators, 23 adds geometry and 30 exports - none
  of them keeps its own renderer. Two answers to "what does this photograph look like" is a
  gallery that does not match the album that does not match the proof.
- **A parameter a person set is never overwritten, and the merge is where that is true.**
  `aura_recipe::schema::merge` is the only function in the workspace that writes one recipe
  into another. There is no argument that disables the protection, no IPC field that routes
  around it, and `AURA-RENDER-8006` makes every refusal visible rather than silent. The one
  thing that shortens `user_edited_fields` is a photographer choosing "reset to AI
  suggestion".
- **The output transform is the only place tone is baked.** Everything before it lives in
  linear Rec.2020 and is reversible; `crates/aura-render/tests/colour_discipline.rs` is a grep
  as a test, so the second module to start encoding fails the build. Invariant 8, enforced by
  a tool rather than by review.
- **A render says what it skipped.** `SkipReason` is a closed set and each variant names the
  phase that fills the gap. An absent mask generator is a sentence in the panel, not a mask
  that silently did nothing - which is the failure mode a local exposure lift has when nobody
  is told it did not apply.
- **A delivered file can be re-created from four values**: the RAW's content hash, the
  canonical recipe, the engine string and the output spec. All four are stored with every
  export in `export_renders`, and `AURA-RENDER-8007` says which of the four moved.

Two decisions in this phase are worth remembering because they will be re-argued later.
**The tiling halo is a sum rather than a maximum** - stages compose, so a pixel committed
after sharpening needs correct values twelve pixels away *after clarity*, which needs correct
values forty-eight pixels away in the input; a maximum is right for one spatial stage and
wrong for two, and the failure is a faint seam that only shows on large exports. And
**frame-wide statistics are measured once and passed in**: sharpening's normaliser, noise
reduction's edge keeper and dehaze's floor are properties of the photograph rather than of a
tile, and `spatial::Stats` exists for that reason alone.

Phase 15 is implemented conditionally: `aura-core::contract::tone` freezes the estimate, the
illuminant, the skin locus, the alternatives, the reasons, the reference frame, the outline, the
override and `ToneService`; `aura-brain-photo::tone` measures the pixels once, finds the known
neutrals, accumulates **each person's own** skin locus across the wedding, generates four
illuminant hypotheses, scores every one of them against that locus and against the neutrals,
walks a twenty-step correction from "leave the light alone" to "remove it completely" and stops
at the first point everybody in frame is plausible again, then moves the exposure onto the
scene's face-luminance band and clamps it against clipping and phase 09's shadow budget.
Migration 15 stores the estimate, the loci and each chapter's anchors; 22 argued-over scene rows
live in editable config; two models are signed with cards; seven IPC commands (ADR-0032) feed a
Basic panel and a per-scene review queue; and `aura-cli verify --phase 15` is the executable
gate. Its exit report is `docs/progress/PHASE-15-EXIT.md`.

**Both shipped heads are placeholders and neither is consulted.** `WB_HEAD_TRAINED` and
`EXPOSURE_HEAD_TRAINED` are false, so the learned illuminant hypothesis is never generated and
the faceless exposure path records `ExposureUnavailable` rather than inventing a number. Every
gate in section 10.1 is measured against synthetic frames whose illuminant, subject luminance and
skin reflectance were **painted into the pixels** and read back through the real pipeline, which
proves the arithmetic and says nothing about a photograph. That is condition C1 and a Sev 2
trigger. Condition C2 is the second and it is the one to be careful about: the fairness gate is
measured on five *reflectances*, not five people, so it proves the mechanism is per-identity and
proves nothing about a real person. `docs/skin-fairness.md` says so in the product's own words.

Two rules that phase 15 adds and every later phase inherits:

- **`ToneService` is the only way to ask what colour the light was.** Eleventh service of its
  kind. Phase 16 grades on top of these values, 17 shifts them, 18 corrects locally against them,
  25 normalises a gallery toward them, 26 matches two cameras with them and 27 checks them. Two
  answers to "what temperature was this room" is an album that does not match the gallery.
- **A skin target is measured, never assumed - and the schema cannot express an alternative.**
  There is no ideal-skin constant in `aura-core`, in migration 15, in `exposure_targets.toml` or
  anywhere in the code path. A fixed target is how an editor lightens dark skin while believing it
  is correcting a cast; the defence is that nothing here has a constant it could compare a person
  against, and the phase gate scans the schema for one on every run. The Monk-scale buckets the
  evaluation needs live in `tests/eval` and never reach the catalog.

Four decisions in phase 15 are worth remembering because they will be re-argued. **The
white-balance confidence is built on agreement between the top two answers, not on the cost gap
between them** - it was built on the gap first, and that scored two independent estimators
landing on the same chromaticity as "undecided", which put every frame below the skin-sample
threshold and left the hard constraint binding on nothing, silently, while every unit test
passed. **The correction is a linear scan rather than a bisection, and it walks in `u'v'`** -
the set is not an interval when two people's loci differ, so a bisection returns an arbitrary
member of it; and the *space* was got wrong first, interpolating a colour temperature, which
walks along the Planckian locus and so could never reach the off-locus light the branch existed
to preserve. And **a row with `user_edited = 1` still
carries AURA's own numbers**, which is what lets the review queue show a disagreement and phase
30's learning loop read one - and it only works because `ToneStore::override_of` exists beside the
frozen service to read the other side.

The fourth is the one to generalise: **ask the room, not the winner.** Anything that describes
the *scene* - what kind of light this is, whether it was a choice - must not be derived from
whichever hypothesis won a cost race, because the winner changes as a project accumulates
evidence and the room does not. Phase 15 shipped with that backwards and the symptom was a
label that was right on a project's first frame and absent on its four-hundredth. Phases 16, 17,
25 and 26 all describe scenes on top of these values and inherit the trap.

Phase 16 is implemented conditionally: `aura-core::contract::colour` freezes the decision, the
monotone curve, the eight bands, the content readings, the skin guard report, the three
variants, twenty-nine reasons, the outline, the override and `ColourService`;
`aura-brain-photo::colour` solves the five tone parameters from the histogram and phase 09's
noise headroom, fits a curve to the scene's subject-contrast intent under three constraints,
reads what is in the frame from its colours, forms a harmony objective, expresses it in the
recipe's eight bands, bounds the whole grade with a clipping guard and a subtlety cap, and
then **measures what it did to the people in the frame and re-solves until they have not
moved**. Migration 16 stores one decision per photograph; 22 argued-over scene rows live in
editable config; one model is signed with a card; seven IPC commands (ADR-0034) feed a Tone
panel, a curve editor and an HSL panel; and `aura-cli verify --phase 16` is the executable
gate. Its exit report is `docs/progress/PHASE-16-EXIT.md`.

**The shipped tone head is a placeholder and is never consulted.** `TONE_HEAD_TRAINED` is
false, so `Analyser::tone_hint` returns `None` and no frame in this build is graded by a random
projection. What ships is a deterministic solver, and that is a decision rather than a
fallback: a random projection blended at any weight is a random contribution at that weight,
and it would be indistinguishable in the panel from a learned one. Every gate in section 10.1
is measured against synthetic frames whose foliage hue, dress luminance and subject contrast
were **painted into the pixels** and read back through the real pipeline, which proves the
arithmetic and says nothing about a photograph. That is condition C1 and a Sev 2 trigger.
Condition C2 is the second and it is the one to be careful about: the fairness gate is measured
on five *reflectances*, not five people. `docs/skin-fairness.md` says so in the product's own
words, and `docs/tone-and-colour.md` is the rest of the phase in the same voice.

Two rules that phase 16 adds and every later phase inherits:

- **`ColourService` is the only way to ask how a photograph should be graded.** Twelfth service
  of its kind. Phase 17 shifts these values toward one photographer's style, 18 grades locally
  on top of them, 25 normalises a gallery of them, 27 checks them and 29 builds albums from
  them. Two answers to "how much contrast does this frame want" is an album that does not match
  the gallery.
- **A guarantee is measured, not asserted.** Section 6.3 promises skin never shifts measurably;
  `colour::skin_guard` grades this frame's own skin pixels **through the real renderer**,
  measures the hue and chroma they actually moved, and re-solves - or withdraws the colour
  operations entirely - until they are inside the ceilings. The measurement is stored on the
  row, so "skin never shifts" is `SELECT MAX(skin_hue_shift)` rather than a sentence in a
  document. A product that could only assert it would have no way to find out it had stopped
  being true.

Four decisions in phase 16 are worth remembering because they will be re-argued. **The skin
guarantee is a post-condition with a re-solve, not an attenuation factor** - every product that
has shipped orange skin applied the factor to a parameter while making the promise about a
pixel, and between them sit a curve that moves chroma without touching a band and a vibrance
operator that is not linear in what it touches. **Its baseline is the frame's own skin after
the tone half**, because chroma scales with lightness and measuring against the raw pixel makes
every correctly brightened frame a violation. **Monotonicity is a property of the control
points rather than of an evaluator** - the renderer's Fritsch-Carlson interpolation is monotone
exactly when its points are, so `ToneCurve::new` refusing a bad set is the whole guarantee, and
the interpolation moved into `aura_raw::colour::curve` when the fitter became its second
consumer. And **the curve fitter bounds its gain rather than clamping its nodes**: clamping a
node that wanted to sit above white produced a flat top, which is a posterised band and new
clipping in one move.

Phase 16 also makes `aura-brain-photo` depend on `aura-render`, which is new and is the
consequence of the guarantee being a post-condition: the guard measures what the *renderer*
does to a skin pixel rather than what a copy of it would do. `aura-recipe` arrives
transitively, and `crates/aura-brain-photo/tests/no_recipe_writes.rs` is a grep as a test that
fails the build if this crate ever calls `schema::merge` - writing a recipe is phase 14's rule
and stays in `aura-app`.

Phase 16 corrected a phase 15 oversight: **migrations 15 and 16 are now in `contracts.lock`**.
`docs/plan/CLAUDE.md` has listed every migration as a frozen contract since phase 01, and 15
had been omitted from `EXTRA_CONTRACTS` when it shipped.

Phase 17 is implemented conditionally: `aura-core::contract::style` freezes the profile, the
delta, the curve shift, the skin lean, the eight scene groups, the ten kinds of light, the
eighty-leaf bucket, the bucket model, the diagnostics, the four fallback levels, the four
matching methods, the two extraction sources, the pair, the query, the advice, the outline,
twenty reasons and `StyleService`, and `ids.rs` gains `ProfileId`; `aura-style` learns.
`pairs.rs` matches an original to a final by content hash, by filename stem, by capture time
and perceptually, and **refuses an ambiguous match** rather than guessing. `extract.rs` reads
an XMP exactly when there is one and otherwise hands the pair to `fit.rs`, which reproduces the
delivered photograph by coordinate descent over twelve parameters **through phase 14's real
renderer** and rejects what it cannot explain, so the model learns a global look rather than
somebody's unmodelled dodging. `bucket.rs` sorts each pair into one of eighty leaves.
`tree.rs` fits ridge regressions with Huber reweighting, shrinks each level toward its parent
by `n / (n + k)`, and caps what any one wedding may contribute. `diagnostics.rs` measures the
result on held-out pairs *against the baseline as well as against the ceiling* and writes one
sentence about what to shoot next. `infer.rs` resolves a leaf through bucket, group, global and
factory and **always answers**. `profile.rs` versions, signs and refuses; `store.rs` owns
migration 17 and `api.rs` is the frozen service and the resumable walk. Migration 17 stores
four tables and a coverage view; two small modules in `aura-brain-photo` apply the shift; eleven
IPC commands (ADR-0036) feed a Teach My AI wizard, a profile report, a bucket matrix and an A/B
comparison; and `aura-cli verify --phase 17` is the executable gate. Its exit report is
`docs/progress/PHASE-17-EXIT.md`.

**This phase ships no model** - the second since phase 08, and for a different reason. There is
nothing to train: the fit has a closed form, and what is missing is not weights but *weddings*.
Section 9's DATA task asks for consented archives from five photographers across traditions and
there are none in this repository, so every number in this phase is measured on synthetic
archives whose look was chosen, applied to authored plates through the real renderer and
recovered. That proves the matcher, the fitter, the bucketing, the regression, the shrinkage,
the archive cap, the bundle and the store; it is not evidence that a photographer would
recognise their own work. That is condition C1 in the phase 17 exit report and it is a Sev 2
trigger. The other Sev 2 is C4: the baseline a training run is a residual *from* is supplied by
the caller, and the desktop shell has only a neutral one, so until an archive can be imported as
a project and run through phases 15 and 16 first, a learned delta is an absolute edit wearing a
residual's shape.

Three rules that phase 17 adds and every later phase inherits:

- **`StyleService` is the only way to ask what a photographer's own look is.** Thirteenth
  service of its kind. Phase 25 normalises a gallery toward these values, 26 matches a second
  shooter to them, 27 checks them, 28 acts on them unattended and 30's learning loop updates
  them. No phase may keep its own style profile or its own bucket vocabulary.
- **A style is a residual, and the baseline is never re-derived.** An empty profile produces
  exactly what phases 15 and 16 decided, so there is no state of this system in which switching
  the feature on makes a photograph worse than switching it off - and `gate_4b` asserts it. Any
  later phase that finds itself computing an absolute from a profile has misunderstood the shape.
- **The shift happens before the guards, and every guard re-runs after it.** Phase 16 wrote this
  rule before this phase existed and this phase implemented it: the style moves the *solved*
  parameters, and then phase 15's clipping bound, phase 15's skin-locus constraint, phase 16's
  clipping guard and phase 16's skin guard all decide how much of the move survives. A personal
  style that would move somebody's skin is a personal style the guard withdraws.

Four decisions in phase 17 are worth remembering because they will be re-argued. **The archive
cap is `w = cap * rest / (1 - cap)` and not `cap / share`** - scaling one archive's weight by its
share of the total leaves it *above* the cap, because shrinking the weight also shrinks the total
it is a share of, and the first implementation measured 48 % influence against a documented 35 %
while reading as correct. **The regression's slopes are fitted and then discarded**: a slope
fitted on eleven samples spanning ISO 1600 to 4000 is not identified at ISO 400, which is exactly
the frame it would be applied to, so the slopes keep confounds out of the intercept and the
intercept is what ships - which is also what makes inference three map lookups and an addition.
**A rejected pair is written rather than dropped**, the only place in the product where a failure
writes a row, because here the failure *is* the evidence a photographer needs. And **the tone
half styles the solved answer rather than the target band**: shifting the band is cleaner on
paper and makes a style profile change what "correctly exposed" means, which is phase 15's
decision and not this phase's.

Phase 18 is implemented conditionally: `aura-vision::contract::mask` freezes the twenty-class
vocabulary, the two payload forms, the region, the edge word, twelve reasons, the algebra, the
resolved plane, the outline and `MaskService`, and `ids.rs` gains `MaskId`; `aura-vision::mask`
measures. `segment.rs` finds twenty classes from geometry and colour seeded by phase 06's boxes
and landmarks, `subject.rs` composes the person classes and bounds them by the person boxes,
`trimap.rs` erodes and dilates into a band that is a fraction of the region's own size,
`matting.rs` solves alpha inside that band with a guided filter in closed form and reports how
much of the boundary the photograph could actually determine, `instance.rs` assigns components to
identities by containment and leaves the ambiguous ones unassigned, `algebra.rs` is the seven
operations the brush and phases 19 to 24 both go through, `quality.rs` turns the two quality
numbers into a strength ceiling for five named operations, `store.rs` is the codec and migration
18, and `api.rs` is the frozen service and the resumable lazy pass. Migration 18 stores one row
per region and one per gated operation; two shaders are held to the reference by
`shader_parity.rs`; two models are signed with cards; eight IPC commands (ADR-0038) feed a mask
panel; and `aura-cli verify --phase 18` is the executable gate. Its exit report is
`docs/progress/PHASE-18-EXIT.md`.

**Both shipped heads are placeholders and neither is consulted.** `SEG_HEAD_TRAINED` and
`MATTING_HEAD_TRAINED` are false, so no photograph in this build is segmented by a random
projection. What ships is measurement, and the matting half of it is not a compromise: a guided
filter in closed form is a real matting algorithm whose failure mode is a slightly soft edge
rather than a confidently wrong one, and the interpreter has no `Resize` and no `ConvTranspose`
to run a network with anyway. Every gate in section 10.1 is measured against synthetic frames
whose regions were **painted into the pixels** and read back through the real pipeline, which
proves the arithmetic and says nothing about a wedding photograph. That is condition C1 and a Sev
2 trigger. Condition C2 is the second and it is the one to be careful about: **the 100 % zoom
artefact audit did not happen**, the halo metric exists and has never been run on data, and a
mask can pass every mIoU gate in the exit report and still show a rim a photographer sees
immediately.

Six rules that phase 18 adds and every later phase inherits:

- **`MaskService` is the only way to ask what is where in a photograph.** Fourteenth service of
  its kind, and the rule matters more here than in any of the thirteen: phases 19 to 24 each edit
  a region, and two answers to "where is her face" is a gallery where the light sculpting and the
  retouching disagree about the same pixels. That reads as neither, and it is unfixable
  afterwards because nothing records which answer each stage used.
- **A region says how much may be done with it, and a later phase multiplies.** `Mask::allowance`
  is the **geometric** mean of two independent uncertainties and `quality::allowance` is the one
  place a gating decision is made. This is the first time a phase has constrained what a *later*
  phase may do, and it exists because a wrong mask is **silent**: a wrong exposure looks wrong,
  and a face mask that includes the wall behind somebody's ear looks fine until phase 20
  brightens it.
- **Two quality numbers, never one.** Confidence is how sure the class is and edge quality is how
  well determined the boundary is; they fail independently and are fixed by different things, and
  a photographer can re-brush a boundary and cannot re-brush a class. Collapsing them loses which
  of the two somebody is looking at.
- **The coverage denominator is *selected* frames, and both numbers are on the wire.** Every
  outline since phase 09 has counted against every photograph and this one does not: a mask over
  a rejected frame is not a gap, it is a frame nobody asked about. Phase 08's rule - say what the
  denominator is - is the one being followed.
- **A photographer's region is never regenerated, and it takes two statements to guarantee it.**
  `user_edited = 0` inside the `DELETE`, *and* the edited coordinates skipped on re-insert. The
  first alone is what phases 06, 08 and 09 needed; here it was not enough, because
  `INSERT OR REPLACE` deletes the row it conflicts with and `masks` has a unique key on
  `(image_id, kind, identity_id)`.
- **Nothing in this phase moves a pixel.** Sixth phase running. `MaskService` produces regions,
  phases 19 to 24 consume them, and there is no `apply_mask` anywhere on the IPC surface.
  `SkipReason::MaskGeneratorAbsent` therefore stays reachable - wiring the resolved planes into
  the render graph is phase 19's first task and changes no shape frozen here.

Three decisions in phase 18 are worth remembering because they will be re-argued. **The frozen
contract is in `aura-vision` rather than in `aura-core`**, because section 5 freezes
`upload_gpu(&self, mask: &Mask, level: RenderLevel)` and `aura-core` depends on no workspace
crate - the rule was never "contracts live in `aura-core`" but "a contract lives in the crate
that owns the kind of thing it describes", which is why `SimilarityIndex` and `RenderService` are
where they are. **Instance scoping is containment rather than intersection-over-union**: a face
is a small ellipse inside a large body box, so an IoU floor leaves every face in the wedding
unassigned while looking like a careful threshold. And **the trimap band is narrow rather than
generous** - it started at twelve per cent of the region's size and was halved after measuring
what a wide band does when the boundary is not soft, because a band is a licence for the matte to
decide and a wide one hands it forty pixels of wall to be wrong about.

Phase 18 also **makes `aura-vision` depend on `aura-catalog`**, which retires a sentence phase 06
wrote twice: "this crate has no catalog dependency, so it *cannot* write a face template". Section
4 of the phase document puts the mask store here, so the claim became a rule.
`crates/aura-vision/tests/no_template_writes.rs` is what replaces it and the phase gate runs the
same grep - the third grep-as-a-test in the repository, after `colour_discipline.rs` and
`no_recipe_writes.rs`. It is a weaker guarantee than the one it replaces and the exit report says
so.

Phase 18 found and fixed two defects of the kind that ship silently. **The resampler manufactured
a halo**: `Plane::resize_bilinear` read zero outside the plane, darkening the outermost half-pixel
of every upsampled region, which is a one-pixel dark rim around every mask at every render level
produced by the code that *delivers* a boundary rather than the code that finds it. And **an
`INSERT OR REPLACE` would have destroyed a hand-edited mask through a constraint** rather than
through a statement anybody was reading. The phase gate caught the second on its first run.

Phase 17 corrected two earlier oversights: **`contracts.lock` carried a stale digest for
`crates/aura-core/src/contract/colour.rs`**, so `cargo xtask contracts --check` would have failed
on `main` - phase 16 re-locked before a final edit to the contract - and **the justfile had no
`phase-16-verify` recipe**, so the only way to run that gate was to remember the argument. It
also filled in the CHANGELOG entries phases 14 and 16 never wrote.

Merging phase 15's coloured-light fix into this branch produced the first version collision in
the repository: **both sides had bumped `tone::ANALYSIS_VER` from 1 to 2 for unrelated
reasons** - `illuminant::ambient` plus the `u'v'` correction on one side, the style lean on the
other - so a build carrying both is a third measurement and the constant is **3**. The general
rule is that a version column counts *measurements*, not commits: two branches that each
invalidate the same column do not merge into one invalidation, and taking either side's number
would leave estimates written by the other parent looking current while being stale, which is
the exact comparison `AURA-ML-5060` exists to prevent.

Phase 19 is implemented conditionally, and it was **written out of order**: it was built on top
of phase 15, under the phase ritual's contract-first handoff, while 16, 17 and 18 did not yet
exist - and merged into them afterwards. `aura-core::contract::local`
freezes `LocalLightPlan`, the `MaskField` input port phase 18 fills, six operations and their
priority order, ten named face zones, thirty reason codes, the outline, the override and
`LocalService`; `aura-brain-photo::local` measures the frame once, splits a lift so shadows
move and highlights barely do, solves every face in a frame together, pairs a subject
enhancement with a matching background reduction, separates three frequency bands and returns
two, places ten retoucher's moves and derives the dodge-and-burn map from them, finds specular
sheen and reduces luminance only, and spends every one of those against one per-image
perceptual allowance. Migration 19 stores the plan, the lit faces and the gates; 22
argued-over policy rows live in editable config; three shaders ship with the processor
reference they are held to; six IPC commands (ADR-0040) feed a Local panel; and
`aura-cli verify --phase 19` is the executable gate. Its exit report is
`docs/progress/PHASE-19-EXIT.md`.

**Phase 18's masks are not wired into this pass, so on this build every operation is still
gated and nothing is edited.** `MaskField` is the only route to a mask, `AppState::local_pass`
never calls `LocalPass::with_masks`, and `aura-brain-photo::local` contains no generator, no
segmentation model and no geometric fallback - because a rectangle's edge does not follow a
person, and an edit through one leaves the bright rim this phase exists to avoid. That gap is
a *connection* rather than a missing dependency now: phase 18 ships `MaskService`, and turning
its resolved planes into `MaskField`s is the one piece of phase 19 that this merge does not
carry. Every quality gate was measured against fixtures whose masks are perfect by
construction. That is condition C1, it is a Sev 2 trigger, and **no later phase may claim a
local-light quality result until the pass reads real mattes and the gates are re-measured
against them.** Condition C2 is the untrained target head, C3 is the missing expert subtlety
study - which means **the headline KPI of this phase is unmeasured** - and C4 was the skipped
dependencies, which this merge closes for 16 and 17 and leaves open for 18.

Three rules that phase 19 adds and every later phase inherits:

- **`LocalService` is the only way to ask how light was shaped inside a photograph.**
  Fifteenth service of its kind. Phase 20 retouches skin this phase has already evened and
  must not do it twice; `idx_local_evened` is that query. Two answers to "what did we do to
  this face" is a portrait that gets lifted twice.
- **The per-image perceptual allowance is shared, stored and checked by the schema.** Six
  individually defensible adjustments are how a gallery quietly starts looking processed.
  Phase 20 adds a seventh operation and inherits the allowance rather than getting its own,
  and `LocalOp::PRIORITY` decides what is given up: face lighting has the first claim and
  dodge and burn the last, because a photographer would not miss the shaping.
- **A phase that consumes another phase's output owns no fallback for it.** When the field
  does not arrive the operation is *gated*, named in `gated_by_mask_quality` and reported in
  `LocalOutline::mask_covered`. A frame nobody could mask and a frame that needed nothing must
  never be the same query.

Three things phase 19 got wrong first, all found by its own gates, all worth generalising:

**A weight evaluated on a partially-edited value is not linear in its own strength.**
`apply_face_light` read its luminosity weights off the pixel *after* the exposure had moved, so
the highlight restraint grew quadratically in the mask's alpha while the lift grew linearly.
Past about half coverage the restraint overtook and a bright pixel received more lift at the
mask's edge than at its centre - which is a bright rim, made by arithmetic that looked
conservative. Any masked operator has this trap: **the weight must read the input.**

**A converged target cannot be used to detect its own constraints.** The joint face solve asked
"was this lift capped" by comparing against the group's common target, which had already
absorbed the caps in order to be reachable. Nothing was ever reported as capped and every unit
test passed. Compare against what was *wanted*, not against what was agreed.

**Section 10.1's edge-gradient halo test cannot be implemented as written.** Every local
brightening increases the step at its own boundary - that is what "local" means - so a
before/after gradient ratio scores the edit's size. Two refinements are also wrong and
ADR-0039 section 7 records why. What a halo is, is an edit that is stronger further from the
subject than nearer to it.

And one guarantee phase 19 deliberately weakened, which phases 20 and 25 will meet again:
**section 10.1's absolute group-fairness threshold is unachievable**, because a family formal
where one person is two stops down under a doorway cannot be evened without either refusing to
plan the frame or darkening everybody else. What is guaranteed instead is about the *edit*:
reach the threshold whenever the caps allow, and never make a group less even than you found
it. ADR-0039 section 6, and `docs/local-light.md` says the same thing in the product's voice.

Phase 20 is implemented conditionally: `aura-core::contract::retouch` freezes the four
operations, the two inpainting methods, the two bands an operator may name, the four presets, the
six protected kinds and their three sources, the protected feature, the texture report,
twenty-six reason codes, the plan, the outline, the override and `RetouchService`;
`aura-retouch` decides. `presets.rs` loads a table whose texture floors the *code* bounds rather
than the file, `strength.rs` computes one gallery-constant number per person from four gallery
statistics, `blemish.rs` finds marks by band-passing luminance **and** the red share of the
chromaticity one sign at a time, `permanent.rs` projects every mark onto the eye-to-eye axis and
calls a mark permanent only when it appears on four frames across forty-five minutes,
`undereye.rs` and `evening.rs` measure against the skin around them and are capped by the
contract, `texture_guard.rs` runs the plan through the real renderer, re-solves at three quarters
strength up to three times and **withdraws the retouch entirely** rather than shipping one that
failed its floor, `ops.rs` is one frame in and one plan out, `store.rs` owns migration 20 and
`api.rs` is the frozen service and the resumable walk. Migration 20 stores four tables, one view
and two triggers; `aura-render` gains `bands` and `retouch` plus three shaders held to them;
two models are signed with cards; eight IPC commands (ADR-0042) feed a Retouch panel; and
`aura-cli verify --phase 20` is the executable gate. Its exit report is
`docs/progress/PHASE-20-EXIT.md`.

**Both shipped heads are placeholders and neither is consulted, and this time that is not the
whole story.** `BLEMISH_HEAD_TRAINED` and `PERMANENT_HEAD_TRAINED` are false, and unlike phases
15, 16 and 18 - which refuse to consult a placeholder and fall back on a reference *model* - this
phase would have nothing underneath if it did the same, so what ships is a **measurement**: a
difference-of-Gaussians with a colour test, whose failure mode is finding fewer marks rather than
confidently wrong ones. ADR-0041 section 7 records the argument. Every gate in section 10.1 is
measured against synthetic faces whose marks were painted into the pixels and read back through
the real detector, the real operators and the real renderer, which proves the arithmetic and says
nothing about a wedding photograph. That is condition C1 and a Sev 2 trigger; it closes with
phase 05's C10 and with the `with_masks` wiring. Condition C2 is the second Sev 2 and it is the
one to be careful about: **there is no per-skin-tone parity study**, the mechanism is
tone-relative by construction and `docs/skin-fairness.md` says so, and no per-bucket number is
published or should be inferred.

Four rules that phase 20 adds and every later phase inherits:

- **`RetouchService` is the only way to ask what was done to somebody's skin.** Sixteenth service
  of its kind. Phase 21 retouches hair, teeth, eyes and clothing and must not re-smooth what this
  phase smoothed; phase 25 normalises a gallery of these decisions; phase 27 has to be able to
  say why a face looks worked on. Two answers to "what did we do to her skin" is a delivery in
  which the album and the gallery disagree about somebody's face.
- **A protected feature is a veto, not a discount - and one kind of it is absolute.** The
  candidate is dropped before strength, preset or guard is consulted, because "inpainted a
  little" on a mole is a smudged mole. `ProtectedKind::is_absolute` is true for a tattoo, and the
  refusal lives in the type, in `RetouchService::set_protection` and in a database trigger,
  because section 10.1 gates tattoo removal at **zero** rather than at a small number and a
  promise enforced in one layer lasts until somebody writes a second caller.
- **A guarantee about a pixel is enforced on the pixel.** Phase 16 wrote this for skin colour and
  this phase inherits it for skin texture: the guard applies the plan through the *renderer* and
  divides the band energies, rather than bounding a parameter and hoping. When it cannot reach the
  floor it withdraws the whole plan - a frame that ships unretouched is a much smaller failure
  than a frame that ships plastic, and a floor that can be exceeded once is not a floor.
- **A decision about a person is a gallery constant.** Strength is one stored number per identity
  per project; the frame decides which operations run, never how strong they are. Section 6.4's
  four inputs are read as gallery statistics, which is what makes section 10.1's five-per-cent
  consistency gate come out at zero. Any later phase that finds itself computing a per-frame
  strength for a person has re-created the failure this rule exists to prevent.

Three things phase 20 got wrong first, all found by its own tests, all worth generalising:

**Transplanting the original high band puts the mark back.** Section 6.2's words are "blend only
the low/mid bands while transplanting the original high band back", and read literally that heals
about a third of a spot: the *edge* of a mark is high-frequency content of the mark rather than of
the skin. Both halves have to come from the donor, with its texture rescaled to the energy of the
ring around the mark. The unit test that caught it was written before the code was right.

**A luminance-only detector misses the most common blemish on a wedding face.** An inflamed spot
is often no brighter or darker than the skin it sits on - it is redder. Any detector for a
*coloured* defect has to band-pass colour.

**A plain mean over a detected component reads a strong mark as a weak one.** A component includes
the falloff as well as the core, and a falloff sample is half skin - so painting the fixture spot
brighter made its temporary probability go *down*. Weight a region's reading by how far each
sample departs from the background it sits on.

Five rules that phase 13 adds and every later phase inherits:

- **`ExplainService` is the only way to record what happened.** Phase 27 writes QC decisions
  here, phase 28 reads the bands, phase 30's learning loop reads the whole table. Ninth
  phase, ninth time. Two ledgers is two answers to "what did the product do", and the one
  thing a support case cannot survive is a product that disagrees with itself about its own
  history.
- **A decision that cannot explain itself is not recorded.** Invariant 2 stops being a
  convention. `AURA-ML-5054` refuses a decision with no reason *and* one citing a code that is
  not in the shipped registry, and migration 13's `reason_count` CHECK refuses it again.
  The registry is assembled from phases 09 to 12's own frozen enums, so it cannot go stale
  and there is no way to write a reason no deciding phase can emit.
- **The record is append-only, and the database enforces it.** `decisions_no_update` aborts
  every `UPDATE`; a correction is a new row whose `supersedes` points backwards. The only
  thing that deletes is a compaction policy that cannot touch a photographer's own decision -
  and `supersedes` is deliberately not a foreign key, because every referential action SQLite
  offers is either that forbidden `UPDATE` or a `CASCADE` that would delete the correction.
- **Confidence is two numbers, and the band is stored.** `raw_confidence` is what the deciding
  code believed and `calibrated_confidence` is what that belief is worth; storing only one of
  them makes either the re-calibration unfalsifiable or the band a guess. `Explain::record`
  **overwrites** whatever band a caller supplied: a deciding phase that could set its own band
  is a deciding phase that could grant itself permission to act.
- **Evidence can never be a pixel.** `Evidence` is a crop rectangle, a list of frame ids or a
  list of named parameter deltas. There is no variant that could hold image bytes, which is
  what makes "a support bundle contains no pixels" a property of the shape rather than a
  promise about the exporter - and the bundle replaces every identifier with a handle anyway.

Two decisions in this phase are worth remembering because they will be re-argued later.
**Phases 09 to 12 were not rewritten to emit the unified model** - `aura_explain::adapt` maps
their own frozen vocabularies instead, because the property that has to hold is that the
deciding code owns the reason, and it already did. ADR-0027 section 4 has the argument. And
**analysis is not a decision**: phases 09, 10 and 11 measure, and recording four hundred
thousand "this frame is sharp" rows per wedding would be a ledger nobody can search and a size
budget nobody can meet. Their output reaches the panel as evidence underneath a cull decision.

Four rules that phase 12 adds and every later phase inherits:

- **`CullService` is the only way to ask what is being delivered.** Phase 14 edits
  survivors, phase 27 swaps in runner-ups, phase 29 builds albums out of keepers and phase
  30 uploads them. Eighth phase, eighth time, and the highest stakes it has had: two answers
  to "what is in this gallery" is a delivery that does not match the album that does not
  match the invoice.
- **A decision is reversible, and nothing on disk moves.** A rejection is a row with reasons
  and a pointer to what was kept instead. There is no path column, no file operation and no
  `deleted` flag anywhere in migration 12 or on the IPC surface - because this is the phase
  where "just move the rejects to a folder" first sounds reasonable.
- **A guarantee outranks a preference, in that order, always.** Modes, sliders, quotas and
  diversity caps are preferences; must-haves and identity minimums are guarantees. `modes.rs`
  cannot see the rule table and `Tuning` has nowhere to put one, so section 10.1's
  "Aggressive mode still satisfies all coverage rules" is a property of the type system
  rather than a test result that could drift. The one thing that *can* degrade a guarantee is
  the photographer removing every candidate by hand, and the report then says so and names
  the override.
- **Say what the gallery was chosen *from*.** `CullOutline::coverage` is the fraction of the
  project that carried a phase 09 verdict, and it is the most consequential denominator in
  the product: a cull over 60 % of a wedding is a gallery with a four-hour hole in it that
  looks exactly like a gallery with a decision in it. `AURA-ML-5050` exists so a frame nobody
  analysed is never rendered as a frame that lost.

Two decisions in this phase are worth remembering because they will be re-argued later.
**A veto excludes a frame from candidacy and not from a guarantee** - if the only photograph
of the ring exchange is out of focus the guard adds it, marks the rule `CoveredWeak` and
names the veto in a warning, because a blurred photograph of the rings beats no photograph
of the rings. And **`Missing` means nobody shot it**, never that the engine chose not to: a
rule that could have been satisfied and was not is a bug, and the gate fails on it.

Phase 12 also **closes phase 11's condition C8** - "out-of-focus beauty loses in phase 12",
which phase 11 could not honestly test because its consumer did not exist. It is now
`the_focus_veto_is_for_completely_out_of_focus_only` plus the geometric-mean fusion, and the
weight loader refuses a scene row that weights framing above whether the photograph worked.

Four rules that phase 11 adds and every later phase inherits:

- **`CompositionService` is the only way to ask how a frame is composed.** Phase 12 may
  combine the evidence and phase 23 may act on a hint; neither reimplements horizon or
  crop auditing.
- **A crop hint is evidence, never an edit.** Nothing in phase 11 moves a pixel, stores an
  applied crop, straightens, removes a distraction, or selects a frame.
- **An absent row is “not checked”, not “clean”.** Missing keypoints, an unknown scene,
  and an unavailable learned head reduce coverage or confidence and are rendered as
  caveats rather than quietly converted to favourable evidence.
- **Scene exceptions are data with reasons.** Intentional tilt, deliberate tight crops,
  centred details and neutral fallback live in `composition_rules.toml`; changing a band
  bumps `rules_ver` and re-analyses affected rows.

Five rules that phase 10 added and every later phase inherits:

- **`EmotionService` is the only way to ask what a photograph is worth.** No phase may keep
  its own expression model, its own idea of a peak or its own reaction linking. Sixth
  phase, sixth time: two answers to "which of these six frames is the one" is two galleries
  that disagree.
- **A score is evidence; the deciding phase owns the cull - and an *ordering* is still
  evidence.** This is the hardest version of the rule so far. Phase 09 produced a number
  that looked like a verdict; this produces a *sorted list*, which is one button away from a
  shortlist. `MomentBrowser` says "An ordering, not a shortlist" in its own header and a
  test asserts no label in it says keep, reject, deliver or cull.
- **Three version columns, and a fourth was deliberately not added.** `model_ver`
  invalidates every reading, `analysis_ver` the gaze, the peaks and the links, `weights_ver`
  the score. The ranker's coefficients ship *inside* `emotion_weights.toml` so one number
  invalidates the score - phase 09's rule read in the direction that removes a column.
  `AURA-ML-5038` is the sixth version-drift code.
- **Report coverage, and say what the denominator is.** `EmotionOutline::coverage` is
  measured against **every photograph**, as phase 09's is. `face_aware` is the second
  number and it is the one that matters when it is low: seven of the nine ranker features
  come from faces, so a wedding at 3 % face-aware has been ranked on very nearly nothing.
- **A weight table is a product decision and needs a written reason per row.** Third config
  file in `aura-brain-wedding` to enforce it and the one where it matters most:
  `emotion_weights.toml` is where the product decides that a composed Hindu ceremony is not
  an empty gallery, and a threshold nobody can explain there is a cultural failure waiting
  to be shipped. **In the four ceremony scenes composure is weighted at or above a smile**,
  three traditions raise it further, and two tests plus the gate check it in every scene
  rather than in the two the phase document names.

Five rules that phase 09 added and every later phase inherits:

- **`IntegrityService` is the only way to ask whether a frame worked.** No phase may keep
  its own sharpness measure, its own blink detector or its own idea of what "recoverable"
  means. Two answers to "is this frame sharp" is two culling decisions that disagree.
- **A measurement is evidence; the deciding phase owns the cull.** Nothing in
  `aura-brain-photo` rejects, ranks or orders a frame, and no column, field or command
  would let it. `technical_score` is the closest this product has come to something that
  *looks* like a verdict, and section 12's first failure mode is what happens when
  somebody reads it as one.
- **Three version columns, because they invalidate three different things.** `model_ver`
  invalidates the learned sharpness and every eye state, `analysis_ver` the motion kind,
  the exposure verdict, the noise figure, the flags and the score, and `calib_ver` every
  *normalised* number. `AURA-ML-5033` exists so a comparison across any of them never
  happens silently.
- **Report coverage, and say what the denominator is.** `IntegrityOutline::coverage` is
  measured against **every photograph** - unlike phase 08's, because a verdict needs only
  pixels - and `subject_aware` is the second number, because a wedding at 100 % coverage
  and 2 % subject-aware has been judged on frame-wide sharpness nearly everywhere.
- **A photographer's dismissal is unbeatable, and it is re-applied rather than excluded.**
  A locked moment *replaces* the machine's grouping; a dismissed flag does not replace the
  measurement, so the frame is still re-measured when the calibration table moves and the
  disagreement is carried onto the new measurement.

Five rules that phase 08 added and every later phase inherits:

- **`MomentService` is the only way to ask what was shot once.** No phase may keep its own
  grouping. Two answers to "are these the same shot" is two culling decisions that
  disagree, and phase 12's coverage guarantee is written against this one.
- **A grouping is evidence; the deciding phase owns the cull.** Nothing in `moments`
  rejects, ranks or deletes a frame, and no column, field or command on any surface would
  let it. `DuplicateSet::keep_hint` is spelled *hint* in the contract, the schema, the
  wire and the panel.
- **Three version columns, because they invalidate three different things.** `embed_ver`
  invalidates every distance and therefore every edge, `group_ver` the graph
  construction, and `profile_ver` the thresholds those edges were compared against.
  `AURA-ML-5028` exists so a comparison across any of them never happens silently.
- **Report coverage, and say what the denominator is.** `MomentOutline::coverage` is
  measured against **groupable** frames rather than photographs: a frame with no
  embedding is a phase 05 gap, and reporting it as a phase 08 failure sends somebody
  looking in the wrong place.
- **A photographer's grouping is unbeatable, and both sides of a split are locked.**
  `moments.user_locked = 0` is inside the `DELETE` a re-grouping starts with, and a locked
  moment's frames are *subtracted from the pass's input* rather than reconciled
  afterwards.

Five rules that phase 07 added and every later phase inherits:

- **`StoryService` is the only way to ask what a photograph is of.** No phase may keep its
  own scene classifier or its own idea of where the ceremony was.
- **A profile is evidence about a scene; the deciding phase owns the action.** Nothing in
  `aura-brain-wedding` culls, grades or crops. `SceneProfile::max_acceptable_noise` is a
  tolerance phase 09 measures against and phase 12 acts on.
- **Four version columns, because they invalidate four different things.** `model_ver`
  invalidates the posterior, `preprocess_ver` the context features, `taxonomy_ver` the
  rite's name, and `embed_ver` everything, because the trunk is underneath all of it.
  `AURA-ML-5022` exists so a comparison across any of them never happens silently.
- **Report coverage when you report a result.** `StoryOutline::coverage` is how a caller
  finds out that a story was drawn over 40 % of a wedding.
- **A photographer's chapter is unbeatable, and a boundary belongs to two chapters.**
  `segments.user_locked` and `image_scenes.source = 'user'` are checked inside the
  statements that would overwrite them, and moving a boundary locks both sides.

Five rules that phase 06 added and every later phase inherits:

- **`PeopleService` is the only way to ask who is in a photograph.** No phase may keep its
  own face store, its own clustering, or its own idea of who the couple are.
- **A photographer's decision is unbeatable.** `identities.user_locked` is checked inside
  the statement that would overwrite it, and the decision journal is replayed onto every
  fresh grouping *before* any conclusion is drawn from it. Automation never assigns `bride`
  or `groom`: the evidence identifies a pair, and which of two people is the bride is not a
  photographic fact.
- **Three version columns, because they invalidate three different things.** `model_ver`
  invalidates frames, `embed_ver` invalidates templates, `quality_ver` invalidates votes.
  Comparing across any of them returns a plausible number that means nothing;
  `AURA-ML-5018` exists so that never happens silently.
- **Report coverage when you report a result.** `SubjectHierarchy::coverage` is how a
  caller finds out that a grouping conclusion was drawn over 40 % of a wedding.
- **Never infer anything about a person.** Gender, ethnicity, religion and any relationship
  beyond couple, close family and guest are out of scope permanently - and the cloud task's
  output type cannot express them, so the rule is structural rather than remembered.

Five rules that phase 05 added and every later phase inherits:

- **`SimilarityIndex` is the only way to ask what looks like something.** No phase
  may keep its own vector store or its own graph. A second index is a second answer
  to "are these two frames the same shot", and the two will disagree.
- **A distance is evidence; the deciding phase owns the threshold.** Nothing in
  `aura-index` decides anything, and `query::NEAR_DUPLICATE_HAMMING` is a label in
  a debug panel rather than a policy. Phase 07 owns scene thresholds, phase 08 owns
  duplicate policy.
- **Bump `PREPROCESS_VER` on any change to the pixels the model sees, and
  `MODEL_VER` on any change to the model.** Comparing a vector from one version
  with a vector from another returns a plausible number that means nothing;
  `AURA-ML-5015` exists so that never happens silently.
- **Report coverage when you report a result.** A grouping conclusion drawn over a
  40 %-embedded project is a conclusion about 40 % of a wedding, and
  `IndexStatusDto.coverage` is how a caller finds out.
- **Descriptors are computed once.** The histogram, the luminance percentiles, the
  edge energy and the palette are in the catalog from phase 05 onward. A phase that
  recomputes one of them is opening a file that did not need opening.

Four rules that phase 04 added and every later phase inherits:

- **`CloudAiGateway` is the only way to reach a model provider.** No phase may
  open a socket; `scripts/check-banned.sh` enforces it exactly as it does for the
  inference runtime.
- **A task without a local fallback does not compile**, and neither does one
  whose `Output` cannot state its confidence and reasons. Invariants 2 and 6 are
  trait bounds, not review items.
- **Bump `CloudTask::VERSION` on any prompt, schema or ceiling change.** The
  cache key contains it, and a stale answer served under a contract that no
  longer exists is worse than no answer.
- **Cloud proposes; deterministic code decides.** A cloud answer may not overrule
  a local decision at confidence 0.90 or above unless it cites contradicting
  visual evidence, and the conflict is logged.

Four rules that phase 03 added and every later phase inherits:

- **`InferService` is the only way to run a model.** No phase may link a runtime
  directly; `scripts/check-banned.sh` enforces it. The `Backend` port inside
  `aura-infer` is deliberately *not* frozen, so a GPU backend can be added
  without an ADR and without touching a caller.
- **No model card, no model.** `cargo xtask models` refuses an unsigned manifest,
  a digest that moved, and a card that is missing a required section. It runs in
  CI lane 1 beside the contract check.
- **A model is pending until it has worked once.** A version that fails its first
  real use is rolled back automatically and recorded as rejected; the
  photographer keeps the quality they had that morning.
- **Numbers come from runs.** The GPU throughput budgets in the phase document
  are *waived*, with an expiry condition, rather than filled in with plausible
  figures. Model cards leave unmeasured reference-machine rows empty.

Phase 03 started under a written waiver (ADR-0006): phase 02's three exit
conditions - real camera files, a photographed ColorChecker, and a three-OS CI
run - need inputs that do not exist in the repository. They are carried forward
in section 8 of the phase 03 exit report, and **the first real camera file is a
Sev 2 trigger that reopens phase 02's criteria whatever phase is in flight.**

Two rules that phase 02 added and every later phase inherits:

- **`PIPELINE_VER` is a contract.** It keys both the preview cache and every
  training dataset. Bumping it needs ML-lead sign-off and a model re-validation.
- **Pixels carry their provenance.** `PixelSource` says whether a buffer came
  from the camera's own JPEG or from AURA's documented render. Never mix the two
  in a score without recording which one it was.
