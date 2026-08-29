# PHASE-20 exit report - Portrait Retouch AI with Natural Texture Protection

**Branch:** `feat/phase-20-portrait-retouch-ai` · **Gate:** `aura-cli verify --phase 20` exits 0 ·
**Status:** implemented **conditionally**, on the five conditions in section 8.

## 0. Read this first: what this phase can and cannot claim

Every algorithm in this phase is real, tested and enforced. Every *number* in it is measured
against synthetic faces whose marks were painted in by `fixtures.rs` and read back through the
real detector, the real operators, the real renderer and the real store.

Four separate things stand between that and a claim about a wedding photograph, and they close at
four different times:

* **Phase 06's face detector is a placeholder**, so on a real photograph there are no faces, no
  landmarks and no identities. Nothing here runs on a real frame, and the cross-frame permanence
  rule - the strongest evidence this phase has - has no correspondence to accumulate. This closes
  with phase 05's condition C10 rather than separately.
* **No skin mask reaches this pass.** Phase 18 ships `MaskService`, but nothing fills
  `RetouchPass::with_masks`, so every operation is withdrawn and every plan says
  `mask_unavailable`. That is a wiring task rather than a missing phase - the same state phase 19
  is in, and for the same reason.
* **Both shipped heads are untrained and neither is consulted.** What runs is the measured
  detector in `blemish.rs`. ADR-0043 section 7 records why this phase ships a measurement
  underneath its placeholder rather than refusing to detect at all, which is what phases 15, 16
  and 18 do.
* **No blind study and no per-skin-tone parity study exist.** Section 10.1's last two rows are
  recorded as unmet rather than estimated, and a test in the eval harness names them so a missing
  gate cannot look like a passing one.

**No later phase may claim a retouch quality result until all four close.**

## 1. What shipped

**The frozen contract.** `aura_core::contract::retouch` freezes the four operations, the two
inpainting methods, the two bands an operator may name, the four presets, the six protected kinds
and their three sources, the protected feature, the texture report, twenty-six reason codes, the
plan, the outline, the override and `RetouchService`. Six spellings differ from section 5 and
ADR-0043 section 2 records each one.

**The decision crate.** `aura-retouch`, eleven modules plus fixtures, errors and guard:

| Module | What it owns |
|---|---|
| `presets` | the preset table, the per-scene limits and the floor bound the code owns |
| `strength` | one gallery-constant strength per person, from four gallery statistics |
| `blemish` | the measured detector: mid-band anomalies in luminance *and* colour, one sign at a time |
| `permanent` | the face-frame projection, the single-frame classifier, the cross-frame rule |
| `undereye` | a capped lift and a capped de-tint, both measured against the surrounding skin |
| `evening` | mid-band unevenness, calmed without reaching the high band |
| `texture_guard` | the guarantee, measured through the real renderer, with re-solve and withdrawal |
| `ops` | one decoded frame in, one plan out |
| `store` | migration 21 and the codec |
| `api` | the frozen service, the resumable pass and the second pass that settles permanence |
| `guard` | the three refusals this crate turns into `AURA-ML-5097` and `5098` |

**The renderer half.** `aura_render::bands` - the three-band separation, moved here for its second
consumer - and `aura_render::retouch`, the processor reference for the stage, plus three WGSL
files held to it by `shader_parity.rs`. Phase 14's pass-through `stage_retouch` in `spatial.wgsl`
retired.

**Migration 21.** Four tables, one view, two triggers. `retouch_protected` is the first table in
this product whose rows a photographer creates directly and whose subject is a person; its
`is_absolute` column is generated rather than supplied, and a trigger aborts any delete of a
protected tattoo.

**Two signed models with cards**, both untrained and neither consulted. **Four Python scripts**,
all self-testing without PyTorch. **Eight IPC commands** and a panel. **Six error codes and six
runbooks.** **Thirteen evaluation gates**, the mechanical gate, and two budget rows.

## 2. Acceptance criteria (section 13)

