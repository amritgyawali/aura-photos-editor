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
| Portrait retouch decisions | `docs/adr/ADR-0043-portrait-retouch-and-texture-protection.md` |
| Retouch presets and per-scene limits (versioned, PM-owned) | `crates/aura-retouch/config/retouch_presets.toml` |
| Retouch evaluation gates | `tests/eval/retouch_eval.rs` + `ml/models/retouch/eval_retouch.py` |
| What AURA does to skin, in the product's own words | `docs/retouch.md` |
| Micro-retouch and cross-frame borrowing decisions | `docs/adr/ADR-0045-micro-retouch-and-cross-frame-borrowing.md` |
| The opt-in matrix, ceilings and loci (versioned, PM-owned) | `crates/aura-retouch/config/micro_retouch.toml` |
| Micro-retouch evaluation gates | `tests/eval/micro_eval.rs` + `ml/models/micro/eval_micro.py` |
| What AURA will and will not do to somebody's appearance | `docs/retouch-ethics.md` |
| Restoration decisions | `docs/adr/ADR-0047-restoration-denoise-sharpen-and-identity.md` |
| Scene ceilings for restoration (versioned, PM-owned) | `crates/aura-restore/config/restore_profiles.toml` |
| Per-camera noise models (versioned, COL-owned) | `crates/aura-restore/config/noise_models/` |
| Restoration evaluation gates | `tests/eval/restore_eval.rs` + `ml/models/restore/eval_restore.py` |
| What AURA does to a noisy or soft photograph | `docs/restoration.md` |
| Geometry, lens and crop-safety decisions | `docs/adr/ADR-0041-geometry-lens-straightening-and-crop-safety.md` |
| Crop rules (versioned, PM-owned) | `crates/aura-geometry/config/crop_rules.toml` |
| Bundled lens profiles, with attribution | `assets/lens_profiles/` |
| Geometry evaluation gates | `tests/eval/geometry_eval.rs` + `ml/eval/crop_agreement.py` |
| What AURA does to a frame's edges, in the product's own words | `docs/geometry-and-cropping.md` |
| Generative cleanup decisions | `docs/adr/ADR-0049-generative-cleanup-and-the-safety-engine.md` |
| Cleanup policy (versioned, PM + SEC co-owned) | `crates/aura-generative/config/cleanup_policy.toml` |
| Cleanup evaluation gates | `tests/eval/cleanup_eval.rs` + `ml/models/generative/eval_cleanup.py` |
| What AURA will and will not generate | `docs/generative-policy.md` |
| Gallery consistency decisions | `docs/adr/ADR-0051-gallery-consistency-and-normalisation.md` |
| Consistency policy (versioned, PM-owned) | `crates/aura-brain-gallery/config/consistency.toml` |
| Consistency evaluation gates | `tests/eval/consistency_eval.rs` + `ml/eval/consistency_eval.py` |
| How AURA makes a wedding look like one gallery | `docs/gallery-consistency.md` |
| Camera matching decisions | `docs/adr/ADR-0053-camera-matching-and-appearance-distance.md` |
| Camera matching policy (versioned, PM-owned) | `crates/aura-brain-gallery/config/camera_match.toml` |
| Bundled brand baselines, none of them measured | `assets/camera_baselines/` |
| Camera matching evaluation gates | `tests/eval/camera_eval.rs` + `ml/eval/camera_match_eval.py` |
| How AURA matches two cameras and two shooters | `docs/camera-matching.md` |
| Quality-control decisions | `docs/adr/ADR-0055-quality-control-tickets-and-the-re-edit-loop.md` |
| QC thresholds (versioned, PM-owned) | `crates/aura-qc/config/qc_thresholds.toml` |
| QC evaluation gates | `tests/eval/qc_eval.rs` + `ml/eval/qc_agreement.py` |
| How AURA checks its own work | `docs/how-qc-works.md` |
| Autopilot decisions | `docs/adr/ADR-0057-autopilot-orchestration-and-autonomy.md` |
| The checklist and the resource budgets (versioned, PM-owned) | `crates/aura-jobs/config/autopilot.toml` |
| Autopilot gates | `tests/e2e/autopilot_chaos.rs`, `tests/e2e/autopilot_run.rs` |
| One button, in the product's own words | `docs/autopilot.md` |
| Curation decisions | `docs/adr/ADR-0059-curation-selection-and-album-composition.md` |
| Album sizes, rhythm, hero and monochrome weights (versioned, PM-owned) | `crates/aura-curate/config/curation.toml` |
| Curation evaluation gates | `tests/eval/curate_eval.rs` + `ml/models/curate/eval_curate.py` |
| What AURA proposes after the cull, in the product's own words | `docs/curation.md` |
| Delivery and learning decisions | `docs/adr/ADR-0061-delivery-learning-loop-and-release.md` |
| Export presets (versioned, PM-owned) | `crates/aura-export/config/export_presets.toml` |
| Delivery evaluation gates | `tests/eval/delivery_eval.rs` |
| What a delivery promises, in the product's own words | `docs/delivery.md` |
| What AURA will and will not learn from you | `docs/learning-loop.md` |
| Release checklist, signing, rollout and flags | `ops/release/`, `ops/sign/`, `ops/notarise/`, `ops/update/`, `ops/flags/` |
| How a release ships | `docs/release-process.md` |
| What leaves the machine, and what does not | `docs/privacy.md` |
| Branching, landing and merging a phase | `scripts/phase-branch.sh`, `scripts/phase-land.sh`, `docs/runbooks/phase-landing.md` |

## Non-negotiables enforced by the build

- `scripts/check-banned.sh` fails on `unwrap()`, `expect(`, `panic!`, `HashMap::new`,
  `SystemTime::now`, `Instant::now` and `any` in UI source, outside tests, benches,
  `xtask` and `main.rs`.
- Every crate root carries the lint block, including `#![forbid(unsafe_code)]`.
- `aura-core` depends on no other workspace crate; a test asserts it.
- Changing a frozen contract requires an ADR and a re-lock, in that order.
- **Every phase begins by cutting its branch and pushing it**, and **ends by merging its
  own pull request into `main`**, without being asked. Steps 0 and 9 of the ritual in
  `docs/plan/CLAUDE.md`, and two commands:

  ```bash
  scripts/phase-branch.sh 25 gallery-consistency        # step 0, before any code
  scripts/phase-land.sh --message "feat(gallery): ..."  # step 9, after the gate exits 0
  ```

  The first cuts `feat/phase-NN-<slug>` off an up-to-date `origin/main` and pushes it
  immediately, so a phase is visible from its first minute rather than its last. The
  second commits what is left, pushes, opens the pull request over the GitHub REST API,
  refuses to merge on a failed check, merges into `main` and leaves the checkout on an
  up-to-date `main`. `just phase-start` and `just phase-ship` are the same two commands.
  `gh` is used for the token when it is installed and is not required; the runbook is
  `docs/runbooks/phase-landing.md`.

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

**All thirty phases are implemented, and the eight process gaps the independent review found are
closed.** `docs/progress/PHASE-01-30-REVIEW.md` is the review; its section 10 is what was done
about it. Three things a later agent needs from that page rather than from this one:

- **Every UI source file is reachable from `main.tsx`, and a test keeps it that way.** Forty-two
  of eighty-two were not, for nineteen phases - the whole develop stack, people, story, style,
  cull, cleanup and camera matching. `App.tsx` now mounts nine workspaces;
  `ui/src/reachability.test.ts` fails on one orphan. **A component with passing tests is not a
  mounted component**, and nothing in this repository checked the difference until it did.
- **All thirty phase gates run.** `scripts/check-phase-gates.sh` runs them and refuses to run at
  all unless it finds thirty gate modules in `aura-cli`, so a phase whose gate is not wired in is
  a red build. It is CI lane `phase-gates` and the `phases` release gate. Sixteen of them had run
  nowhere.
- **`ui/src-tauri` is compiled by CI lane `shell` and by nothing on this machine.** The reference
  Windows machine has no linker for it - no `gcc` under the GNU toolchain, no MSVC linker - so
  `just shell-check` will not run here. That lane is the only thing that type-checks the IPC
  boundary; `scripts/check-ipc-surface.sh` proves the names and not the types, and says so.

**Nothing in section 7 of the review is closed and nothing in it can be closed by writing code.**
Every model-capability flag is still false, no camera file has ever been decoded, nothing is
calibrated, no lens or brand baseline is measured, there is no GPU backend, no network transport
and nothing has been signed. The product is architecturally complete and evidentially empty, and
every quality number anywhere in it was measured against a fixture this repository authored.

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
failed its floor, `ops.rs` is one frame in and one plan out, `store.rs` owns migration 21 and
`api.rs` is the frozen service and the resumable walk. Migration 21 stores four tables, one view
and two triggers; `aura-render` gains `bands` and `retouch` plus three shaders held to them;
two models are signed with cards; eight IPC commands (ADR-0044) feed a Retouch panel; and
`aura-cli verify --phase 20` is the executable gate. Its exit report is
`docs/progress/PHASE-20-EXIT.md`.

**Both shipped heads are placeholders and neither is consulted, and this time that is not the
whole story.** `BLEMISH_HEAD_TRAINED` and `PERMANENT_HEAD_TRAINED` are false, and unlike phases
15, 16 and 18 - which refuse to consult a placeholder and fall back on a reference *model* - this
phase would have nothing underneath if it did the same, so what ships is a **measurement**: a
difference-of-Gaussians with a colour test, whose failure mode is finding fewer marks rather than
confidently wrong ones. ADR-0043 section 7 records the argument. Every gate in section 10.1 is
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

