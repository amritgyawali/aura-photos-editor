# PHASE-17 exit report - Style Learning: Scene-Conditional Personal AI Profiles

**Branch:** `feat/phase-17-style-learning-personal-ai` · **Gate:** `aura-cli verify --phase 17`
exits 0 · **Status:** implemented **conditionally**, on the five conditions in section 8.

## 1. What shipped

One frozen contract, one new crate of thirteen files, one migration, one IPC surface, four
panels, two ADRs, three Python scripts, one product document and a gate. **No model.**

`aura-core::contract::style` freezes the shape: `StyleProfile`, `StyleDelta`, `CurveShift`,
`SkinBias`, `SceneGroup`, `LightingBucket`, `StyleBucket`, `BucketModel`, `ProfileDiagnostics`,
`FallbackLevel`, `MatchMethod`, `ExtractSource`, `StylePair`, `StyleQuery`, `StyleAdvice`,
`StyleOutline`, twenty reason codes and `StyleService`. `ids.rs` gains `ProfileId`. **There is
no field anywhere in it for a skin colour**, which is the same structural defence phases 15 and
16 made and the third time this product has made it.

`aura-style` learns. `pairs.rs` matches originals to finals by four strategies and refuses an
ambiguous one. `extract.rs` reads an XMP exactly, or hands the pair to `fit.rs`, which
reproduces the delivered photograph by coordinate descent over twelve parameters **through the
real renderer** and rejects what it cannot explain. `bucket.rs` sorts the pair into one of
eighty leaves. `tree.rs` fits ridge regressions with Huber reweighting, shrinks each level
toward its parent by `n / (n + k)`, and caps what any one wedding may contribute.
`diagnostics.rs` measures the result on held-out pairs *against the baseline as well as against
the ceiling* and writes one sentence about what to shoot next. `infer.rs` resolves a leaf
through bucket, group, global and factory, and **always answers**. `profile.rs` versions, signs
and refuses. `store.rs` owns migration 17 and `api.rs` is the frozen service and the resumable
walk.

`aura-brain-photo` gains two small modules - `tone/style.rs` and `colour/style.rs` - that apply
a `StyleAdvice` to the **solved** parameters and then let phase 15's clipping bound, phase 15's
skin-locus constraint, phase 16's clipping guard and phase 16's skin guard run on the result.
That ordering is the phase's central decision and it is not negotiable by any later one.

Migration 17 adds `profiles`, `profile_buckets`, `style_pairs`, `project_style` and
`v_style_coverage`. **There is no skin colour anywhere in it**, the two skin *lean* columns
carry CHECKs below phase 16's own ceilings, and the gate scans for both on every run.

The IPC surface is eleven commands (ADR-0036); the Teach My AI wizard shows what a folder
contains before anything is fitted, the profile report leads with a measurement, the bucket
matrix distinguishes a taught leaf from a borrowed one, and the A/B comparison renders numbers
rather than pixels.

## 2. Acceptance criteria (section 13)

| Criterion | Status |
|---|---|
| Pointing the app at past weddings produces a scene-conditional profile with a per-bucket accuracy report | **met in code and measured on synthetic archives.** Not exercised on a photographer's archive - C1, C3 |
| Edits made with the profile are measurably closer to the photographer's own edits than the factory baseline | **met on synthetic archives** - 1.4 dE00 styled against 4.6 baseline on the `airy` look, and `gate_2b` asserts a do-nothing style *fails* the same ceiling |
| Weak buckets are named with a concrete recommendation for what to add | **met** - `diagnostics::recommend`, one bucket, one sentence, generated from the gap and shown identically in the panel and the CLI |
| Profiles can be versioned, compared, adopted, exported and shared safely | **met** - versions are rows, adoption retires its predecessor, the bundle round-trips and five kinds of tampering are refused. "Safely" means integrity, not provenance - C2 |
| Training runs locally with progress, cancel and resume, and never uploads imagery | **met, and the last clause is structural** - no cloud dependency, no socket, no field that could hold an image. **Not exercised end to end**, because the shell has no archive to hand it - C3 |
| At least three of five validation photographers cannot reliably distinguish AURA's output from their own | **not done.** There are no photographers here - C1 |

## 3. What the section 10.1 gates measured

`cargo test -p aura-style --test style_eval` - 19 gates, all green.

| Gate | Threshold | Measured |
|---|---|---|
| Fitted parameters match known ground truth | residual inside `REJECT_DE00` on 8 pairs | **0.97 dE00 worst**, exposure recovered to **0.02 EV** |
| Style match dE00 on held-out pairs | <= 2.5 | **met**, and asserted to beat the unstyled baseline on the same frames |
| A weak bucket's ceiling | <= 3.5 | **met**, and a populated bucket at 3.2 is asserted to *fail* |
| A usable profile improves on the baseline in every populated bucket | exact, 4 buckets, 300 pairs | **met**, every bucket answered at its own level |
| A sparse bucket falls back without erratic output | < 10 samples | **met** - four wild pairs moved the leaf by under 0.05 EV from its parent |
| One outlier wedding cannot shift the global profile | <= `influence_bound(2.0)` = 0.70 EV | **0.63 EV**, and the same archive alone moves it past 0.5 |
| Bundle round-trip and tamper refusal | exact | **met** - digest, signature, schema and size all refuse |
| Determinism | byte-identical | **met**, tree and canonical form |