| Criterion | Status |
|---|---|
| Temporary blemishes disappear while pores, fine lines and permanent features remain | **Met on fixtures.** `retouch_eval` gates 1, 2 and 7; the detector removes a painted spot and refuses a painted mole, a freckle field and a tattoo, and the texture ratio stays at 0.999 |
| Freckles, moles, scars and tattoos preserved by default and listed as protected | **Met.** The veto is geometric and pre-strength; a tattoo cannot be cleared by the service or by the database. Gate 5 |
| Under-eye and uneven tone corrections visible as improvement, not as retouching | **Partly.** The caps hold and no corrected region ends brighter than the skin around it (gate 4). Whether it reads as improvement is the blind study - condition C4 |
| The same person looks like the same person across the gallery | **Met by construction.** Strength is one stored number per identity; the spread is zero rather than the five per cent asked for. Gate 3 |
| Texture retention measured, gated in CI and reported in the UI | **Met.** Stored per row, asserted by gate 1, printed by the panel with its sample count, and run in CI |
| Blind expert study shows parity or better | **Not met.** Condition C4 |

## 3. What the section 10.1 gates measured

| Gate | Asked for | Measured |
|---|---|---|
| Texture retention on all presets | >= 0.90, Polished never below 0.80 | 0.999 on the fixture heal; every preset floor bounded by the code |
| Blemish recall | >= 0.90 | 1.00 over three painted spots |
| False removal of permanent features | <= 2 %, tattoos 0 % | 0 % over a mole, nine freckles and a tattoo |
| Cross-frame consistency | <= 5 % spread | 0 % |
| Under-eye cap | never exceeded | 0.25 EV exactly, and reported as capped |
| Per-skin-tone parity | no bucket 10 % worse | **not measured** - condition C2 |
| Proxy matches full resolution | within a perceptual tolerance | the same plan leaves the same share of the mark at 160 px and at 320 px, within 0.15 |
| Blind expert study | parity or better | **not run** - condition C4 |

Two further gates are this phase's own: an evening-only plan costs no texture at all (which is the
strongest statement the phase makes, and would be the first thing to fail if the band separation
stopped reconstructing exactly), and every reason code has a sentence in `docs/retouch.md`.

## 4. Performance (section 11)

| Row | Budget | This build |
|---|---|---|
| Retouch at full resolution (45 MP, GPU) | <= 350 ms | **waived** - no `wgpu` backend (ADR-0029 section 4) |
| Retouch at proxy (2048 px) | <= 45 ms | **waived as written** - the row is about applying a plan on a device |
| 1,000-image gallery at export | <= 7 min | 57.6 s extrapolated, on the processor path |
| Processor fallback (45 MP) | <= 4 s | **waived** - no camera file exists in this repository |
| Storage (not in section 11) | 1,000 B/image, self-imposed | 659 B/image measured over 1,000 photographs |

The measured figure is the **decision** rather than the application, and it includes at least one
full render because the texture guard is a post-condition. A frame that has to re-solve three
times costs four renders and is still inside the budget.

## 5. What this build's numbers are and are not claims about

They are claims about arithmetic: the detector's geometry and its colour test, the protect veto,
the band separation and its exact reconstruction, the texture floor, the re-solve ladder, the
withdrawal, the per-identity constancy, the store's two protections and the schema's refusals.

They are not claims about a photograph, for the four reasons in section 0. In particular:

* **the fixture skin is one reflectance with one pore pattern.** The thresholds are all relative
  to the skin they are measured on, which is what makes the mechanism tone-independent - and a
  mechanism being tone-independent is not the same as a detector being measured across tones;
* **the marks are the ones the generator knows how to paint.** A real face carries marks this
  detector has never seen;
* **nothing in this repository has been near a camera.**

## 6. Three things the tests found, recorded because they are the useful part

**A high-band transplant puts the blemish back.** The obvious composition - take the donor's tone
and put the *original* texture back - is what section 6.2's words read like, and it healed about a
third of a spot while reporting a texture ratio of one. The edge of a mark is high-frequency
content *of the mark*, so transplanting it transplants the mark. The fix is that both halves come
from the donor and the donor's texture is rescaled to the energy of the ring around the mark. A
retoucher that measures itself as perfect while doing almost nothing is the worst available
failure, because nothing downstream can see it.

**A luminance-only detector misses the most common blemish on a wedding face.** An inflamed spot is
often no brighter or darker than the skin it sits on - it is *redder*. The first detector
band-passed luminance, found every shadow edge, and walked straight past the fixture spot it
existed to find. It now band-passes the red share of the chromaticity as well, either signal
qualifying.

**A plain mean over a component reads a strong mark as a weak one.** A detected component includes
the falloff of the mark as well as its core, and a falloff sample is half skin - so painting the
fixture spot *brighter* made its temporary probability go *down*, because the brighter mark had
more falloff. The colour reading is now weighted by how far each sample departs from the skin.

Each of the three was found by a unit test that existed before the code was right, and each is
recorded in the module that carries it.