Phase 21 is implemented conditionally: `aura-core::contract::micro` freezes the five operations,
the ten regions and their total mapping onto phase 18's vocabulary, the mask port, the two
colour-locus shapes, the five clothing issues, the two glare methods, three op families, the
naturalness guard and its report, thirty-three reason codes, the plan, the outline, the override and
`MicroService`; `aura-retouch::micro` decides. `matrix.rs` loads an opt-in table whose ceilings the
*code* bounds rather than the file, `hair.rs` finds thin high-contrast structures in the halo
outside the hair alpha and refuses every one of them over a background with detail of its own,
`teeth.rs` and `eyes.rs` measure a mouth and a pair of eyes against the frame's own neutral and
its own redness, `clothing.rs` finds small anomalies inside the garment and vetoes patterned
fabric entirely, `glare.rs` finds specular sheets over an iris, `borrow.rs` searches a sibling
frame for an alignment and **refuses to composite anything that still carries information**,
`guard.rs` runs the plan through the real renderer and measures the catchlights, the hairline and
the teeth, re-solving at three quarters strength up to three times and withdrawing a family that
still misses, `ops.rs` is one frame in and one plan out, `store.rs` owns migration 22 and `api.rs`
is the frozen service and the resumable walk. Migration 22 stores three tables, two views and two
triggers; 22 argued-over scene rows and a neutral one live in editable config; two shaders ship with the processor
reference they are held to; three models are signed with cards; nine IPC commands (ADR-0046) feed
a Micro-Retouch panel; and `aura-cli verify --phase 21` is the executable gate. Its exit report is
`docs/progress/PHASE-21-EXIT.md`.

**All three shipped heads are placeholders and none is consulted.** `FLYAWAY_HEAD_TRAINED`,
`GLARE_HEAD_TRAINED` and `LINT_HEAD_TRAINED` are false, so what runs is the measured detection
ADR-0045 section 6 argues for - and the argument is not the same for all three: glare and lint are
*measurements by definition*, and the flyaway detector is deliberately the most conservative of
them because a measurement cannot tell a strand from a twig. Every gate in section 10.1 is
measured against synthetic frames whose strands, sheets, marks and teeth were painted into the
pixels and read back through the real detectors, the real operators, the real guard and the real
renderer. That is condition C1 and a Sev 2 trigger. Condition C2 is the second and it is the
headline: **the naturalness audit did not happen**, so the phase's own KPI - corrections judged
natural at or above 95 % - is unmeasured, and no claim about naturalness may be made from this
build.

Four rules that phase 21 adds and every later phase inherits:

- **`MicroService` is the only way to ask what was done to somebody's hair, teeth, eyes or
  clothes.** Seventeenth service of its kind. Phase 22 restores and sharpens, phase 24 removes
  objects, phase 25 normalises a gallery of these decisions and phase 27 has to be able to say why
  a face looks worked on. No phase may keep its own flyaway detector, its own teeth locus or its
  own idea of what a borrow is.
- **A borrow may only replace pixels that carry no information, and it is disclosed in five
  places.** This is the first code in the product that composites two photographs. What separates
  a glare repair from the eye swap section 2.2 forbids is not the mechanism: a specular sheet has
  destroyed the record, and a closed eye *is* the record. `MIN_SPECULAR_FRACTION` is that rule as
  a number, `GlareMethod::BorrowFrom` carries the source in the type, and a database trigger
  aborts any statement that would take it away.
- **A ceiling can be lowered by a studio and raised by nobody.** The contract owns every bound,
  the config file may only tighten one, and there is no strength field anywhere on the IPC
  surface. That is what makes `docs/retouch-ethics.md` a promise about the product rather than a
  description of its defaults - and it is the shape any later phase that touches somebody's
  appearance should copy.
- **A guarantee is measured per family, not per plan.** Phase 20's texture guard withdrew a whole
  retouch; this one withdraws hair, teeth or eyes independently, because the three measurements
  are over disjoint regions and a frame whose teeth could not be evened safely should still get
  its lint removed. Collapsing them would throw away work for a reason that has nothing to do with
  it.

Three things phase 21 got wrong first, all found by its own gates, all worth generalising:

**A chance-corrected margin cannot be met at an extreme marginal rate.** `eval_micro.py` required
the retouchers to agree an absolute 0.10 above chance, and at a 97 % natural rate chance agreement
is already 0.92 - eight points of headroom for a ten-point margin, so a *perfect* panel failed the
gate. It is now a share of the available headroom, which is Scott's pi. This is the same shape of
defect as phase 19's edge-gradient halo test: **a threshold a correct implementation cannot meet
is a bug in the threshold**, not a finding.

**A per-image storage figure written before it was measured was wrong by a factor of two.** The
store documented 612 B and measures 1,633 B over a thousand rows. The reason is structural and is
worth remembering when phase 22 or 24 writes its own: every phase from 09 to 20 stores **one
fixed-width verdict** per photograph, and this stores a **list** whose length is the number of
things that were wrong with the frame. It is also the first figure in the product above a kilobyte
per image, and `perf/budgets.toml` carries the argument rather than the aspiration.

**A refusal test that cannot tell a working guard from a broken fixture proves nothing.** The
phase gate's two trigger checks read "the statement failed" as a pass, and an INSERT refused for a
missing foreign key looks exactly like one refused by the promise. Both now run a control first
and report `Inconclusive` rather than success when the attempt never reached the thing under test.

Phase 21 also **closes phase 20's condition C5**: `ui/src-tauri` had no `icons/` directory and its
`main.rs` had lost `fn main` in the phase 19 to 20 merge, so the desktop shell had not compiled
since. Both are fixed, the shell builds under the GNU toolchain with the target directory moved
off the space-containing project path, and this phase's nine commands are registered in it - 75 of
the product's IPC commands were reachable from the window when this phase shipped.

The merge that brought phases 20, 21 and 22 onto main closed that gap for the whole product, and
found it was much larger than any exit report had said. `ui/src/ipc/client.ts` carried 179 typed
wrappers and the shell registered 89 commands, so **ninety client calls reached a window that did
not answer to them** - `get_preview`, `render_image`, `set_param`, every mask command, every moment
command. All 180 are registered now, they are generated from the command functions' own signatures
rather than typed by hand, and `image_subjects` was missing from `aura-app`'s own `pub use` list as
well, so nothing anywhere could have called it. The three-way count is **180 handlers, 180
`#[tauri::command]` definitions, 180 typed client wrappers**, and a script asserts the equality.

The shell's own Rust does not compile on this machine - `dlltool` is absent and the toolchain's
`self-contained` copy has no assembler to call - so this is verified by `rustfmt` parsing the file
plus that symbol cross-check: every handler name has a definition, every definition calls an
`aura_app` function the crate re-exports, and every DTO the shell imports is a type `contract::ipc`
defines. **That proves the names and the syntax and not the types**, and it is weaker than a build.

**What is still not reachable is the panels.** `ui/src/App.tsx` mounts three of them - problems,
hardware and AI keys - and every develop panel from phase 12 onward exists with tests and is
imported nowhere. The commands answer and the components are props-driven; no view puts the two
together. That is the remaining half of phase 21's condition C6, it is a UI-shell task rather than
any one phase's, and it is what stands between this product and a photographer using it.

Phase 22 is implemented conditionally: `aura-core::contract::restore` freezes the four tiers, the
seven regions, the mask port, the sensor noise model, the denoise spec, the sharpen spec and its
mask, the per-face recovery record, the three destinations, the two occasions, the artefact report,
thirty reason codes in four subjects, the plan, the outline, the override and `RestoreService`;
`aura-restore` decides. `profiles.rs` loads 22 scene ceilings and twenty camera noise models and
refuses a file that widens a bound the code owns, `denoise.rs` chooses one of four tiers from phase
09's measured `noise_sigma_rel` and turns it into three amounts under *this* sensor at *this* ISO,
`kernel.rs` measures the blur from the frame's own edge profiles at a low quantile, `sharpen.rs`
refuses deconvolution unless four independent preconditions hold, `face_recovery.rs` checks a
narrow softness band before any model is consulted and then holds every face to a cosine-distance
ceiling measured through the real renderer, `selfcheck.rs` measures texture retention and ringing
on the rendered result and steps two independent levers, `schedule.rs` keeps the work off the
interactive path and can never reach a provider, `decide.rs` is one frame in and one plan out,
`store.rs` owns migration 23 and `api.rs` is the frozen service and the resumable walk. Migration
23 stores two tables, two views and one trigger; `aura-render` gains `restore` plus two shaders
held to it; two models are signed with cards; seven IPC commands (ADR-0048) feed a Restore panel;
and `aura-cli verify --phase 22` is the executable gate. Its exit report is
`docs/progress/PHASE-22-EXIT.md`.

**Both shipped heads are untrained, and for the first time in this product one of them ships as a
refusal rather than as a measurement.** Denoising falls back on a noise-model-conditioned
edge-preserving filter whose failure mode is leaving noise behind, which a photographer can see and
correct. Face recovery falls back on **nothing**: `FACE_RECOVERY_HEAD_TRAINED` is false,
`solve` returns `None` on every frame, and no face in this build is recovered - because the
measurement that would stand in for a face prior is unsharp masking on a face, which is a different
operation with a worse result and the same name. ADR-0047 section 6 has the argument. Every gate in
section 10.1 is measured against synthetic frames whose noise, blur and structure were painted into
the pixels and read back through the real detectors, the real operators and the real renderer.
That is condition C1 and a Sev 2 trigger. Condition C2 is the second Sev 2 and it is the one to be
careful about: **the identity constraint is measured with an untrained recogniser**, so what the
gates prove is that it refuses what it should refuse, and not that a real embedding would notice a
real identity change. Condition C4 is the third gap and it is the headline: **the expert preference
study did not happen**, so the phase's own KPI is unmeasured and no claim about how a restored
photograph looks may be made from this build.