Six of the nineteen exist to prove the harness can fail: a dodged region must be rejected as
unmodelled work, a do-nothing style must miss the ceiling, a single-archive fit must follow its
one archive, an unmeasured bucket must report `null`, a populated bucket at 3.2 dE00 must fail
its own ceiling, and a project with no profile must still produce a reason.
`ml/models/style/{train_residual,eval_style,export}.py --self-test` make the same assertions on
the Python side; `export.py --check` also verifies that the shared constants agree across the
two implementations and that `models.lock` still names no style model.

**Every one of these numbers is about synthetic archives.** A look was chosen, applied to
authored plates through the real renderer, and recovered. That proves the matcher, the fitter,
the bucketing, the regression, the shrinkage, the archive cap, the bundle and the store. It is
not evidence about a photographer.

## 4. Benchmarks

| Row | Section 11 | This build |
|---|---|---|
| Recipe fitting per pair (GPU, 512 px) | <= 1.5 s | **waived** - no GPU backend (ADR-0007). 160 CPU renders of a 96x64 plate take about 40 ms in release; a 512 px plate is roughly 28x the pixels, so the reference path is around 1.1 s per pair and the budget is not measured on a reference machine |
| Training 2,000 pairs end to end (RTX 4070) | <= 25 min | **waived**, same reason. The tree fit itself is 41 seven-by-seven solves per level and is not the cost |
| Training 2,000 pairs (M3 Pro) | <= 45 min | **waived** - no reference machine has run this build |
| Style inference overhead per image | <= 2 ms | **met by construction.** `infer::advise` is three map lookups, an addition and a clamp - which is what storing each level's intercept rather than its slopes buys (`tree.rs` header) |
| Profile size | <= 3 MB | **met** - a bundle with one populated leaf is 1,717 bytes; `MAX_BUNDLE_BYTES` refuses anything above the ceiling before parsing it |
| Extra storage per profile | (no section 11 row) | **1,540 B against this phase's own 12 KB budget** |

## 5. Telemetry (section 11)

`style.training` (pairs, accepted, rejected, buckets, ms) is `api::STAGE` and is filled by
`TrainPass::run`. `style.profile_adopted` (profile, version, overall_de00) is `ADOPTED_STAGE`.
`style.bucket_fallback` (bucket, fallback_level) is `FALLBACK_STAGE` and is what
`StyleOutline::level_counts` aggregates - the number that matters when it is skewed, because a
wedding whose frames all resolve at `global` has had its scene conditioning do nothing.

## 6. Invariants

1. **Never mutate a RAW.** No path column that points at a photograph, no file operation on an
   original, and the archive walk opens nothing.
2. **Confidence and reasons.** `StyleAdvice` carries both and `StyleAdvice::none` is a complete
   explained answer rather than an absence - the one case where a phase could have returned
   `Option` and did not.
3. **Three-tier compute.** The fit runs at 512 px, section 6.1's own figure, and never at full
   resolution.
4. **Determinism.** No randomness, no clock and no map iteration order anywhere in the fit. The
   held-out split is every fifth pair in hash order.
5. **Resumability.** The unit is a **pair**, keyed on the original's content hash, so a re-scan
   after a rename re-uses the fits too.
6. **Local-first.** Section 7 says there is no cloud call in this phase; `aura-style` depends on
   no cloud crate and `tests/no_network.rs` fails the build if that changes.
7. **Scene-conditioned everything.** Eighty leaves, and the fallback level is recorded on every
   answer.
8. **Colour discipline.** Nothing here encodes; the fit compares two sRGB outputs of the *real*
   output transform, and the style reaches pixels only through phase 14's renderer.
9. **No silent failure.** Six codes, `AURA-ML-5072` to `5077`, each with a runbook. A rejected
   pair is **written** with its reason, which is the opposite of what phases 09, 15 and 16 do
   with a failed frame and is right here: a rejection is the evidence.

## 7. Rollback

Migration 17 is reversible: one `DROP VIEW`, three `DROP TABLE`s and one `DELETE` return the
catalog to schema 16. Both `ANALYSIS_VER` bumps heal themselves, because `ToneStore::pending`
and `ColourStore::pending` are keyed on them.

**It is recomputable only if the archives are still there**, which is the first migration since
01 where that caveat is real: a profile is derived from folders outside the catalog. The
rollback runbook says to export every adopted profile to a signed bundle first, which is the
whole reason export exists as a first-class feature rather than as a convenience.