## 7. What was deliberately not built

**The learned inpainting network** of section 6.2. `InpaintMethod::Learned` is in the frozen enum
and this build never emits it: there is no such network here, and an operator that cannot run must
not be recorded as though it did. What ships is the healing-brush equivalent, which section 6.2
names for small marks and which is the right operator for every mark this phase is willing to
touch.

**Phase 19's shine reduction, again.** `RetouchOp::ShineReduce` is in the enum because section 5
freezes it and phases 21 and 22 will want it; this phase never emits one, and
`RetouchPlan::broken_guarantee` refuses a plan that carries one. Two phases reducing the same hot
spot is a forehead brought down twice.

**A ledger row per anomaly.** ADR-0043 section 9: these codes do not enter phase 13's reason
registry, for the reason phase 19 gave and with one addition - a protect row is recorded where a
person can see it, which is the part that actually matters.

## 8. Conditions

**C1 - the pipeline underneath is placeholder.** Phase 06's detector finds no faces and phase 18's
masks do not reach this pass, so on a real photograph nothing is retouched. **Sev 2.** Closes with
phase 05's C10 and with the `with_masks` wiring; the second is a small change that touches no
frozen shape.

**C2 - no per-skin-tone parity study.** Section 10.1 asks that no bucket be more than ten per cent
worse than the best. There are no labelled faces across tone buckets in this repository. The
mechanism is tone-relative by construction and `docs/skin-fairness.md` says so, but **no
per-bucket number is published and none should be inferred**. **Sev 2.**

**C3 - the two heads are untrained.** `BLEMISH_HEAD_TRAINED` and `PERMANENT_HEAD_TRAINED` are
false and neither head is consulted; every plan carries `head_untrained`. Closes when a corpus and
a GPU exist.

**C4 - no blind expert comparison.** Section 13's last criterion and section 10.1's last row.
Nothing in this phase may be described as at parity with Retouch4me, Evoto, Aperty or Portraiture
until retouchers have judged it.

**C5 - the desktop shell does not build here.** `ui/src-tauri` fails its build script for want of
`icons/icon.ico`, which is a pre-existing condition rather than one this phase introduced. The
eight new commands are wired in the same shape as phases 15 to 19's and are not compile-checked on
this machine. The workspace excludes `ui/src-tauri`, so no gate covers it either.

> **Update, at the merge onto main.** The icons are in place and the missing `fn main` was
> restored in phase 21, so the shell builds where a linker exists; `dlltool` is what is absent on
> this machine. `rustfmt` parses `main.rs` cleanly, which proves the syntax and not the types, and
> a symbol cross-check proves the names: 180 handler entries, 180 `#[tauri::command]` definitions,
> 180 `aura_app` functions the crate re-exports, 210 DTOs `contract::ipc` defines, and 180 typed
> client wrappers. That is the strongest statement available without a linker, and it is weaker
> than a build.

## 9. Rollback

Feature flag: `RetouchPass::enabled(false)`, or `RetouchPreset::Off` on a project. A disabled pass
still writes a plan per frame - one that does nothing - because a frame with no plan and a frame
the photographer switched off must not look the same in a coverage report.

Migration: reversible, and the rollback statements are at the top of
`crates/aura-catalog/migrations/0021_retouch.sql`. **Export `retouch_protected` where
`source = 'user'` and `retouch_identity` where `user_edited = 1` first.** Everything else in the
migration is recomputable from pixels; those two are not derivable from anything, and a
photographer telling the product to keep somebody's beauty mark is the most expensive data in it.

Models: `models.lock` pins both heads by sha256 and the manifest is signed. A rollback bumps
`model_ver`, raises `AURA-ML-5096` and re-plans in the background. It does **not** clear the
protect set.

## 10. What phase 21 inherits

* **`RetouchService` is the only way to ask what was done to somebody's skin.** Phase 21 retouches
  hair, teeth, eyes, clothing and glare, and must not re-smooth skin this phase worked on - the
  plan is where it finds out what happened.
* **The protect set is shared.** A mole is a mole whichever phase is looking at it, and phase 21
  reads `retouch_protected` rather than building its own.
* **The texture guard is the pattern for every later operator.** Measure through the real
  renderer, re-solve, and withdraw rather than ship. Phase 22's denoising and phase 24's
  generative fill both have the same failure mode and neither may assert its way past it.
* **The per-image allowance is still shared.** Phase 19 introduced it, phase 20 spent against it,
  and phase 21 is the eighth operation on the same budget rather than the first on a new one.