Four rules that phase 22 adds and every later phase inherits:

- **`RestoreService` is the only way to ask what was repaired in a photograph.** Eighteenth service
  of its kind. Phase 23 straightens and crops after this phase sharpens and must not sharpen again;
  phase 24 fills generatively and inherits this phase's identity constraint rather than re-deriving
  one; phase 25 normalises a gallery denoised at four different tiers; phase 27 has to be able to
  say why an edge looks crunchy. Two answers to "how much noise reduction did this frame get" is a
  delivery where the album and the gallery disagree about the same photograph's grain.
- **A repair that cannot be measured is not performed.** The denoiser does nothing without a sigma,
  sharpening refuses without regions rather than running at a lower amount, and the identity
  constraint skips a face it cannot embed. This is the strongest form the rule has taken: phase 19
  said a phase owns no fallback for another phase's output, and here the absence of an input is a
  *refusal* rather than an attenuation, because the three regions sharpening needs are exactly the
  three places sharpening is visible as damage.
- **A guarantee is a stored number on every row, including the rows where nothing happened.**
  `restore_face.identity_drift` is written whether the face was kept or skipped, so "no delivered
  face was changed" is `SELECT MAX(identity_drift) FROM restore_face WHERE skipped = 0` rather than
  a sentence. A refusal is a row, because here the refusal *is* the product working.
- **A ceiling can be lowered by a studio and raised by nobody - and one bound runs the other way.**
  `restore_profiles.toml` may lower the sharpening ceiling and the face-recovery cap, and may only
  *raise* `skin_attenuation`, because "sharpening is explicitly attenuated on skin" is section 6.2
  of the phase document rather than a default somebody chose.

Three things phase 22 got wrong first, all found by its own gates, all worth generalising:

**A threshold on a measurement is a statement about the instrument as well as about the world.**
`SHARPEN_KERNEL_LO` was set from optics at 0.55, and the estimator measures the width of a Sobel
gradient ridge - which for a mathematically perfect step edge is two samples, a sigma of 0.849.
Nothing can measure below that, so *every frame in every wedding* would have passed the kernel
precondition. A synthetic chequerboard came back needing sharpening. Phase 19's edge-gradient halo
test could not be met by a correct implementation and phase 21's agreement margin could not be met
by a perfect panel; **a threshold nothing can meet and a threshold everything necessarily meets are
the same bug**, and the fix in all three cases was to define the threshold against the instrument.

**A ringing measurement must score the excursion, not the edit.** Comparing the gradient before and
after measures how hard the frame was sharpened, because every sharpening increases the step at an
edge. What ringing is, is a pixel pushed **beyond the range its own neighbourhood had before the
operation**. Phase 19 made the same mistake with its halo test and ADR-0039 section 7 recorded it;
this is the same lesson in the phase that could most easily have shipped it.

**Cosine distance is a function of direction, so a "more sensitive" probe built by scaling one
component is less sensitive.** The identity test probe multiplied its high-band term by a gain, and
raising the gain made the before and after vectors both point along that component - the distance
between them collapsing toward zero. A broken constraint would have passed. The response is an
angle now. Any test that turns a magnitude into a direction has this trap.

Phase 22 also makes two changes to phase 14's render graph, both recorded in ADR-0047 section 2 and
neither to a frozen file. **`restoration.denoise` now invalidates from `Stage::NoiseReduction`
rather than from `Stage::Restoration`**, which is a latent cache-invalidation bug nothing had hit
because nothing wrote the field; and a denoise tier alone no longer enables `Stage::Restoration`,
because this phase occupies *two* of phase 14's stages rather than the one named after it.
Denoising is a sensor-domain operation and belongs at index 6, before every stage that reads
texture as signal; only face recovery belongs at index 19.

Phase 23 is implemented conditionally, and it was **written out of order**: it was built on top
of phase 19, with 20, 21 and 22 not yet existing, and it reached `main` before them. They are
here now, merged behind it and renumbered onto the migration, the error codes and the ADR
numbers this phase had already taken - so this section's own numbers are the ones it shipped
with and the three sections above it are the ones that moved. Unlike phase 19 it consumes
nothing that has not shipped - its dependencies are phases 06, 11 and 14, and all three are
here.
`aura-core::contract::geometry` freezes `GeometryPlan`, `LensCorrection`, `Keystone`,
`CropVariant`, `CropPurpose`, `Aspect`, `CropSafetyReport`, the `ProtectedRegion` input port,
twenty-four reason codes, the outline, the override and `GeometryService`; `aura-geometry`
picks one of three routes to a lens correction and withholds fringing on the weakest of them,
tracks straight edges out of a proxy and fits a distortion coefficient from them, gates a
rotation on confidence and band and then *solves* it against the crop it implies, fits a
vanishing point from at least three verticals and refuses past the stretch cap, filters
candidate rectangles for safety **before** the composition objective ever sees one, searches a
bounded lattice, and generates the aspect variants an album and a feed need. Migration 20 stores
the plan and its crops at 839 bytes a photograph; 23 argued-over scene rows live in editable
config; eight fabricated lens profiles ship with attribution; `geometry.wgsl` and
`aura-render::geometry` apply it; six IPC commands (ADR-0042) feed a Geometry panel; and
`aura-cli verify --phase 23` is the executable gate. Its exit report is
`docs/progress/PHASE-23-EXIT.md`.

**This phase ships no model** - the third since phase 08, and for the same reason phase 17 gave:
there is nothing to train. What is missing is not weights but *weddings*. Section 9's DATA task
asks for expert crop labels on two thousand frames and there are none, so every gate is measured
against frames whose geometry was chosen, painted into the pixels and read back through the real
pipeline. That proves the estimator, the tracker, the caps, the safety filter, the search and
the store; it is not evidence that a photographer would agree with a crop, and section 10.1's
audit of 300 auto-crops has not happened. That is condition C1 and a Sev 2 trigger. C2 is the
second: **every bundled lens profile was fabricated**, no lens was measured, and the first
measured profile reopens this phase's criteria whatever phase is in flight - exactly as the
first real camera file reopens phase 02's.

Four rules that phase 23 adds and every later phase inherits:

- **`GeometryService` is the only way to ask how a photograph's frame was finished.** Nineteenth
  service of its kind - sixteenth when this phase was written, because phases 20, 21 and 22 had
  not yet added `RetouchService`, `MicroService` and `RestoreService` in front of it. Phase 27 checks these crops, phase 29 lays albums out of the variants and
  phase 30 exports them; two answers to "what is this photograph's frame" is an album page
  cropped from a rectangle the gallery never delivered.
- **A crop that cannot be proven safe is not a candidate.** The safety filter runs before the
  objective, never after it as a penalty. Phase 12's rule - a guarantee outranks a preference -
  applied where the preference is a tuned score and the guarantee is a bride's hands. A filter
  applied afterwards invites exactly one repair: nudge the winning rectangle until the face is
  back inside, and a nudged crop is a different aspect ratio, a different resolution, or a fresh
  violation at the opposite edge. Any later phase that finds itself adjusting a rejected
  rectangle has misunderstood the ordering.
- **The optics maths has one implementation, in the lowest crate both sides reach.**
  `aura_raw::colour::lens`, beside the camera matrices and the monotone curve, for the reason
  those are there: this crate decides where a face box lands after a correction and
  `aura-render` draws it, and two copies of the polynomial is two answers to where the face is.
- **The corners this phase opens are phase 24's to fill, and phase 24 must not widen the crop to
  hide them.** The rectangle here is the one the safety filter passed.

Two decisions in phase 23 are worth remembering because they will be re-argued. **The lens
coefficients travel in the recipe rather than being looked up at render time** - the tidier
alternative fails phase 14's rule that a delivered file can be re-created from four values,
because a coefficient living only in a profile table is a fifth, and updating that table changes
what an already-delivered photograph looks like under an identical hash. And **the improvement
margin applies to the primary crop and to nothing else**: a 1:1 crop of a wide reception frame
will essentially never score better than the frame it came out of, so requiring an improvement
from the aspect variants ships a product with no square variants in it at all.

Three things phase 23 got wrong first, all found by its own gates, and all three the same
shape - **a measurement pipeline can be wrong in a way that is self-consistent**, so unit tests
over synthetic inputs to the *fitter* will not find it, because the fitter is correct:

**A crossing is not an ending.** The edge tracker died at every line intersection, because the
gradient *along* one edge collapses for two or three pixels where another crosses it. An
eleven-by-eleven grid produced chains of twenty-three pixels and the span floor rejected every
one: zero chains, on a plate made of nothing but straight lines, with every unit test passing.

**A robust fit must reject the chains no coefficient can straighten, not the chains with the
largest residual.** Trimming the worst third scored 0.000 against a painted 0.020 - because on a
distorted frame the largest residuals belong to the chains nearest the *edge*, which are the
only ones that see any distortion at all. Trimming by residual keeps the optical centre and
throws away the evidence.

**Re-acquiring after a gap needs a window as wide as the gap.** A one-pixel search re-acquires
at the wrong place after a three-row crossing, holding the chain flat at every intersection and
straightening the very curvature the estimator exists to measure. It biased the answer low by
about a sixth, and **every chain agreed with every other chain about the wrong answer**.

