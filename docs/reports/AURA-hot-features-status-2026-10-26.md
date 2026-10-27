# AURA status - Hot Features, 2026-10-26

What has been achieved, what was done and fixed, what is still open, and what to do next. Covers
the Hot Features work (2026-10-25/26), the repository clean-up, and the all-tools test sweep.
The Match a look work of the same day is reported in [AURA-status-2026-10-26.md](AURA-status-2026-10-26.md).

## 1. What has been achieved

- **Every Hot Features tool works end to end in the real desktop app**: liquify background repair,
  face freckles and acne, remove glasses glare, train your AI look, AI colour match. Checked by
  `scripts/test-hot-features-desktop.py` (7 cases: undo/redo, preview equals export, originals
  untouched, 7 verified exports, no page errors).
- **Glare removal is a real reflection remover**, not a highlight squash: on a known reflection
  added to four real eyes it removes 11-85% of it, against 3-41% for the previous tool.
- **Glare can find reflections by itself**: "Find and remove reflections" proposes one glare edit
  per lens. On 64 held-out labelled proposals from 1,000 wedding photos: 78% precision, 66% of the
  true reflections found; no proposals on glasses-free portraits.
- **Background repair behind liquify no longer tears or doubles the background**: the background
  follows the subject at its edge and relaxes to straight, never folding.
- **Every colour control is tested at both ends with direction checks** on a colour chart in the
  real app (all 24 HSL controls, white balance, saturation, vibrance, exposure, contrast, tone,
  channel and tone curves, every colour-grading wheel, blending, balance, calibration, parametric
  curve, effects): all pass.
- **Every face and body reshape slider, and every skin, make-up, hair, attire, scene, subject and
  light value, is tested at both ends** on a real portrait in the real app, with undo checked.
- **CLAUDE.md is 111 lines instead of 2,420** (about 97% less loaded into every session); the map and
  the phase history moved verbatim to `docs/where-things-are.md` and
  `docs/history/current-state.md`.

## 2. What was done

| Area | Change | Where |
|---|---|---|
| Glare removal | Veil subtraction: scene estimated from around the selection, veil carried across enclosed dark gaps (iris), coating tint by robust median, frames kept exact | `crates/aura-render/src/glare.rs`, `fill.rs` |
| Glare finder | Lens-window search, skin-relative seeds with hysteresis growth, eye-opening geometry, lens core, crisp-edge and local-contrast tests, spectacle-rim evidence, overlap merge | `crates/aura-render/src/glare_find.rs`, `find_glare` in `crates/aura-app/src/portrait_commands.rs`, button in `NativeRetouchWorkspace.tsx` |
| Background repair | Layered resample with a filled background plate and a fold-free transition | `resample_layered`, `layer_relax` in `crates/aura-render/src/studio_finish.rs` |
| Shared tools | Push-pull fill, chamfer distance, enclosed-gap flood fill | `crates/aura-render/src/fill.rs` |
| Skin report | Adaptive notes no longer say "raised to 0%" | `crates/aura-app/src/portrait_features/expert.rs` |
| Jewellery stage | Glare strength 0.25 so the "about a sixth" promise holds | `crates/aura-app/src/advanced_retouch.rs` |
| Test harnesses | Hot Features, glare known-answer, glare finder scan, glare finder desktop, colour-values and finish-values groups in the all-tools harness | `scripts/`, `crates/aura-render/tests/` |
| Docs | Hot Features doc, CLAUDE.md split | `docs/hot-features.md`, `CLAUDE.md`, `docs/where-things-are.md`, `docs/history/current-state.md` |

## 3. What was fixed (found by testing)

1. **HSL luminance moved neutral greys by up to 55 levels** (orange band). A near-grey's hue is
   noise; the shifts now fade in with saturation, CPU and GPU shader alike
   (`tonemap::hsl`, `shaders/tone.wgsl`). The same defect was fixed in parallel on `main` (ramp
   3-15 %, engine string `+hsl.2`); on merging, `main`'s version was kept and `hsl-neutral.1` dropped.
2. **Dehaze showed nothing while the slider moved**: it was skipped on the interactive (fast and
   live) preview path as a "heavy" stage, though it reads the same small base as Clarity. It now
   runs on every path; restoration stays the heavy stage. Performance budgets still pass.
3. **Body sliders warped photographs with no people** (37-46% of a dolphin photo, 28-43% of a
   still life), and **subject exposure/warmth/clarity changed 92-99% of such frames**. Body and
   subject tools now require a detected (or drawn) face. Cache tag `people-gate.1` (body warps);
   `main`'s parallel fix `+faceless.1` empties the subject-selection regions too, and both are kept.
