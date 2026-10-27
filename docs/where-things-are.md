# Where things are

Moved from `CLAUDE.md` on 2026-10-26 so it is no longer loaded into every session. One row per
concern: the decision record, the code, the configuration, the evaluation gates and the
product-voice document for each feature.

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
| Provider catalogue, first-run setup and TLS | `docs/adr/ADR-0066-tls-and-the-provider-catalogue.md` |
| The nineteen AI providers, as data | `crates/aura-cloud/src/catalog.rs` |
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
| Reference-look decisions | `docs/adr/ADR-0063-reference-look-matching.md` |
| What matching a look does, in the product's own words | `docs/match-a-look.md` |
| Look evaluation gates | `tests/eval/look_eval.rs` |
| Edit profiles (the start screen's looks, validated JSON) | `crates/aura-app/config/edit_profiles.json` |
| How edit profiles adapt, and how learned ones are measured | `docs/edit-profiles.md` |
| A reference look learned from its own original found in the collection, on the start screen, in collection preparation and in the Match a look panel | `docs/adr/ADR-0161-learn-a-reference-look-from-its-original-in-the-collection.md`, `crates/aura-app/src/color_match/original.rs`, `crates/aura-app/src/color_match/learned.rs`, `learn_original` in `crates/aura-app/src/reference_style.rs`, `crates/aura-app/tests/reference_original_panel.rs`, `crates/aura-app/tests/reference_original_discovery.rs` (ignored, `AURA_ORIGINAL_SHOOT`) |
| The tone of a look estimated against ordinary photographs (scene prior); the known-look harness | `docs/adr/ADR-0180-scene-prior-tone-for-match-a-look.md`, `crates/aura-app/src/color_match/transfer/tone_prior.rs`, `crates/aura-app/src/color_match/transfer/tests/looks.rs` (ignored, `AURA_LOOK_PHOTOS`, `AURA_LOOK_METHODS`, `AURA_LOOK_FLAT`) |
| Fixes for the deep Match a look assessment: unrelated scenes matched as looks, the transform stretch limit, the perceptual smoothness guard at every lightness, refusal-owned progress; the 20-photo desktop test with visible-change, banding and learned-original measurements | `docs/adr/ADR-0181-match-a-look-assessment-fixes.md`, `stretch`/`limit_stretch` in `crates/aura-app/src/color_match/transfer.rs`, `crates/aura-app/src/color_match/contours.rs`, `crates/aura-app/src/color_match/gallery_fit/adaptive.rs`, `write` in `ui/src/components/develop/PhotoStudio.tsx`, refusals shown as written (`ui/src/components/look/refusal.ts`), `scripts/test-main-match-twenty.py` (`--workflows ... learned`) |
| Exact colours of the reference (the photo's colour distribution moved onto the reference's, skin to skin, written as a stored grade); the band guard's near-black rule; Apply saving the strongest passing share; the reentrant image-work gate | `docs/adr/ADR-0182-exact-colours-of-the-reference.md`, `crates/aura-app/src/color_match/exact.rs`, `crates/aura-app/src/color_match/transfer/exact_grade.rs`, `dark_neighbours` in `crates/aura-app/src/color_match/contours.rs`, `BACKOFF` in `crates/aura-app/src/color_match/matcher.rs`, `crates/aura-render/src/work_gate.rs`, `scripts/test-main-match-twenty.py` (`--workflows ... exact`) |
| One-click AI Color Match (ADR-0170): superseded before merging by the Color Match panel and the Full look; the record and its 2026-10-24 measurements | `docs/adr/ADR-0170-one-click-color-match.md`, `docs/reports/AURA-one-click-color-match-2026-10-24.md`, `ui/src/components/look/ColorMatchPanel.tsx` |
| Lightroom panel parity decisions | `docs/adr/ADR-0070-lightroom-panel-parity.md` |
| Every Lightroom feature and where it lives in AURA | `docs/lightroom-parity.md` |
| Learning a profile from RAW before/after pairs | `ml/edit-profiles/fetch_fivek_pairs.py` + `crates/aura-app/tests/profile_fit.rs` |
| One-click intelligent editing and step history | `docs/adr/ADR-0081-intelligent-auto-edit-and-step-history.md`, `crates/aura-app/src/smart_edit.rs`, `crates/aura-app/src/portrait_features.rs` |
| What Auto enhance does, in the product's own words | `docs/automatic-editing.md` |
| Retouch strength sliders after Auto retouch (one factor per kind of retouch, applied by the renderer, synced across a collection) | `docs/adr/ADR-0130-retouch-strength-after-the-automatic-pass.md`, `crates/aura-recipe/src/retouch_intensity.rs` (`effective`), `crates/aura-app/src/retouch_intensity.rs`, `ui/src/components/develop/RetouchIntensity.tsx`, `docs/retouch-strength.md` |
| Twenty-eight selected effects: liquify, spin/privacy blur, pixelate, soft focus, Orton, local sharpen and denoise, hue shift, invert, chroma key, veins, colour unify, local white balance, light leak, glitch, haze, sun rays, halation, catchlight, eye glow, lip gloss, freckles, floor reflection, drop shadow | `docs/adr/ADR-0131-selected-effects.md`, `crates/aura-render/src/retouch_effects.rs` (+ `retouch_effects/`), `ui/src/ipc/retouchEffects.ts`, `ui/src/components/develop/EffectControls.tsx`, `docs/selected-effects.md`, `scripts/test-effects-gui.py` |
| Every Evoto effect module against AURA | `docs/evoto-module-map.md` |
| Twenty practical repairs and creative looks: dust and scratches, JPEG cleanup, deband, auto levels, colour cast, even lighting, HDR detail, bokeh, tilt-shift, duotone, posterize, halftone, sketch, oil paint, cross-process, infrared, mirror, lens flare, god rays, weather | `docs/adr/ADR-0132-practical-repairs-and-creative-looks.md`, `crates/aura-render/src/retouch_effects/practical.rs`, `crates/aura-render/src/retouch_effects/creative.rs` |
| Every tool and feature exercised in the real window, with a report | `scripts/test-everything-gui.py` (writes `.work-checks/everything/report.md`) |
| Tilted faces on gold ornament (why a tilted face at five windows needs both eyes confirmed) | `docs/adr/ADR-0133-tilted-faces-on-gold-ornament.md`, `accept` in `crates/aura-portrait/src/face.rs` |
| Twenty more effects: split tone, film fade, bleach bypass, teal and orange, colour splash, sky blue, spotlight, rim light, chromatic aberration, fisheye, anamorphic streak, kaleidoscope, watercolour, stained glass, cartoon, neon edges, thermal, clipped highlight repair, local dehaze, limbal ring | `docs/adr/ADR-0134-colour-looks-lens-art-and-repair.md`, `crates/aura-render/src/retouch_effects/{looks,lens,art,repair}.rs` |
| The Effects screen (every retouch tool, searchable, favourites, "Use on this photo" opens Retouch with it) and twenty more effects: lash darken, hair fill, match grain, channel swap, sunset glow, day to night, Lomo, instant film, pop art, iris blur, echo, ripple, frosted glass, pixel sort, VHS, emboss, crosshatch, sparkle, fairy lights, rainbow prism | `docs/adr/ADR-0135-effects-library-and-twenty-more.md`, `ui/src/components/effects/EffectsLibrary.tsx`, `ui/src/state/effectRequest.ts`, `crates/aura-render/src/retouch_effects/{grooming,stylise,distort,engrave,sparkle}.rs` |
| The per-photo finishing look (depth, colour bands, vibrance, split tone, skin measured through it), scene colour that is not a cast, parallel folder runs and exports | `docs/adr/ADR-0123-adaptive-look-parallel-folder-runs-and-scene-colour.md`, `crates/aura-app/src/adaptive_look.rs`, `scene_colour_explains` in `crates/aura-app/src/smart_edit.rs`, `crates/aura-render/src/work_gate.rs` (`AURA_IMAGE_WORKERS`), `ExportPass::with_workers` in `crates/aura-export/src/api.rs` |
| Body skin healed on its own patches (packed islands, frame-anchored sampling), matte edges refined on every core, strokes painted over their reach, and no colour added to a black-and-white or toned photograph | `docs/adr/ADR-0124-skin-islands-matte-edges-and-monochrome-looks.md`, `Layout` in `crates/aura-render/src/retouch_clear.rs`, `crates/aura-render/tests/body_skin_islands.rs` (`timing` ignored), `Monochrome` in `crates/aura-app/src/smart_edit.rs`, `adaptive_look::plan` |
| Edge-aware Shadows/Highlights (region gains, clipped regions protected), halo-free Clarity, dark-channel Dehaze, all from one frame-wide guided-filter base on a 512 px grid; Auto enhance sizing Shadows/Highlights from regions | `docs/adr/ADR-0126-edge-aware-tone-clarity-and-dehaze.md`, `crates/aura-render/src/edge_aware.rs`, `frame_base` in `crates/aura-render/src/cpu.rs`, `Stats::base`, `crates/aura-app/src/local_range.rs`, `GlobalPlan::highlights_shadows`, harnesses `crates/aura-render/tests/edge_aware_photos.rs` (ignored, `AURA_LOCAL_PHOTOS`) and `crates/aura-app/tests/auto_range_photos.rs` (ignored, `AURA_RANGE_PHOTOS`) |
| Lateral chromatic aberration measured from the photograph and confirmed on the corrected pixels; the edge-clamped CA resampler; the Defringe tool and the automatic purple-rim removal | `docs/adr/ADR-0125-measured-lens-fringing-and-defringe.md`, `crates/aura-app/src/lateral_ca.rs`, `apply_fringing` and `DEFRINGE_ID` in `crates/aura-app/src/smart_edit.rs`, `correct_ca` in `crates/aura-render/src/geometry.rs`, `crates/aura-render/src/retouch_defringe.rs` (`timing` ignored), `retouch_tools::defringe_rims` |
| Where a folder run's time goes, and the speed work (interpreter convolutions, column-strip blurs, frequency healing on a working grid, rendering ahead of the export writer) | ADR-0123 section 10, `crates/aura-app/tests/batch_profile.rs` (ignored, `AURA_PROFILE_DIR`, `AURA_PERF=1`), `crates/aura-infer/src/onnx/ops/conv.rs`, `WORK_RADIUS` in `crates/aura-render/src/retouch_clear.rs`, `crates/aura-export/src/ahead.rs` |
| Auto-edit qualification on the real app | `scripts/prepare-auto-edit-fixtures.py`, `scripts/test-auto-edit-v3.py`, `scripts/test-auto-edit-gui.py` |
| Segmented face/body skin and the 52 retouch settings | `docs/adr/ADR-0082-segmented-skin-and-retouch-settings.md` |
| Per-face adaptive retouch and scene intent | `docs/adr/ADR-0086-adaptive-retouch-and-scene-intent.md`, `crates/aura-app/src/portrait_features/expert.rs`, `scripts/test-adaptive-batch.py` |
| The skin segmenter (model, card, converter) | `assets/models/selfie_multiclass/`, `crates/aura-vision/src/skin.rs`, `ml/models/skin/` |
| Automatic retouch settings, in the product's own words | `docs/automatic-retouch-settings.md` |
| Acne clear, the heal selection and the blemish brush | `docs/adr/ADR-0092-acne-clear-and-the-blemish-brush.md`, `crates/aura-render/src/retouch_acne.rs`, `crates/aura-render/tests/acne_clear.rs` |
| Frequency healing and texture graft decisions | `docs/adr/ADR-0090-frequency-healing-and-the-texture-graft.md`, `crates/aura-render/src/retouch_clear.rs`, `crates/aura-render/src/retouch_texture.rs` |
| Skin finish checks (painted fixtures, then real pixels by hand) | `crates/aura-render/tests/skin_finish.rs`, `crates/aura-render/tests/skin_finish_photos.rs`, `crates/aura-app/tests/auto_retouch_photos.rs` (preset `pro`) |
| What the Professional retouch preset was run on, and what it did | `docs/professional-retouch-validation.md`, `scripts/verify-professional-retouch.py` |
| Auto advanced retouch: the eighteen-stage professional workflow, one history step per stage | `docs/adr/ADR-0093-auto-advanced-retouch.md`, `crates/aura-app/src/advanced_retouch.rs`, `crates/aura-app/src/advanced_retouch/measure.rs`, `ui/src/components/develop/AdvancedRetouch.tsx` |
| What Auto advanced retouch does, in the product's own words | `docs/auto-advanced-retouch.md` |
| Dark-circle correction measured against the cheek, and its lid guard | `docs/adr/ADR-0094-measured-dark-circle-correction.md`, `crates/aura-render/src/retouch_undereye.rs`, `crates/aura-app/src/portrait_features.rs` (`dark_circle`), `crates/aura-app/src/portrait_features/eye_guard.rs` (`lid`) |
| Full-quality editing previews, their three caches, and the parallel retouch blurs | `docs/adr/ADR-0097-full-quality-editing-previews.md`, `crates/aura-app/src/preview_render.rs`, `ui/src/state/previewCache.ts`, `crates/aura-render/src/bands.rs`, `crates/aura-app/tests/full_quality_preview.rs` (ignored, `AURA_PREVIEW_PHOTO`) |
| Retouch checkpoints, preview request order, per-operation profiling | `docs/adr/ADR-0098-retouch-checkpoints-and-preview-order.md`, `crates/aura-render/src/retouch_cache.rs`, `crates/aura-render/tests/retouch_checkpoints.rs`, `crates/aura-render/tests/retouch_profile.rs` (ignored, `AURA_PROFILE_*`) |
| Live preview while a setting changes (retouch carried over) | `docs/adr/ADR-0099-live-preview-while-settings-change.md`, `CpuEngine::render_live` in `crates/aura-render/src/cpu.rs`, `native_retouch::live_preview` |
| The graphics card: measured, not enabled, and what would pay | `docs/adr/ADR-0100-graphics-card-measured-and-not-enabled.md`, local branch `experiment/gpu-compute` |
| Keeping a face's colour and depth (redness-only evening, whites-confirmed white balance, nostril guard) | `docs/adr/ADR-0101-keep-the-colour-and-depth-of-a-face.md`, `crates/aura-render/src/retouch_acne.rs`, `crates/aura-app/src/smart_edit.rs` |
| ONNX Runtime (DirectML) and the learned masking models: subject, sky, objects | `docs/adr/ADR-0103-onnx-runtime-and-learned-masks.md`, `crates/aura-infer/src/accelerated.rs`, `crates/aura-vision/src/ai.rs`, `scripts/fetch-ai-models.sh`, `docs/model-cards/isnet_general.md`, `skyseg.md`, `sam21_tiny.md` |
| Personal style learned from a Lightroom catalogue, adapting per photograph | `docs/adr/ADR-0104-personal-style-from-lightroom.md`, `crates/aura-app/src/lightroom.rs`, `crates/aura-app/src/personal_style.rs`, `ui/src/components/profiles/PersonalStyle.tsx`, `crates/aura-app/tests/style_adaptive.rs` |
| Windows installer, code signing and offline licence keys (14-day trial, export needs a key) | `docs/adr/ADR-0105-installer-signing-and-licensing.md`, `crates/aura-licence/`, `crates/aura-app/src/licensing.rs`, `tools/licence-issue/`, `scripts/build-installer.sh`, `ui/src/components/LicencePanel.tsx`, `docs/release-process.md` |
| Third-party licence audit, notices, the EULA, and why the FiveK profiles are not shipped | `docs/adr/ADR-0106-third-party-licences-and-the-eula.md`, `scripts/third-party-notices.py`, `legal/EULA.txt`, `ml/edit-profiles/fivek-research-profiles.json` |
| Selling: Paddle shop and licence server, subscription keys that renew in the app, signing route, 1.0.0 | `docs/adr/ADR-0107-selling-aura-from-nepal.md`, `shop/` (website + `api/licence`), `crates/aura-app/src/licensing.rs` (`refresh_licence`), `ops/sign/README.md` |
| Display-referred input (an untouched JPEG is the JPEG), selected bands and their lean, body skin completion | `docs/adr/ADR-0117-display-referred-input-and-selected-bands.md`, `output::unroll` in `crates/aura-render/src/output.rs`, `Lean` in `crates/aura-render/src/retouch_planes.rs`, `crates/aura-vision/src/skin_body.rs` |
| Stretch marks (evening that stops at structure), moles told from spots, repairs refused on structured skin, a matched look kept as a personal look | `docs/adr/ADR-0118-stretch-marks-moles-and-structured-skin.md`, `crates/aura-render/src/retouch_striae.rs`, `near_a_mole` in `crates/aura-render/src/retouch_clear.rs`, `in_structure` in `crates/aura-app/src/portrait_features/deep_blemish.rs`, `personal_style::look_of` |
| Auto edit measured across shooting conditions (veil levels, highlights on evidence, underexposure witnesses, cast by choice, camera-file white balance) and the one-click calibrated finish | `docs/adr/ADR-0120-measured-light-across-conditions.md`, `levels`, `highlights_on_evidence`, `white_object_short_of_white`, `CAST_BY_CHOICE`, `CAMERA_CAST_FLOOR` in `crates/aura-app/src/smart_edit.rs`, `fresh_skin_options` in `crates/aura-app/src/internal_ai.rs`, `scripts/make-condition-variants.py`, `scripts/test-conditions-gui.py`, harness mode `light` and `AURA_PAIR_REVERT` in `crates/aura-app/tests/reference_pairs.rs` |
| AI providers from `AI-CONFIG.md`: routes checked with a generated picture, rest times by cause, measured fallback, proposals held to the photograph | `docs/adr/ADR-0119-checked-ai-routes-and-guarded-proposals.md`, `check` and `discover` in `crates/aura-cloud/src/internal_ai.rs`, `rest_after` in `crates/aura-cloud/src/gateway.rs`, `Seen` and `guard_tone` in `crates/aura-app/src/smart_edit.rs`, `crates/aura-app/src/auto_edit_commands.rs`, `scripts/test-ai-config-gui.py` |
| A before/after pair through the native commands, scored without a window (ignored, `AURA_PAIR_DIR`) | `crates/aura-app/tests/reference_pairs.rs`, `scripts/compare-reference-pairs.py`, results in `docs/evoto-paired-validation.md` |
| Masking in the Studio (AI selections, gradients, brush, combine; sky detector) | `docs/adr/ADR-0102-masking-in-the-studio.md`, `crates/aura-recipe/src/local_masks.rs`, `crates/aura-render/src/local_masks.rs`, `crates/aura-app/src/local_mask_commands.rs`, `crates/aura-vision/src/sky.rs`, `ui/src/components/develop/MasksPanel.tsx` |
| Auto advanced retouch on real photographs (ignored, `AURA_ADVANCED_PHOTOS`) | `crates/aura-app/tests/advanced_retouch_photos.rs` |
| Branching, landing and merging a phase | `scripts/phase-branch.sh`, `scripts/phase-land.sh`, `docs/runbooks/phase-landing.md` |
| One studio, the unattended run and the measured cull | `docs/adr/ADR-0088-one-studio-and-the-unattended-wedding-run.md`, `crates/aura-app/src/one_click_commands.rs`, `crates/aura-app/src/measured_cull.rs`, `ui/src/components/workflow/FinishFolder.tsx` |
| Finishing a folder in the real app, scored against ground truth | `scripts/test-wedding-run.py`, results in `docs/unified-studio-validation.md` |
| Evoto-style finishing: face/body reshape, liquify, background replace, makeup; Connect Claude / ChatGPT | `docs/adr/ADR-0108-portrait-finishing-and-connecting-an-ai-account.md`, `crates/aura-recipe/src/studio_finish.rs`, `crates/aura-render/src/studio_finish.rs`, `crates/aura-app/src/finish_commands.rs`, `crates/aura-cloud/src/browser.rs`, `ui/src/components/develop/PortraitFinishPanel.tsx`, `ui/src/components/ConnectProvider.tsx` |
| Hot Features: layered background repair behind liquify, glare removal by veil subtraction, the shared push-pull fill and distance tools, the real-window qualification | `docs/hot-features.md`, `resample_layered` in `crates/aura-render/src/studio_finish.rs`, `crates/aura-render/src/glare.rs`, `crates/aura-render/src/fill.rs`, harness `crates/aura-render/tests/glare_photos.rs` (ignored, `AURA_GLARE_PHOTO`, `AURA_GLARE_REGION`), `scripts/test-hot-features-desktop.py`; automatic lens-reflection finding `crates/aura-render/src/glare_find.rs` + `find_glare` in `crates/aura-app/src/portrait_commands.rs`, harness `crates/aura-render/tests/glare_find_photos.rs` (ignored, `AURA_FIND_DIR`), `scripts/test-glare-finder-desktop.py` |
| Every Evoto tool and where it lives in AURA | `docs/evoto-parity.md` |
| A hundred measured Evoto-style tools: 30 face and body sliders, skin, makeup, hair, attire, background and people groups that find their own target, 7 hand-placed body/attire/background repairs, ID photos, measured crops, cut-out, grid, carousel, before/after, 2x enlargement, face picker and a no-white-of-the-eye list (not a blink verdict); the learned subject matte frozen into the recipe; parse version 4 (mouth on the line between the lips, lips by hue, sclera beside the iris) | `docs/adr/ADR-0150-measured-portrait-finishing-and-evoto-workflows.md`, `docs/evoto-hundred-tools.md`, `crates/aura-recipe/src/studio_finish/evoto.rs`, `crates/aura-render/src/studio_evoto/`, `crates/aura-render/src/retouch_effects/evoto.rs`, `crates/aura-app/src/evoto_workflow.rs`, `ui/src/components/develop/EvotoWorkflowPanel.tsx`, harness `crates/aura-app/tests/evoto100_photos.rs` (ignored, `AURA_EVOTO_PHOTOS`, `AURA_EVOTO_OUT`) |
| Portrait light (key light shaped by an ellipsoid head, measured fill, placed catchlights, glow) and Auto light | `docs/adr/ADR-0127-portrait-light-and-auto-light.md`, `crates/aura-render/src/portrait_light.rs`, `Light` in `crates/aura-recipe/src/studio_finish.rs`, `suggest_portrait_light` in `crates/aura-app/src/finish_commands.rs`, the Light tab of `ui/src/components/develop/PortraitFinishPanel.tsx`, harness `crates/aura-render/tests/portrait_light_photos.rs` (ignored, `AURA_LIGHT_PHOTOS`) |
| Evoto's Beauty & Fashion pass, measured from its homepage: the bright-backdrop lift and the preset | `docs/adr/ADR-0109-evoto-beauty-and-fashion-pass.md`, `crates/aura-app/src/advanced_retouch/measure.rs` (`backdrop_lift`), `crates/aura-app/src/advanced_retouch.rs` (`beauty_fashion_options`), `crates/aura-app/tests/beauty_fashion.rs` |
| Portrait parsing and portrait retouch decisions | `docs/adr/ADR-0065-measured-portrait-parsing-and-portrait-retouch.md` |
| Faces, landmarks and the nineteen portrait regions | `crates/aura-portrait/` |
| The vendored Haar cascades, their converter and their card | `crates/aura-portrait/data/`, `docs/model-cards/haar_cascades.md` |
| The fourteen portrait operators the renderer executes | `crates/aura-render/src/portrait.rs` |
| Portrait evaluation instruments (your own photographs) | `crates/aura-portrait/tests/local_eval.rs`, `crates/aura-render/tests/portrait_local.rs` |
| What portrait retouch finds and changes, in the product's own words | `docs/portrait-retouch.md` |