Phase 23 also amended a frozen contract for the third time in the product's history, after phase
09's `FaceRef` and phase 16's re-lock: `aura_recipe::Lens` gained `coefficients`. And it added
the fourth grep-as-a-test, `crates/aura-geometry/tests/no_render_calls.rs`, which fails the build
if the deciding crate ever reaches a pixel, writes a recipe, or grows a face detector.

Phase 24 is implemented conditionally: `aura-core::contract::cleanup` freezes the ten-class
distraction vocabulary, the five safety checks, the verdict, the three methods, thirty-one reason
codes, the proposal, the disclosure, the outline, the override and `CleanupService`, and `ids.rs`
gains `ProposalId`; `aura-generative` decides. `policy.rs` loads 23 scene rows whose bounds the
*code* owns rather than the file, `safety.rs` runs the five checks of section 6.2 **before anything
is scored**, `denylist.rs` intersects against phase 18's regions and treats an absent mask as
ignorance rather than as safety, `detect.rs` finds unexplained salience and names nothing,
`borrow.rs` fits an exhaustive least-median homography from a ring of control patches and refuses a
sibling that still carries the object, `fill.rs` synthesises from exemplars and corrects the seam
with a harmonic field, `inpaint.rs` declares a diffusion tier and refuses on every call, `source.rs`
is the one choke point, `selfcheck.rs` measures three artefacts against the rest of the frame and
reverts, `judgement.rs` is a cloud port whose answer type cannot approve anything, `queue.rs` is one
photograph in and one plan out, `store.rs` owns migration 24 and `api.rs` is the frozen service and
the resumable walk. Migration 24 stores four tables, two views and four triggers; `aura-render`
gains `Stage::Cleanup` at index 18 plus `cleanup_paste.wgsl`; `aura-recipe` gains `Recipe.cleanup[]`;
nine IPC commands (ADR-0050) feed a proposal queue, a before-and-after and a manual removal tool; and
`aura-cli verify --phase 24` is the executable gate. Its exit report is
`docs/progress/PHASE-24-EXIT.md`.

**This phase ships no model** - the fourth since phase 08 - and unlike phases 17 and 23 the reason is
data rather than there being nothing to train. Section 9's DATA row asks for a labelled
wedding-distraction vocabulary on ten thousand frames and there are none, so `DISTRACTION_HEAD_TRAINED`
is false and `detect::candidates` returns `DistractionClass::Unclassified` for everything it finds -
which cannot be shown to be story-irrelevant, so the safety engine refuses all of it. **This build
therefore proposes no removals on a real photograph**, which is the correct behaviour for a build
that cannot tell a bin from a gift. That is condition C2 and a Sev 2 trigger. Condition C1 is the
other Sev 2 and it is the one to remember: **phase 18's twenty mask classes contain no word for a
ring or a cake**, so a coverage assembled from them is never complete even with a trained segmenter,
and `CleanupOutline::mask_covered` is 0 % on every project. Condition C4 is the third gap and it is
the headline: the *human* adversarial audit of section 9 did not happen, so no claim about
artefact-free rate on a wedding may be made from this build.

Five rules that phase 24 adds and every later phase inherits:

- **`CleanupService` is the only way to ask what was removed from a photograph.** Twentieth service
  of its kind. Phase 27 has to be able to say why a background looks smeared; phase 28 must know
  what ran unattended; the delivery report lists every disclosure. No phase may keep its own
  distraction detector, its own denylist or its own idea of a safe removal.
- **The safety filter runs before the score, and the score has no term for safety.** Phase 12 wrote
  this for coverage guarantees and phase 23 for crop safety; here it is a property of the type
  system rather than an ordering in a function. `source::select` takes a `SafeCandidate`, which has
  no public constructor and comes only from `safety::check` returning `Allowed`. A later phase that
  finds itself scoring an unchecked region cannot obtain the argument.
- **An absent input is ignorance, not permission.** Phases 19 to 23 *gated* an operation down when
  an input was missing and the safe direction was less. Here the safe direction is **none**, and
  "gated to zero" and "blocked" would look identical in a panel while meaning completely different
  things: one says the product checked and found nothing to worry about, the other says it could not
  check. `CleanupCode::ProtectionUnknown` and `CleanupCode::OverlapsProtected` are separate codes,
  separate rows and separate runbooks, and only the second is a claim.
- **A cloud call that can only make the product do less has no unsafe failure mode.**
  `aura_generative::judgement::Answer` is `Decline | Stand | Unavailable` and there is no `Approve`.
  An unreachable provider, an invalid response, a spent budget and a cautious model all leave the
  photograph in the same state. That is the property phase 12's tie-breaker lacked, and it is why
  the same repository reaches opposite conclusions about two superficially similar features.
- **A disclosure is written in the same transaction as the removal and can never be edited.** Three
  triggers, plus `Recipe.cleanup[]`, because phase 14's rule is that a delivered file is
  re-creatable from four values and a removal in none of them cannot be audited. `CleanupMethod::
  Inpaint` in a stored row means a diffusion model ran; there is no build in which it means
  something else.

Six things phase 24 got wrong first, all found by its own tests, and three of them worth
generalising:

**A correlation is the wrong instrument for "is this the same object".** Normalised cross-correlation
is undefined over a flat window and returns zero by design - and a gaffer-taped cable, an exit sign
and a caterer's crate are all close to flat. So a burst neighbour containing the *identical* object
correlated at zero, read as "completely different", and the borrow replaced the exit sign with the
exit sign. It is a mean absolute difference now, scaled by the surrounding texture's own spread.

**Both removal modules feathered toward the object they were removing.** The seam feather ran inward
from the region's boundary, so blending `original * (1 - w) + replacement * w` over the region
carried the outermost samples of the replacement back toward the bin. The code that exists to hide a
seam left a rim of the distraction behind - phase 18's resampler defect in a different module.
`pixels::feather_out` is the fix: full weight over the whole object, falloff on the band outside it.

**Comparing two maxima compares two unrelated facts.** The repeated-texture check subtracted the
frame's strongest autocorrelation from the patch's, so a background with a slow twenty-pixel
undulation excused a patch repeating hard at four. Section 6.4 asks for "a period that occurs nowhere
else", which is a per-lag comparison. The same file also had phase 22's threshold trap for the third
time in the repository: a 99th percentile of adjacent-pixel steps is **zero** on a smooth frame,
because a 256-bucket histogram cannot resolve a step below 1/255, so a hard rectangle edge in a
studio backdrop scored as no artefact at all.

Phase 24 amended a frozen contract for the fourth time in the product's history, after phase 09's
`FaceRef`, phase 16's re-lock and phase 23's `Lens::coefficients`: `aura_recipe::Recipe` gained
`cleanup`. It also added the fifth grep-as-a-test,
`crates/aura-generative/tests/one_choke_point.rs`, which fails the build if a removal is reached from
anywhere but `source::select`, if the crate writes a recipe, if it reaches a provider - or if any
type in it grows a field a text prompt could go in.

Phase 26 is implemented conditionally: `aura-core::contract::camera` freezes the flash state, the
brand table, the appearance distance, the fingerprint, the transform, the matched pair, the shooter
bias, the reference, thirty-two reason codes, the outline, the override and `CameraMatchService`, and
`ids.rs` gains `PairId`; `aura-brain-gallery::camera` matches two bodies to one visual result.
`policy.rs` loads a table whose bounds the *code* owns, `baseline.rs` holds eight bundled brand
transforms and the refusal that keeps a fabricated one from claiming to be measured,
`fingerprint.rs` measures each body's colour response from the wedding's own frames in two flash
states, `pairs.rs` finds two cameras that photographed the same conditions and verifies them on
**backgrounds rather than subjects**, `solve.rs` runs a bounded coordinate descent with phase 15's
skin locus as a hard constraint and a deterministic held-out split, `transform.rs` owns the
appearance distance and derives the three per-channel gains in closed form, `shooter.rs` corrects an
exposure habit by strictly less than the whole of it, `report.rs` assembles the sentence a
photographer reads, `store.rs` owns migration 26 and `api.rs` is the frozen service and the pass.
Migration 26 stores five tables, two views and three triggers at 57 bytes a photograph; eleven IPC
commands (ADR-0054) feed a camera match panel; and `aura-cli verify --phase 26` is the executable
gate. Its exit report is `docs/progress/PHASE-26-EXIT.md`.

**This phase ships no model** - the sixth since phase 08 - and the reason is phase 17's, 23's and
25's: there is nothing to train. What is missing is not weights but **multi-camera weddings**.
Section 9's DATA row asks for Sony+Canon, Canon+Nikon and Fuji fixtures with matched scenes and there
are none, so every per-brand colour response in every fixture was authored and read back through the
real code. That is condition C1 and a Sev 2 trigger, and it closes with phase 05's C10 because the
pair finder's subject-similarity term reads the placeholder embedding. Condition C2 is the second Sev
2 and it is the one to be careful about: **all eight bundled baselines were fabricated**, every one
carries `measured = false`, and the fallback path is the *common* one - a wedding where the second
shooter worked a different room all afternoon has no matched pairs at all, and every correction on it
comes from that table. What is proved is that the path runs, reports itself honestly, and leaves an
unknown manufacturer alone. Condition C3 is that the skin term - **weighted 3.0, the heaviest of the
four** - is unmeasured, because phase 25's `SKIN_FIELD_AVAILABLE` is false. Condition C4 is the
headline: section 9's blind study did not happen, so "frames from different brands look like they
came from one camera" is unmeasured.

Four rules that phase 26 adds and every later phase inherits:

