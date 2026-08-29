# Phase 24 progress - Generative Cleanup & Distraction Removal

One line per task, in the order section 8 orders them. Files touched, tests added, and what the
task actually cost.

| # | Task | Files | Tests | Notes |
|---|---|---|---|---|
| 1 | Publish the generative policy | `docs/generative-policy.md` | - | CTO/PM/SEC co-signed before any code, section 8 step 1 |
| 2 | Freeze the contract | `crates/aura-core/src/contract/cleanup.rs`, `ids.rs` | contract unit tests | 935 lines; `ProposalId` amends the frozen id list, ADR-0049 §10 |
| 3 | Register the errors | `crates/aura-core/errors.toml`, `errors/ml.rs`, 8 runbooks | registry test | ML 5115-5122; five of eight are refusals |
| 4 | The policy table | `crates/aura-generative/{policy.rs,config/cleanup_policy.toml}` | 6 | 23 scene rows, each with a written reason; the loader may only tighten |
| 5 | **The safety engine, before any removal code existed** | `safety.rs`, `denylist.rs` | 17 | Section 8 step 3, taken literally |
| 6 | Unexplained-salience detection | `detect.rs` | 7 | Names nothing; every candidate is `Unclassified` |
| 7 | The safety gates | `tests/eval/cleanup_eval.rs` | 11 | Written against the engine, not against the removals |
| 8 | The shared pixel view | `pixels.rs` | 10 | One bilinear sample, one luminance, one feather in the crate |
| 9 | Sibling-frame borrowing | `borrow.rs` | 9 | Exhaustive least-median homography over 495 four-subsets; deterministic by construction |
| 10 | Classical content-aware fill | `fill.rs` | 8 | Exemplar synthesis, onion-peeled, plus a harmonic seam correction |
| 11 | The diffusion tier | `inpaint.rs` | 4 | Declared, refused, reachable. No fallback under it |
| 12 | **The choke point** | `source.rs`, `tests/one_choke_point.rs` | 7 + 6 | `select` takes a `SafeCandidate`, which has no public constructor |
| 13 | The artefact self-check | `selfcheck.rs` | 11 | Three measurements over the result; cannot see the before-state |
| 14 | The editorial-judgement port | `judgement.rs` | 4 | An answer type with no approving variant |
| 15 | The queue, the bands and the revert | `queue.rs` | 12 | Safety, source, self-check, judgement, band - in that order |
| 16 | Migration 24 | `crates/aura-catalog/migrations/0024_cleanup.sql` | 12 | Four tables, two views, four triggers |
| 17 | The store | `store.rs`, `tests/store_and_triggers.rs` | 6 + 12 | Every refusal test runs its control first |
| 18 | The frozen service and the pass | `api.rs` | 5 | Resumable; the work remaining is a query |
| 19 | The cloud task | `crates/aura-cloud/src/cleanup_judgement.rs` | 10 | The first task in the product that can only say no |
| 20 | The recipe disclosure | `crates/aura-recipe/src/contract/recipe.rs` | recipe suite | `Recipe.cleanup[]`; fourth frozen-contract amendment in the product |
| 21 | The render stage | `crates/aura-render/src/cleanup.rs`, `shaders/cleanup_paste.wgsl`, `graph.rs` | 7 | `Stage::Cleanup` at index 18; `ORDER` goes 23 → 24 |
| 22 | The removal gates | `tests/eval/cleanup_eval.rs` | 15 | The three that were pending now measure the modules they were waiting for |
| 23 | The IPC surface | `crates/aura-app/{cleanup_commands.rs,contract/ipc.rs,state.rs}`, ADR-0050 | app suite | Nine commands; 180 → 189, three-way count still equal |
| 24 | The panels | `ui/src/components/cleanup/{ProposalQueue,BeforeAfter,ManualRemove}.tsx` | tsc clean | Props-driven; no view mounts them yet |
| 25 | The ML scripts | `ml/models/generative/{train_distraction,train_artefact,eval_cleanup}.py` | - | Two audit a dataset that does not exist; the third reads a real catalog |
| 26 | The phase gate | `crates/aura-cli/src/phase24.rs`, `justfile` | gate | `aura-cli verify --phase 24` exits 0 |