4. **Glare tool ignored the reflection under a feathered selection** (the reflection was part of
   its own "scene beneath" estimate).
5. **Background repair doubled background lines** at the subject's edge (old method) and left a
   seam at uncertain edges (first new version).
6. **Duplicate glare proposals on one lens** (a face parsed twice) removed a reflection twice.
7. Harness defects: undo depth for multi-step auto retouch, sidebar selector, cleanup on a full
   disk, a variable shadowing bug, dependent controls tested without their prerequisite.

## 4. What is remaining to fix

- **AI Color Match on a different scene** still fails two accuracy gates
  (`a_batch_matched_to_one_reference_converges_on_one_look`,
  `a_new_photograph_takes_an_edited_references_look_through_the_renderer`), as it did at commit
  8a7866f. A single edited reference cannot separate its grade from its scene's colours, and the
  tool's job (match appearance) differs from the gates' (recover a hidden edit). An additive-cast
  fix was tried and reverted because it made the real button do nothing on another scene.
- **Glare finder**: one proposal in five is still not a lens reflection (eyelid creases and eye
  make-up can imitate a rim); faint whole-lens sheen is not found; a reflection that hides the eye
  can move the eye landmarks and be missed.
- **Glare removal**: smooth skin shading under a reflection is flattened toward the surrounding
  tone; eye sockets much darker than the brow and cheek are the hard case (11-35% removed).
- **Background repair**: with a very large brush right next to the person most of the background
  sits in the transition and is only partly straightened.
- **The test-photo manifest is mislabelled**: the first "indian bride" is a pod of dolphins and only
  1 of 10 harness photos has a detectable face, so person tools are exercised on one portrait.
- **Lens corrections on JPEGs** show no change without a lens profile (expected: no profile, no
  coefficients); a measured profile library is still missing.
- **Autopilot** finishes "degraded" and leaves the cull for review, by design, because nothing in
  the build is calibrated (see `docs/history/current-state.md`, phase 13 and 28).

## 5. What needs to be done next

1. Add a few real portraits (several faces, full-length, glasses, beards, visible teeth) to the
   harness inputs so every person tool is tested on a subject it can act on.
2. Decide the product intent for AI Color Match on unrelated scenes (match appearance vs recover
   the edit) and then either re-scope or re-design the two gates.
3. Collect a labelled set of real glasses photos (reflections marked) to train or tune the glare
   finder's rim evidence beyond the 128 tiles used here.
4. ~~Show a sentence in the finish panel when body or subject controls cannot act because no face
   was found.~~ Done in #75: `save_studio_finish` returns a note the panel shows.
5. Keep the C: drive healthy (the Rust build folder reached ~132 GB once); build tests on F:.

## 6. Final test results

| Suite | Result |
|---|---|
| All-tools real-window harness (`scripts/test-everything-gui.py`, full run) | 721 checks: 648 pass, 51 refused by design, 20 warn, 2 fail - both fixed and re-verified below |
| Re-run of the two groups with the fixes (retouch tools, auto retouch) | 171 checks: 170 pass, 1 warn, 0 fail |
| Colour values (in the full run) | 122 checks, all pass |
| Face, body and finishing values (in the full run) | 225 checks: pass, or refused by design (negative values on one-sided 0..100 controls), or warn where the portrait has nothing for the control to act on |
| Hot Features real-window harness | 7 of 7 cases pass |
| Glare finder real-window test | found and removed on a real guest in glasses; nothing on a clean portrait; one Undo |
| Rust: aura-render, aura-recipe, aura-app (lib + integration) | 1,398 pass, 3 fail before the gate update: the two known colour-match gates, and the interactive-path gate updated for dehaze (now passes, 18/18) |
| Performance budgets (`aura-perf`, host scale 6) | all pass |
| UI (vitest) | 121 files, 1,148 tests pass |
| Lint and banned patterns | clippy clean on changed files, rustfmt clean, `check-banned.sh` clean |

The 20 warnings are all "nothing to act on in this photograph": a speaker behind a lectern (waist,
arms and belly hidden), one face (group balance and group skin), no beard, no visible teeth, no red
garment, no backlight, no lens profile for a JPEG, a still life Auto edit left as it was, and
Autopilot holding the cull for review. The full report is `docs/reports/AURA-full-test-2026-10-26.md`.