- **`CameraMatchService` is the only way to ask what a camera body does to colour.** Twenty-second
  service of its kind and the first whose subject is a **device**. Phase 27 reads a camera transform
  when it explains why a frame looks unlike its neighbours, 28 acts on one unattended and 30's
  delivery report lists which bodies were matched from evidence and which from a baseline. No phase
  may keep its own camera fingerprint or its own idea of what two bodies agreeing means.
- **Match appearance, never parameters.** Two bodies can solve to the same 5,200 K and render skin
  two dE00 apart, because a temperature answers "what light was this" and a rendering answers "what
  does this sensor do with it". Every term of the appearance distance measures a *frame*; nothing in
  it reads a recipe. Any later phase that finds itself comparing two cameras' slider values has
  re-created the problem this phase exists to solve.
- **A parameter that is not identifiable from the evidence is derived, not fitted.** A matched pair
  supplies a chromaticity, a white point, a grade signature and a contrast reading, and **none of them
  separates a hot red channel from a cold green one**. A least squares over ten parameters with an
  eight-dimensional observation converges, reports a small residual and returns whichever point its
  initial conditions were nearest. The three gains come from the two fingerprints in closed form and
  the descent runs over the seven that are identified. Phase 17 found the same defect from the other
  side - a fitted slope not identified where it was applied - and this is the general statement:
  **count the dimensions of the observation before fitting the vector.**
- **Correcting a machine completely is the feature; correcting a person completely is not.** A
  sensor's colour response is not a decision anybody made, so it is removed in full. A shooter's
  exposure is, so `SHOOTER_HARMONY` is strictly below one and there is no configuration that removes
  a habit entirely. Any later phase that normalises something a person chose inherits the asymmetry.

Two things phase 26 got wrong first:

**A storage note written before the measurement was wrong about the *shape*, not the size.** It said
the pair table grew with the square of a wedding's overlap; `pairs::find` truncates at
`MAX_PAIRS_PER_CAMERA`, so a two-body wedding stores 57,724 B at a thousand frames and 57,729 B at
two thousand. The test asserts the bound now as well as the number, by running the same pass over a
doubled wedding - a size assertion alone would pass on a build that had removed the cap and happened
to be measured on a small fixture. Phase 21's rule covers the sentence as much as the figure.

**A fixture that seeds a project but not its rows passes every unit test.** The gate's first run
failed on `camera_pair`'s foreign keys onto `photo`, and phase 25's gate had failed the same way on a
skin correction naming an identity that did not exist. Twice in two phases: a store test is handed
ids rather than making them, so nothing below the gate ever exercises a referential constraint.

Phase 25 is implemented conditionally: `aura-core::contract::gallery` freezes the scene node, the
node target, the normalisation delta, the per-identity skin target, the skin correction, the five
bounds, twenty-six reason codes, the outlier, the outline, the override and `GalleryService`, and
`ids.rs` gains `NodeId`; `aura-brain-gallery` matches a wedding to itself. `policy.rs` loads 23
argued-over scene rows whose bounds the *code* owns rather than the file, `tree.rs` builds a tree of
lighting groups from phase 07's chapters and sub-clusters the long ones on time, `changepoint.rs`
splits a node wherever the light genuinely changed - a step its own trend does not explain, or a
span no single target can cover - `stats.rs` is the robust arithmetic every target is built from,
`anchors.rs` ranks three to five reference frames by a **product** of four terms and honours a
photographer's pin over all of them, `normalise.rs` damps then bounds each frame's movement toward
its node, `skin_consistency.rs` builds each person's appearance from their own well-lit frames and
corrects toward it under a cap that falls with the mood of the room, `scene_consistency.rs`
harmonises contrast and character where a scene's variation is not the point, `outlier.rs` reports
what is left *after* the correction, `store.rs` owns migration 25 and `api.rs` is the frozen service
and the pass. Migration 25 stores five tables, two views and three triggers at 330 bytes a
photograph; nine IPC commands (ADR-0052) feed a consistency view, before-and-after timeline strips,
an anchor picker and an outlier list; and `aura-cli verify --phase 25` is the executable gate. Its
exit report is `docs/progress/PHASE-25-EXIT.md`.

**This phase ships no model** - the fifth since phase 08 - and the reason is phase 17's and phase
23's rather than phase 24's: there is nothing to train. Anchor selection is a ranking over numbers
other phases already produced, the solver has a closed form, the change-point detector is a
two-sample statistic and the outlier detector is a threshold. What is missing is not weights but
**weddings**: section 9's DATA row asks for labelled intentional lighting transitions and there are
none, so every gate is measured against synthetic galleries whose drift was authored and whose
transitions are known by construction. That is condition C1 and a Sev 2 trigger, and it closes with
phase 05's C10 rather than separately - the anchor ranking multiplies phase 15's white-balance
confidence by phase 06's identity prominence and both are placeholder-backed. Condition C2 is the
second Sev 2 and it is the one to be careful about: `SKIN_FIELD_AVAILABLE` is false, no photograph in
this build has an identity-scoped skin region, and section 6.3's promise about a person's skin ran on
**authored readings rather than on people**. Condition C3 is the headline gap: the perceptual audit
of five weddings did not happen, and the failure it exists to catch - a gallery that is more uniform
and less alive - would show as a *better* number on every gate in the phase.

Five rules that phase 25 adds and every later phase inherits:

- **`GalleryService` is the only way to ask what a wedding looks like as one body of work.**
  Twenty-first service of its kind and the first whose subject is a **set** of photographs. Phase 26
  matches a second camera into these nodes, 27 reads these outliers as its QC input, 28 acts on them
  unattended and 29 builds albums out of a gallery this phase has already made coherent. No phase may
  keep its own scene-node tree, its own anchor selection or its own idea of a consistent gallery.
- **A gallery decision is a residual, and the thing it is a residual from is immutable with respect
  to it.** `normalise::solve` reads phase 15's and phase 16's stored answers and never reads its own
  output, which is what makes running the pass twice a no-op. Idempotence here is not convergence and
  not a "have we run" flag; it is a property of where the delta is measured from. Any later phase
  that adds a layer on top of this one inherits the shape: read the layer below, never your own row.
- **Anchors, not averages - and a node that cannot be anchored normalises nothing.** An average over
  a chapter includes the chapter's mistakes at their true weight. `GalleryCode::NodeUnanchored` and
  `GalleryCode::AlreadyConsistent` both produce five zeroes and mean opposite things, and they are
  separate codes, separate rows and separate runbooks. Phase 24's rule - an absent input is
  ignorance, not permission - in the phase where the two are easiest to confuse.
- **A correction is damped, then bounded, and the order is the guarantee.** Bounding first would make
  the bound a *target*: every distant frame would land at `damping * bound` exactly and a gallery
  would grow a visible band of identically-corrected frames at the edge of every transition. And a
  clamped frame is **less** confident rather than more, because the clamp says the frame and the node
  disagree about what room they are in.
- **A promise about a person is measured per person, from their own frames, and stored.**
  `gallery_skin_target.spread_after` makes "the same person's skin varies by no more than 2.0 dE00
  across the gallery" a `SELECT MAX(...)` rather than a sentence. There is no ideal-skin constant in
  the contract, the config, the migration or the code; the phase gate scans the schema for one on
  every run and `tests/no_recipe_writes.rs` scans the source. Phase 15 wrote this rule and this is
  its second application at gallery scale.

Two things phase 25 got wrong first, both worth generalising:

**A statistic with a trend in it splits the drift it exists to normalise.** The obvious change-point
test divides the difference between two runs' robust means by the spread *within* the runs. On a
flash that works. On a slow drift it also fires, because a chapter that warms 500 K over forty frames
has a tiny frame-to-frame spread and a large difference between its halves - which is the definition
of drift, and drift is what the whole phase exists to remove. The first implementation cut a
forty-frame ceremony into six unanchorable nodes and reported six lighting changes, with every unit
test passing. The divisor is the **trend** now. Phase 22's rule in its second half: a threshold on a
measurement is a statement about the instrument as well as about the world, and the instrument had a
slope in it. The first fix was itself half wrong - it divided by the *shorter* run's length rather
than by the distance between the two runs' midpoints, which scored a smooth ramp at six.

**A reduction gate is only meetable while the thing being reduced is inside the bound.** Section
10.1 asks for the exposure spread to halve, and a within-node drift of a full stop cannot halve when
the bound is 0.35 EV - fifty-three per cent of the frames clamp, and the arithmetic does not care
what the gate says. That is not a solver failure and not a threshold to lower: it is a *fixture* that
authored a lighting change and called it drift. The gate measures a realistic third of a stop now,
and a second test asserts that a wider drift is **reported as outliers** rather than silently
half-corrected. Same family as phase 19's edge-gradient halo test, phase 21's chance-corrected margin
and phase 22's sharpening kernel floor - and this is the fourth time, which is enough to state it
generally: **when a gate cannot be met, work out whether the fixture, the threshold or the code is
the thing that does not match reality, and fix that one.**

Phase 25 also added the sixth grep-as-a-test,
`crates/aura-brain-gallery/tests/no_recipe_writes.rs`, which fails the build if this crate writes a
recipe, opens a file, reaches a provider, grows its own tone solver, or acquires a constant it could
compare a person's skin against.