Feature flag: a project with no selected profile gets exactly what phases 15 and 16 decided on
their own. There is no state of this system in which the feature makes a photograph worse than
switching it off would, which is the safety property of the residual design and is asserted by
`gate_4b`.

## 8. Conditions carried out of this phase

**C1 - There are no photographers' archives here, so no number in this phase is about a
photograph.** `Sev 2.` Section 9's DATA task - "collect consented archives from 5 photographers
across traditions" - has not happened and cannot happen in this repository. Everything measured
above is a synthetic archive whose look was chosen, applied through the real renderer and
recovered.

This is a **different** gap from every placeholder-weights condition before it, and the
difference matters. Phases 05 to 16 ship real code waiting for real *weights*; this phase ships
real code waiting for real *weddings*. There is nothing to train and nothing to sign - the fit
has a closed form - so the day an archive arrives, this phase produces a real profile with no
further engineering. Section 13's sixth criterion, the blind study, is what closes it.

**C2 - The bundle signature proves integrity and not provenance, and the product says so.**
`Sev 3.` With the public key inside the bundle, a verified signature proves the document has not
changed since somebody signed it and proves nothing about who. There is no key distribution in
this product. ADR-0035 decision 8 records the argument, `AURA-ML-5076`'s runbook says it in the
operator's words, `docs/style-profiles.md` says it in the photographer's, and
`ProfileReport.test.tsx` asserts the panel never renders the word "verified". A studio PKI is a
real feature and belongs to whatever phase builds distribution.

**C3 - The shell has no archive-import flow, so `train_profile` refuses in this build.** `Sev 3.`
The command is registered, its shape is frozen and it goes through the real pass; what it lacks
is a `PairSource`, because nothing in the desktop shell yet reads a folder of somebody's
finished work into one. It is wired to `fixtures::EmptySource`, so it fails with
`AURA-ML-5073` - "not enough usable pairs" - which is the honest answer rather than a silent
success. `scan_archive` is complete and does open a real folder. Closing this is a file-reading
adapter and touches no frozen shape.

**C4 - The baseline a training run is a residual *from* is supplied by the caller, and in this
build it is neutral.** `Sev 2.` `api::Baseline` is a port; given phases 15 and 16's real
per-frame decisions, a `StyleDelta` is what this phase promises. Given `NeutralBaseline` - which
is what the shell has, because the frames in an archive are files rather than catalog rows - the
delta is the photographer's **absolute** edit relative to a neutral develop, which is a larger
number and a different claim. The bounds in `aura_core::contract::style` clamp most of it away,
which is correct behaviour and is also the signal that the baseline was wrong. The production
answer is to import an archive as a project and run phases 15 and 16 over it first; that is the
same adapter C3 needs.

**C5 - The lighting axis is not filled in by either consuming pass.** `Sev 3.` `ColourPass` and
`TonePass` both resolve a style at `LightingBucket::Unknown`, so every frame answers at
`FallbackLevel::Group` rather than at its leaf. The tone pass **cannot** do better - it is the
pass that decides what colour the light was, so it cannot condition on the answer - and the
colour pass could, by reading `ToneService`. It is recorded on every decision and counted in
`style.bucket_fallback`, so the degradation is visible rather than silent. Wiring `ToneService`
into `ColourPass` is a small change and it belongs with the phase that needs the light.

## 9. Two corrections to earlier phases

**`contracts.lock` had a stale entry for `crates/aura-core/src/contract/colour.rs`.** The file
is byte-identical to what phase 16 committed, and the recorded digest was not its digest - so
`cargo xtask contracts --check` would have failed on `main`. Phase 16 re-locked before a final
edit to the contract. It is corrected in this phase's re-lock, which is the same shape of fix
phase 16 applied to phase 15's omitted migration.

**The justfile had no `phase-16-verify` recipe.** The gate has existed since phase 16 shipped
and the only way to run it was to remember the argument. Added beside this phase's.

## 10. What phase 18 inherits

- **`StyleService` is the only way to ask what a photographer's own look is.** Thirteenth
  service of its kind. Phase 25 normalises a gallery toward these values, 26 matches a second
  shooter to them, 27 checks them, 28 acts on them unattended and 30's learning loop updates
  them. No phase may keep its own style profile or its own bucket vocabulary.
- **A style is a residual, and the baseline is never re-derived.** An empty profile produces
  exactly the baseline. Any later phase that finds itself computing an absolute from a profile
  has misunderstood the shape.
- **The shift happens before the guards, and every guard re-runs after it.** Phase 16 wrote this
  rule before this phase existed and this phase implemented it; phase 18's local adjustments
  inherit it unchanged. A style that would move somebody's skin is a style the guard withdraws.
- **A rejected pair is stored, not dropped.** It is the only place in the product where a failure
  writes a row, and the reason is that here the failure *is* the evidence. Phase 30's learning
  loop reads the same table.
