# Phase 15 progress - Exposure AI & White Balance AI

One line per task group, in the order section 8 asks for them. Files touched, tests added,
benchmark delta.

## T1 - DATA: the training set (section 8 step 1)

**Not done, and it cannot be done here.** Section 9 budgets ten days for "RAW + expert final
edits with exposure/WB parameters across traditions and lighting types". There are no camera
files, no photographed ColorChecker and no expert edits in this repository. What shipped
instead is `crates/aura-brain-photo/src/tone/fixtures.rs`: synthetic frames built physically
(reflectance x illuminant -> linear RGB -> sRGB), so the right answer is arithmetic rather
than opinion, and five skin reflectances spanning light to dark so a tone-group comparison is
possible at all. Condition C1.

## T2 - SRC/COL: statistics, neutrals and skin patches (section 8 step 2)

Files: `tone/stats.rs`, `tone/neutrals.rs`. One decode, every measurement the rest of the
phase reads. Tests: block uniformity, the specular exclusion, the luminance floor, the skin
patch sampler's trim.

## T3 - COL: the hypothesis generators (section 8 step 3)

Files: `tone/illuminant.rs`, `aura-raw/src/colour/illuminant.rs` (new: `cct_to_uv`,
`cct_from_uv`, `duv`, `uv_distance`, `adapt`). Four generators in a fixed order plus the
learned slot. Tests: the fixed order, the CCT round trip, the mixed-light split, the kind
classifier.

## T4 - COL: the per-identity skin locus (section 8 step 4)

Files: `tone/skin_locus.rs`, `identity_skin_locus` in migration 15. Tests: one bad frame
cannot move the centre, a dark and a light person get loci of the same tightness, the same
person under two lights gives one locus, a low-confidence frame contributes nothing.

## T5 - COL: the constrained solve (section 8 step 5)

Files: `tone/solve.rs`, `tone/wb.rs`, `tone/exposure.rs`. Twenty-step linear scan rather than
a bisection (the satisfying set is not an interval with two people in frame). Tests: the
veto, the least-bad fallback, the clipping clamp, the shadow ceiling, the dead band.

## T6 - SRML: the two heads (section 8 step 6)

Files: `aura-infer/src/onnx/fixtures.rs`, `xtask/src/models.rs`,
`ml/models/tone/{train_wb,train_exposure,export,eval_tone}.py`,
`docs/model-cards/{white_balance,exposure_scene}.md`, `models/models.lock`,
`models/manifest.sig`. Both heads are signed placeholders and **neither is consulted**:
`WB_HEAD_TRAINED` and `EXPOSURE_HEAD_TRAINED` are false. int8 is forbidden on both. Tests:
`python ml/models/tone/eval_tone.py --self-test` (every metric rejects its degenerate case),
`export.py --check` (the train/serve contract, including the linear-input declaration).

## T7 - COL: mixed light and coloured light (section 8 step 7)

Files: `tone/illuminant.rs` (the split), `tone/solve.rs` (the preserve-mood policy),
`config/exposure_targets.toml` (22 scene rows, `preserve_coloured_light` per row). Tests: two
lights detected and both regions recorded, a single-light frame is not marked, a purple dance
floor keeps its cast, a red mandap is not read as red light.

## T8 - SRC: reference frames (section 8 step 8)

Files: `tone/reference.rs`, `segment_reference_frames` in migration 15. Tests: the answer does
not depend on input order, a chapter with two candidates gets none rather than bad ones.

## T9 - SRC/SFE/MFE: recipes, the wire and the panels (section 8 step 9)

Files: `crates/aura-app/src/tone_commands.rs` (new, 7 commands), `contract/ipc.rs` (11 DTOs),
`state.rs` (`tone_store`, `tone`, `tone_pass`, `frame_exif`), `ui/src-tauri/src/main.rs`,
`ui/src/ipc/{types.ts,client.ts}`, `ui/src/components/develop/{BasicPanel,ToneReviewPanel}.tsx`,
`docs/adr/ADR-0032-tone-ipc-surface.md`. The recipe write goes through
`aura_recipe::schema::merge` in the command layer, because `aura-brain-photo` deliberately does
not depend on `aura-recipe`. Tests: 16 in `BasicPanel.test.tsx` including "offers no control
that grades" and "a person's value survives an automated pass".

## T10 - QAL/QAIQ: the gates (section 8 step 10)

Files: `tests/eval/tone_eval.rs` (22 gates), `crates/aura-cli/src/phase15.rs`,
`crates/aura-perf/tests/tone_budgets.rs`, `perf/budgets.toml`, `justfile`. The fairness gate
reports mean and spread and refuses a pass from one bucket. QAIQ's 600-frame blind audit has
**not** been done - condition C3.

## Defects this phase found and fixed

Four, all found by the gate or the harness rather than by review. They are listed because
three of them were silent.

1. **The white-balance confidence penalised hypotheses that agreed.** `CLEAR_MARGIN` read the
   *cost gap* between the top two candidates as evidence, so two independent estimators landing
   on the same chromaticity - the strongest evidence available - scored as "undecided". Every
   frame in a wedding therefore scored below `MIN_CONTRIBUTING_CONF`, no frame contributed a
   skin sample, no locus was ever built, and section 6.3's hard constraint bound on nothing.
   Replaced with an agreement term over the top two answers' `u'v'` distance (`AGREE_UV`).
   Before: 0 samples, 0 loci, every frame `SkinLocusUnavailable`. After: 78 samples, 5 loci,
   98 % of frames skin-constrained.
2. **Migration 15's foreign keys named columns that do not exist.**
   `REFERENCES identities(identity_id)` and `REFERENCES segments(segment_id)`; both tables key
   on `id`. Every `put_locus` and every reference-frame write failed with a foreign-key
   mismatch. Caught by the phase gate on its first run.
3. **An override was written and could never be read back.** `set_override` fills
   `user_exposure_ev`, `user_temperature_k` and `user_tint`; nothing read them, and the frozen
   `ToneService` has no field for them. Added `ToneStore::override_of` and three optional
   fields on `ToneDto`, so the panel shows the photographer's number and still says what AURA
   suggested - which is what the review queue and phase 30's learning loop need.
4. **`round3` rounded in `f32` and widened to `f64`.** `0.263_f32` widens to
   `0.263000011444091796875` and `serde_json` prints every digit, because that is the shortest
   string that round-trips as an `f64`. The three stored documents cost 687 B instead of 455 B
   - about half the per-image budget, spent on noise.

## Benchmark delta

New rows only; nothing earlier moved.

| Measurement | Figure |
|---|---|
| `tone_estimate_frame` (debug, processor) | 1,148 ms/image; release guard 500 ms/unit |
| 4,000-image extrapolation (debug) | ~4,590 s |
| `tone_store_per_1000_images` | **806.9 B/image** against section 11's 600 B |

The storage row is over budget and the figure is recorded rather than the schema squeezed;
`perf/budgets.toml` carries the decomposition and the four reductions that were considered and
rejected. The two runtime rows name an RTX 4070 and this build has no GPU backend (ADR-0007),
so they are waived.

## Contracts

`contracts.lock` re-locked: `crates/aura-core/src/contract/tone.rs` is new,
`crates/aura-app/src/contract/ipc.rs` and `ui/src/ipc/types.ts` changed together.
`crates/aura-recipe/src/contract/recipe.rs` also re-locked - its digest was stale at HEAD and
the file itself is unchanged.