Phase 27 is implemented conditionally: `aura-core::contract::qc` freezes the ten inspections, 43
codes, the reason, the evidence, the five remedies, the six ticket statuses, the ticket, the round,
the replacement, the report, the outline, the override and `QcService`, and `ids.rs` gains
`TicketId`; `aura-qc` judges what every phase before it decided. `policy.rs` loads a thresholds
table whose nineteen ceilings the *code* owns and a studio may only tighten, `checks/` holds ten
pure inspections that each return `Clean`, `Found` or **`Skipped`**, `ticket.rs` gives every finding
a code, a number, a threshold, a reason and an autonomy band, `triage.rs` works root causes before
their symptoms and asks for a second opinion on a frame with several at once, `remedy.rs` is the one
choke point a remedy can be built through, `reedit.rs` applies one, re-inspects, and keeps the
change only if it delivered half of what it promised and broke nothing else, `replace.rs` swaps a
frame through four gates of which the coverage guarantee is a filter rather than a term,
`planner.rs` is a bounded reasoning-tier call whose output type cannot become a remedy, `report.rs`
leads with what was checked, `store.rs` owns migration 27 and `api.rs` is the frozen service and the
resumable pass. Migration 27 stores four tables, two views and four triggers at 421 bytes a
photograph; 23 argued-over scene rows live in editable config; nine IPC commands (ADR-0056) feed a
report, category chips, a grouped queue and a before-and-after; and `aura-cli verify --phase 27` is
the executable gate. Its exit report is `docs/progress/PHASE-27-EXIT.md`.

**This phase ships no model, and for the first time that is a decision about the *feature* rather
than about the data.** Phases 17, 23 and 25 shipped none because there was nothing to train; phase
24 because there was no data. Here `DETECTOR_TRAINED` is false because a QC agent that guesses at
defects is worse than one that measures them: every check is a comparison between numbers phases 08
to 26 already stored, whose failure mode is finding fewer problems rather than inventing them, and a
false ticket is the one failure that makes a photographer close the queue and stop reading it.
ADR-0055 section 3. Every gate in section 10.1 is measured against galleries whose **readings** this
repository authored - not photographs - so what is proved is the arithmetic, the triage, the loop's
bounds, the refusals and the store. That is condition C1, it is a Sev 2 trigger, and it closes with
phase 05's C10. Condition C2 is the second Sev 2 and it is the headline: **the photographer-agreement
study did not happen**, so the false-ticket rate is measured against two hundred frames this
repository authored as clean, which is a test of the thresholds against themselves.

Five rules that phase 27 adds and every later phase inherits:

- **`QcService` is the only way to ask whether the product's own work is any good.** Twenty-third
  service of its kind and the first whose subject is a **problem**. Phase 28 reads these tickets to
  decide what may run unattended, phase 29 must not build an album out of frames this phase flagged,
  and phase 30's learning loop reads the dismissals. No phase may keep its own idea of a defect, its
  own thresholds table, or its own re-edit loop.
- **Clean and skipped are different values, and a phase that collapses them is dangerous rather than
  imprecise.** Every earlier phase reported coverage as a number beside a result; here the number
  *is* the result. A gallery whose masks are absent reporting zero mask artefacts reads as a clean
  bill of health, and on this build that is the common case rather than the exotic one. `Outcome`
  has three variants, `QcOutline::inspection_completeness` is on the wire, and the panel renders a
  category that found nothing and skipped everything in grey. Phase 24's rule - an absent input is
  ignorance, not permission - in the phase where the two are most expensive to confuse.
- **Improvement is measured against what the finding opened with, never against the threshold.**
  Phase 19's lesson, applied to the loop that would most easily have shipped it: a remedy that
  closed 90 % of the gap and landed just outside is a remedy that worked, and one that crossed
  because the threshold moved is not. `QcRound` stores both deviations and the expected gain, and
  `realised_share` is what the loop, the panel and the archived report all decide on.
- **A ticket's sentence is rendered, never stored.** Phase 09's rule at its conclusion: there is no
  `diagnosis` column in migration 27 and no free-text field automation can write into, so a stored
  sentence cannot become copy a release has to maintain, a catalog full of English, or a place a
  cloud answer gets quoted back as a measurement. The gate scans the schema for one on every run.
- **Agreeing is not authorising, and the two are separate shapes on the wire.**
  `QcDecideBulkInput` has no `applyRemedy` field. Agreeing that forty findings are real is a
  statement about the findings; instructing AURA to act on forty frames unattended is a statement
  about the remedies, and they are different judgements made with different amounts of attention.
  Any later phase that adds a bulk action which changes photographs has re-created the failure this
  rule exists to prevent.

Phase 27 amended a frozen contract for the fifth time in the product's history, after phase 09's
`FaceRef`, phase 16's re-lock, phase 23's `Lens::coefficients` and phase 24's `Recipe::cleanup`:
`TicketStatus` gained `Dismissed`. A finding a photographer disagreed with must not come back on the
next pass, and no existing status could express it. ADR-0055 section 9.

Three things phase 27 got wrong first, all found by its own gates, all worth generalising:

**A predicate named for one question was reused for a second one it answers wrongly.**
`TicketStatus::is_open()` is true for `Open`, `Escalated` and `Reverted`, and both the triage and
the retry check read it - so a finding already handed to a person was remediated again and consumed
the second attempt the bound exists to protect. Every unit test passed, because each exercised a
single round. "Is this finding outstanding" and "may automation still act on it" are not the same
question, and a predicate that answers the first must not be spent on the second.

**A check that reads documentation as if it were code fails hardest on the codebases that document
themselves best.** Twice in one phase: a grep asserting the skin module holds no fixed skin target
matched its own test name, and the gate's schema scan matched migration 27's four paragraphs about
why there is no `diagnosis` column - `sqlite_master.sql` stores a migration verbatim, comments
included. Both strip comments before scanning now, which is what `tests/no_pixel_ops.rs` already
did.

**A reading that cannot be honestly filled must be an `Option`, and the third time is the rule.**
`NodeReading::frame_signature`, then `ExposureReading::subject_luma` and
`ExposureReading::shadow_headroom`: nothing in the product stores the luminance a finished frame
landed on - phase 15 stores the band it solved *toward* and phase 25 the move it still *owes* - and
no frozen contract carries a finished frame's remaining shadow room. A proxy in any of the three
would have reported every frame as sitting exactly on its target, on every frame nobody measured.

Phase 27 also added the seventh grep-as-a-test, `crates/aura-qc/tests/no_pixel_ops.rs`, and closed a
gap that belonged to no phase: **no gate anywhere checked that the IPC surface was reachable.** Phase
21's exit report found ninety client calls reaching a window that did not answer to them, fixed them
by hand, and left nothing behind. Section 11 of the phase 27 gate reads the three files that have to
agree - the `#[tauri::command]` definitions, the `generate_handler!` list, and the typed client's own
string literals - and compares them. It proves the names and the syntax and **not the types**,
because the shell's Rust does not compile on this machine, and it currently reports 220 = 220 = 220.

Phase 28 is implemented conditionally: `aura-jobs::contract::autopilot` freezes the twenty-five
stages and their declarations, the run and its five statuses, the eight skip causes, the four stage
verdicts, the four governor actions, the machine state, the eight pre-flight checks, the progress
watch, the handle, the summary, the outline, the override and `AutopilotService`, and `ids.rs` gains
`RunId`; `aura-jobs` runs a wedding. `dag.rs` builds one deterministic order from a compile-time
table and refuses a cycle, `policy.rs` loads a checklist whose five bounds the *code* owns and a
studio may only tighten, `checkpoint.rs` keys a stage's completion on a hash of what it read,
`resume.rs` decides what a second life repeats, `governor.rs` folds seven readings with `max` and
has no action that makes the product do more, `preflight.rs` answers eight questions before a
two-hour job and blocks on four of them, `retry.rs` tries an optional stage three times and then
isolates it, `summary.rs` says what did not happen and why, `store.rs` owns migration 28 and
`api.rs` is the frozen service and the run. Migration 28 stores four tables plus settings, two views
and three triggers at 1,760 bytes a thousand photographs; 25 argued-over checklist rows live in
editable config; nine IPC commands (ADR-0058) feed a checklist, a progress panel, a pre-flight
dialog and a run summary; and `aura-cli verify --phase 28` is the executable gate. Its exit report
is `docs/progress/PHASE-28-EXIT.md`.

**This phase ships no model** - the eighth since phase 08 - and the reason is none of the three the
earlier seven gave. There is nothing here a model could do: the orchestrator's whole job is to
decide what runs next, and that is a topological order over a compile-time table. A phase that
trained something to schedule would be a phase that had given a scheduler an opinion. What it also
ships no measurement of is a **wedding**: section 11's four wall-clock rows are waived because this
machine has no GPU backend, no trained model and no camera file, so every stage in every gate is a
`ScriptedRunner` and what is measured is the *scheduler*. That is condition C1 and a Sev 2 trigger.
Condition C6 is the second Sev 2 and it is the headline: **the intervention rate is unmeasured**, so
no claim about how much work this product saves may be made from this build. Condition C7 is the one
to say out loud: **this build writes no files**, because phases 29 and 30 do not exist, so a
completed run leaves a chosen and edited gallery in the catalog and nothing on disk.

Five rules that phase 28 adds and every later phase inherits:

- **`AutopilotService` is the only way to ask what the product did to a whole wedding.**
  Twenty-fourth service of its kind and the first whose subject is a **run**. Phase 29 adds a
  curation stage to this DAG, phase 30 adds an export stage and reads these summaries as its
  learning signal. No phase may keep its own pipeline runner, its own checkpoint format or its own
  idea of what a finished wedding is.