## What the numbers came out at

- **149 tests in `aura-generative`**: 117 unit, 26 evaluation gates, 6 choke-point greps, 12 store
  and trigger tests. Plus 10 in `aura-cloud`, 7 in `aura-render`.
- **300 adversarial attempts** in the gate, zero successes.
- **16 of 31 reason codes are refusals** - the highest proportion in the product.
- **`mask_covered` is 0 % on every project**, for two independent reasons. See the exit report.

## Six things that were got wrong first, and what each one cost

Kept because each is a trap the next phase to touch pixels will meet.

**1. A correlation is the wrong instrument for "is this the same object".** `borrow` refused a
sibling frame when the aligned region *correlated* with the target above 0.80. Normalised
cross-correlation over a flat window is undefined and returns zero by design - and a gaffer-taped
cable, an exit sign and a caterer's crate are all close to flat. So a burst neighbour containing the
**identical object** correlated at zero, read as "completely different", and the borrow went ahead:
replacing the exit sign with the exit sign, which is the single failure the refusal exists for. It
is a mean absolute difference now, scaled by the ring's own spread.

**2. Both removal modules feathered toward the object they were removing.** The seam feather ran
*inward* from the region's boundary, so `original * (1 - w) + replacement * w` blended the outermost
samples of the replacement back toward the bin. The code that exists to hide a seam left a rim of
the distraction behind. It is phase 18's resampler defect in a different module - a halo
manufactured by the delivery code rather than by the code that decides. `pixels::feather_out` is the
fix: full weight over the whole object, falloff on the band of background outside it.

**3. Comparing two maxima compares two unrelated facts.** The repeated-texture check took the
patch's strongest autocorrelation and subtracted the frame's strongest. A background with a slow
twenty-pixel undulation scores as high as a patch that repeats hard at four, so the difference came
out at nothing and a synthesis artefact passed. Section 6.4 asks for "a period that occurs nowhere
else", which is a **per-lag** comparison.

**4. A 99th percentile of zero is not a threshold.** The ghost-edge check compared the seam against
the frame's own step distribution and bailed out when that came back zero - which it does for any
smooth frame, because a 256-bucket histogram cannot resolve a step below 1/255. The result was that
a hard rectangle edge in a perfectly smooth photograph scored as **no artefact at all**. Phase 22's
rule for the third time in this repository: a threshold on a measurement is a statement about the
instrument, and it has to be floored at what the instrument can see.

**5. An exemplar fill is correct in texture and wrong in tone.** The synthesis matches the *pattern*
around a hole and takes it from a slightly different part of the shading, so a wall came back
perfect in every local detail and a tenth of a stop out overall - a rectangle. The fix is a harmonic
correction field solved from the seam discrepancy, which is only a tone shift and therefore cannot
introduce structure the fill did not copy from somewhere in this photograph.

**6. A fixture that looks like noise and is a linear congruence.** The untouched-frame gate failed at
0.252 against a threshold of 0.25 because `(x * 7919 + y * 104_729) % 1000` repeats, and the patch
and the reference windows sat at different phases of its period. A fixture on its own threshold
measures f32 arithmetic rather than the rule - the trap phases 19, 21 and 22 each hit - and here it
was the fixture that was wrong rather than the code.

## What was deliberately not built

- **A trained distraction detector.** No labels, no data. `detect::candidates` measures unexplained
  salience and names nothing, so the safety engine refuses everything it finds.
- **A diffusion inpainting tier.** No model, no runtime support, no reachable provider - and no
  fallback under it, because the fallback would be the classical fill the selector already tried.
- **A "creative fill" mode.** ADR-0049 section 12. A warning dialog is a thing a user clicks past on
  the second use.
- **Any way to reorder borrow, fill and inpaint.** Diffusion is faster than a homography search
  across a moment, so the reordering would be chosen for the reason that makes it worst.