- **A scheduler decides nothing, and the manifest is only the first lock.** `aura-jobs` depends on
  none of the twenty-two deciding crates, and `crates/aura-jobs/tests/no_decisions.rs` - the eighth
  grep-as-a-test - fails the build if any type in it grows a field that could hold a keep, a
  rejection, a strength, a threshold or a confidence about a photograph. Two locks because the
  manifest catches a dependency and the grep catches the version where somebody adds the dependency
  and the call in one commit. The corollary is the shape of every stage arm: **one call into the
  command that phase already ships**, so the autopilot runs a wedding through exactly the code path
  a photographer clicking each panel's button would.
- **Every resource action makes the product do less, and only one reading stops a run.**
  `GovernorAction` has no variant that raises concurrency, enlarges a batch or disables a check, so
  a sensor that is broken, absent or lying cannot cause anything worse than the run going at the
  speed it would have gone at anyway. A full disk is the only pressure that does not clear on its
  own, so it is the only one that stops. ADR-0050 gave phase 24's cloud judgement this property;
  this is the same shape applied to hardware, and it is why the governor is safe on a machine that
  exposes no telemetry at all - which is this one.
- **An optional stage that fails does not fail the wedding, and the summary says what did not
  happen.** Three attempts, then isolation, then `CompletedDegraded` with the stage named and a
  sentence. Only ingest, previews, embed and cull can end a run. `degraded_stages` is a list rather
  than a count, because the count is not what a photographer needs at one in the morning.
- **A checkpoint is keyed by what a stage read, and a delivered run's record cannot be rewritten.**
  Keying on anything else resumes onto stale work silently. `RunStatus::is_resumable` continues a
  stopped run and mints a new one for a delivered wedding, and `autopilot_run_no_reopen` enforces
  the second half in the database - because a correction is a new run rather than an edit to what a
  photographer was told happened to their wedding.

Four things phase 28 got wrong first, all worth generalising:

**A terminal status is not the same question as an unfinished one.** Treating every terminal state
as final forced a resume to mint a new run id, and because checkpoints are keyed `(run_id, stage)`
that found no checkpoints and repeated every finished stage - two hours of a photographer's evening,
lost to a bookkeeping rule rather than to a bug. Phase 27 hit the same shape from the other side,
where `TicketStatus::is_open()` answered "is this outstanding" and was spent on "may automation
still act". **One predicate, one question.**

**A value that has already absorbed the thing you are testing for cannot test for it.** `plan_stage`
wrote the freshly computed `inputs_hash` before the comparison read it, so every checkpoint always
matched and no stage was ever re-run, with every unit test passing because each exercised one life.
Phase 19's converged-target defect in a third place.

**A budget written before it was measured was wrong about the shape as well as the size.** The
storage note quoted a per-table breakdown nothing had measured; `dbstat` reports 5,281 B, and the
number does not grow with the wedding at all - the first migration since phase 01 with that shape.
The bound is asserted as well as the number now, by running the same orchestrator over ten times the
units. Phase 21 wrote this rule and phase 26 wrote its second half.

**A gate that reads a wall clock on a fixture measures the fixture.** The first phase gate printed
the run's elapsed time beside section 11's budget, which on a `ScriptedRunner` is a number that looks
like a wall clock and is a measurement of the test harness. Four of five rows are waived now and the
gate prints the seven conditions it did **not** prove on every run, rather than leaving them in a
document nobody opens.

Phase 28 also closed a lock gap of its own making before it could become one: **migration 28 was not
in `EXTRA_CONTRACTS`**, which `docs/plan/CLAUDE.md` has required of every migration since phase 01
and which phase 16 found missing for migration 15 the same way. `contracts.lock` now carries 76
entries. And it **mounted its own panel**, which the develop panels from phase 12 onward still do
not have: `ui/src/App.tsx` renders `AutopilotPanel` beside the gallery and QC panels, and the
container-plus-pure-views split phase 25 established is what made the five views testable without a
window.

Phase 29 is implemented conditionally: `aura-core::contract::curate` freezes the eight-band monochrome
mix, the four suitability terms, the hero pick with its five terms and its binding constraint, the
spread and its four pairing measurements, the album plan with its chapter map and its own coverage
report, the three social sets, the teaser, the caption and its source, 39 reason codes in five groups,
the outline, the override, twelve export formats and `CurateService`, and `ids.rs` gains `SpreadId`;
`aura-curate` proposes. `policy.rs` loads a table whose five bounds the *code* owns and whose keys are
scanned for anything naming a skin target, `read.rs` is the one port through which this crate learns
anything and every reading on it is an `Option`, `bw.rs` reads eight bands out of phase 05's histogram
and solves a mix that spreads the **collapsed** set against itself rather than away from the mean,
`hero.rs` blends five terms under a technical veto and records which of three diversity constraints was
binding, `album.rs` filters for coverage *before* it scores, apportions chapters by largest remainder
and lays out spreads with a bounded look-ahead, `spread.rs` refuses a pair rather than penalising it,
`caption.rs` builds a closed vocabulary out of this wedding's own labels, `sequence.rs` is a cloud task
whose every move the local objective may refuse, `export.rs` writes twelve specifications by hand,
`store.rs` owns migration 29 and `api.rs` is the frozen service and the resumable pass. Migration 29
stores eleven tables, two views and five triggers at 211 bytes a selected image; eleven IPC commands
(ADR-0060) feed a hero grid, a monochrome panel, a spread view, social sets and an album builder; and
`aura-cli verify --phase 29` is the executable gate. Its exit report is
`docs/progress/PHASE-29-EXIT.md`.

**This phase ships no model** - the ninth since phase 08 - and the reason is phase 17's, 23's and 25's
rather than phase 24's: there is nothing to train until there is data. Section 9's DATA row asks for
sixty consented album sequences, hero sets and monochrome selections and there are none, so
`HERO_HEAD_TRAINED` and `BW_HEAD_TRAINED` are false and both are on the wire. That is condition C1 and
a Sev 2 trigger. Condition C2 is the second and it is the one to be careful about: **the skin rule is
unreachable on this build** - phase 06's detector finds no faces, so phase 15 measures no locus, so
every mix is solved as a faceless frame, and no claim may be made that this build protects anybody's
skin in a monochrome conversion. C3 is the same absence reaching the facing term. C4 is the headline:
**the three studies of section 10.1 are unmeasured** - hero agreement, album reordering, monochrome
acceptance - and all three need photographers.

Five rules that phase 29 adds and every later phase inherits:

- **`CurateService` is the only way to ask what a photographer should show.** Twenty-fifth service of
  its kind and the first whose subject is an **audience**. Phase 30 exports these albums, posts these
  social sets and reads these overrides as its learning signal. Two answers to "which twenty
  photographs are the portfolio" is a website that does not match the album that does not match the
  post.
- **A proposal is never an application, and here that needed a grep.** Nothing in this phase writes a
  recipe. The monochrome suggestion is why the rule needed enforcing rather than stating: the `bw`
  block is two fields, `schema::merge` is one call away, and the result would be beautiful - and would
  be the product deciding a wedding is monochrome. `aura-render` and `aura-recipe` are
  dev-dependencies only, so the eval can measure a mix on the greys it actually produces while the
  library cannot reach a renderer at all.
- **Chapter order is inviolable, and a photographer's own order outranks the composer's.** A drag that
  crosses a chapter is refused rather than accepted quietly; an order somebody set is never
  overwritten. `album_order` is a separate table from `album_spread` for exactly that reason - a pass
  that stored the order in the spreads would lose it the moment it rebuilt them, which is what a
  photographer does after adding two hundred frames.
- **The cloud can only be agreed with.** `AlbumSequencing` returns moves and captions; the local
  objective accepts or refuses each move, and a chapter-crossing one is refused rather than nudged
  back inside. An unreachable provider, a spent budget, a malformed response and a cautious model all
  produce the same album. Phase 24's shape, second application.
- **A caption is assembled from words this wedding supplied, and the check is the same for a template
  and for a model.** The local template passes by construction, which is what makes the grounding
  check meaningful rather than decorative. No name, no venue, no relationship, no claim about how
  anybody felt - and no gendered role word, because which of two people is the bride is not a
  photographic fact.

Five things phase 29 got wrong first, and the first of them is the one to generalise:

**A fixture that minted random identifiers looked deterministic for the whole of this phase's
development.** `ImageId::new()` is a v7 UUID - time-ordered in its high bits and random in its low
ones - so two runs of one seed agreed about every score and disagreed about every identifier, and
every tie-break in the crate falls back on `image_id`. A gate moved fifteen points between two runs of
an unchanged build. `the_same_seed_produces_the_same_wedding` had been passing throughout, because it
compared scores and chapters rather than ids. **A determinism test that does not compare the
identifiers is not a determinism test.**

**Three attempts at an arithmetic proxy for a human judgement, all wrong and all plausible.** "Is this
mix better than a fixed preset" cannot be answered by a statistic: one that rewards how far a mix
moves the tones is won by the preset, one that rewards restraint is won by the solver, and one that
takes the worst pair of seven bands is a lottery. The album's reordering row went the same way through
three successive distance bounds, each either loose enough to prove nothing or tight enough to fail a
correct build. What both rows carry now is the *fact* underneath the judgement, with the judgement
named as unmeasured. **When a gate cannot be met honestly, the answer is sometimes that the gate is a
study** - which is the fifth member of the family phases 19, 21, 22 and 25 started, and the first where
the right fix was to stop asserting.

**Three fixture defects in one file, all found by the gates.** Every frame was one hue, which gives a
monochrome solver nothing to separate; every frame's luminance was drawn across the whole range, which
models a gallery nobody normalised and which no curation pass ever sees because phase 25 runs first;
and similarity was a hash of two ids, which makes distinctness independent of everything else about a
photograph and turns eighteen per cent of the portfolio blend into a coin toss. Phase 25's lesson three
more times.

**A ground truth an algorithm cannot reach is a ground truth, not a finding.** `fixtures::planted`
spread its twenty picks at a fixed stride, which follows chapter length, which filled three chapters'
hero quotas exactly - so one ordinary frame winning one round cost a plant permanently and the gate
measured ties.

**A budget written before it was measured, wrong by a factor of ten and in the wrong direction.** The
store note said 2,143 B an image; it measures 211 B at 600 frames and 2,439 B at the smallest gallery,
because the album, the portfolio and the captions are capped by the contract and the gallery is not -
so the figure *falls* as a wedding grows, which is the opposite of every migration from 09 to 20. The
budget is set at the small end now and the bound is asserted as well as the number. Phase 21 wrote this
rule and phase 26 wrote its second half; this is the first time the shape was inverted rather than
mis-sized.

Phase 29 also **half closes phase 28's condition C7**: `AppRunner::availability` no longer reports
`SkipCause::PhaseNotBuilt` for curation, and the stage's arm is one call into `curate_project` - the
shape phase 28's own rule asks for. Export is still unbuilt, so a completed run leaves a curated
wedding in the catalog and nothing on disk, and phase 28's gate prints C7 saying exactly that.

Phase 30 is implemented conditionally, and it is the last phase of the plan.
`aura-core::contract::delivery` freezes the three formats, the three colour spaces, the resize, the
output sharpening, the seven naming tokens and their template, the metadata policy, the destination,
the set, the job, the written file, the sealed manifest, the upload state machine, thirty
`DeliveryCode`s of which three stop a job, `ExportService` and `DeliveryService`;
`aura-core::contract::learn` freezes `Learnable` - **closed at fifteen members with no `Other`** - the
correction, its context, its bucket, the aggregate, the held-out split, the update, the A/B
comparison, consent, twenty `LearnCode`s and `LearnService`. `aura-export` writes: `naming.rs` plans
every name before a byte exists and refuses a template that could name a folder, `resample.rs`
downscales in linear light and sharpens on encoded samples after it, `icc.rs` synthesises v4
matrix/TRC profiles with the creation date zeroed, `metadata.rs` **builds** a block rather than
copying one forward, `jpeg.rs`/`tiff.rs`/`png.rs` are the three writers, `verify.rs` writes, flushes,
`sync_all`s, re-opens, re-reads and hashes, `manifest.rs` seals a record that can never be edited and
`store.rs` owns the export half of migration 30. `aura-delivery` sends: `backup.rs` is matched,
missing and **diverged**, `providers/` is the `Transport` port with a folder and a scripted
implementation, `mapping.rs` is per-set, `resume.rs` takes its offset from the far end in 4 MiB
chunks under a three-attempt bound. `aura-learn` learns: `attribute.rs` puts a correction beside the
decision it corrected, `aggregate.rs` splits deterministically by the correction's own id and trims
with a MAD that falls back on the mean absolute deviation, `update.rs` bounds the step at half of
what was asked and measures the result on corrections the fit never saw, `review.rs` offers,
`rollback.rs` restores exactly. Migration 30 stores 13 tables, 3 views and 5 triggers at 566 to 594
bytes a delivered file; six presets live in editable config; seventeen IPC commands (ADR-0062) feed a
delivery panel; the Lightroom and Photoshop plugins and the whole release machinery ship in
`plugins/` and `ops/`; and `aura-cli verify --phase 30` is the executable gate. Its exit report is
`docs/progress/PHASE-30-EXIT.md`.

**This phase ships no model** - the tenth since phase 08, and for a fourth distinct reason. It is not
"nothing to train" (17, 23, 25, 29), not "no data" (24) and not "a model would be worse than a
measurement" (27, 28): there is nothing here a model would be *for*. An export is arithmetic and file
I/O, a digest is a digest, and the learning loop is a trimmed median over rows the ledger already
holds. **No socket ships either**, which is the condition to be careful about: `check-banned.sh`
forbids one outside `aura-cloud` and a client-gallery provider is not a model provider, so everything
above the socket was built and the socket was not. `NETWORK_TRANSPORT_AVAILABLE` is false and is on
the wire. That is condition C3 and a Sev 2 trigger. C1 is that every pixel in every delivered file
came from a placeholder - the guarantee this phase makes is about the *bytes* and not about the
picture - C2 is the waived export budget, C4 is that no profile has been fitted from a real
photographer's corrections so the 15 % style-match improvement is unmeasured, C5 is that nothing has
been signed, notarised or rolled out, and C6 is that neither plugin has met the application it is
for.

Five rules that phase 30 adds, and they are the product's rather than the next phase's:

- **`ExportService` and `DeliveryService` are the only ways to turn a decision into a file.**
  Twenty-sixth and twenty-seventh services of their kind, and the first whose subject is a **thing on
  somebody else's disk**. Every store from phase 05 to 29 caches a decision that can be recomputed;
  these record a JPEG on an external drive, and re-running does not re-derive it - it writes it
  again, which is a different operation with a different cost and a different risk.
- **A file is verified by reading it back, never by hashing the buffer.** The failure this catches is
  silent: a loose reader, a NAS that dropped a packet, a drive on the way out - all of them give you
  a folder that looks right in a file browser and is wrong in the middle. A file that does not read
  back the same **stops the job**, because a destination that corrupted one file is not one to send
  three thousand more to. It costs 2 % of an export, measured in release.
- **A guarantee is not learnable, and the enforcement is a closed vocabulary.** `Learnable` has
  fifteen members and no `Other`, so the texture floor, the skin protection, the identity constraint,
  the crop safety filter and the naturalness guard cannot be named by a correction, aggregated into a
  bucket, or moved by an update. `crates/aura-learn/tests/no_guarantee_learning.rs` is the tenth
  grep-as-a-test. An open vocabulary is one where the next feature adds "retouch texture floor" and
  the guarantee erodes one correction at a time.
- **A record of what was delivered cannot be edited.** `delivery_manifest_no_update` aborts every
  UPDATE and a correction is a new job. Phase 28 gave a run's record this property and this is the
  same argument about the thing a client actually received.
- **Automation never chooses a destination.** The autopilot repeats the export a wedding was already
  given and skips with `SkipCause::NoInput` when there is none; there is no default folder and no
  field on the wire that could set one. A run that invented a destination would be the scheduler
  making a decision, which is what `crates/aura-jobs/tests/no_decisions.rs` exists to prevent.

Phase 30 **closes phase 28's condition C7**. `AppRunner::availability` is empty for the first time
since phase 28 wrote it: every stage in the DAG is built and a completed run writes files.
`crates/aura-jobs/src/stages/deliver.rs` predicted exactly that - "phases 29 and 30 change two
`availability` answers in `aura-app` and change nothing here" - and the prediction held.

Four things phase 30 got wrong first, and the first is the one to generalise:

**A ratio measured as a difference of two large numbers measured nothing.** The verification overhead
was first measured by running the same export twice, with and without the read-back, and subtracting.
The two whole-run timings came out within a third of a per cent of each other and the *verified* run
was faster - an overhead of zero, passing an 8 % budget, for no reason at all. It is measured
directly now. Phase 19's halo test and phase 22's ringing measurement are the same defect, and this
is the third time: **a difference of two large numbers is the wrong instrument for a small one.**

**A budget asserted in a debug build asserts the wrong thing.** The same ratio is *flattered* by a
debug build, because the encoder is several times slower than it ships and the denominator inflates.
Release-only now, printing the number with a note in debug.

**A performance fixture must be a shape the product would actually produce.** The learning budget's
first fixture proposed 45 changes and `LearningUpdate::validate` refuses anything over
`MAX_DIFF_LINES` - 24, because that is what a photographer reads before agreeing. The fixture folds
45 buckets and moves 24 now, which is the largest legitimate fit.

**Ten of fifteen learnables were unattributable, silently.** `DecisionKind` has six members and phase
13's reason registry carried a vocabulary for one, so a correction to an `Edit` decision could never
be recorded - `AURA-ML-5054` refuses a code that is not in the shipped registry, and every unit test
passed because each exercised the kind that worked. The registry now covers `ToneCode`, `ColourCode`,
`CurateCode` and `QcCode`, and `docs/reason-codes.md` is regenerated at 227 codes.

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

Three rules the post-review work adds, and they are the repository's rather than any phase's:

- **A component with passing tests is not a mounted component.** The review counted forty-two
  finished, tested, command-backed UI files that no import path reached from the entry point.
  Every one of them looked done from inside its own test file. `ui/src/reachability.test.ts` is
  the guard, and the general form of the rule is that *proving a part works is not proving the
  product has it* - which applies to a gate nothing runs and a crate nothing compiles just as
  much as to a panel nothing mounts.
- **A guard written in a doc comment is not a guard.** `Calibrator::fit_isotonic` had documented
  since phase 13 that it returns the identity map when there is nothing to fit; it checked the
  *outcome count* and not the *fitted map*, so a degenerate fit produced one constant confidence
  for every decision in the product. It was found by writing a test that asserted what the
  comment said. The two lessons compound: the crate had no unit tests, which is why a doc comment
  was the only place the rule lived.
- **A check that only runs inside one phase's gate is a check that stops running.** Phase 30
  lifted the IPC parity check out of phase 27's gate for this reason and the review found sixteen
  gates that had gone the other way. Anything that has to hold across the product belongs in
  `scripts/`, in CI and in `ops/release/release.toml` - all three.
