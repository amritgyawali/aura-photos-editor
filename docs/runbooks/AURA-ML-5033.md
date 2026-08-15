# AURA-ML-5033 - Stored technical verdicts were made by a different build

**Severity / recovery:** see `crates/aura-core/errors.toml` for the registered values.

## What the photographer sees

The registered sentence, once, while the affected frames are re-checked in the background. Sharpness marks, blink marks and exposure marks stay visible and keep working while it happens. Anything they dismissed by hand is kept.

## What actually happened

`image_integrity` carries three version columns and they invalidate three different things.

| Column | What it is | What a bump invalidates |
|---|---|---|
| `model_ver` | the focus head and the eye-state head, shipped as one pack | `subject_sharpness`, every row in `face_eye_state`, and therefore the eye flags |
| `analysis_ver` | this build's arithmetic - motion, exposure, noise, flags, scoring | the motion kind, the exposure verdict, `noise_sigma_rel`, `flags` and `technical_score` |
| `calib_ver` | `camera_calibration.toml` | every *normalised* number: subject sharpness, background sharpness, the noise figure and the exposure headroom |

The fifth version-drift code in the product, after `AURA-ML-5015` (embeddings), `AURA-ML-5018` (faces), `AURA-ML-5022` (scenes) and `AURA-ML-5028` (moments), and it exists for the reason all four of those do: **a number produced under one version and a number produced under another are not comparable, and comparing them returns a plausible answer that means nothing.**

The specific harm here is concrete rather than theoretical. A `calib_ver` bump that raised the expected MTF50 for a body makes every frame from that body look softer; mixing the two vintages inside one wedding sorts a review queue by which day the frame was analysed on.

## What AURA does automatically

Re-analyses the stale rows in the background at `Priority::Background`, oldest first. `IntegrityOutline` reports the **lowest** version present, so a caller drawing a conclusion over a mixed set finds out before it draws it.

`user_reviewed = 1` rows are re-analysed like any other, and the dismissal is replayed onto the fresh verdict rather than dropped. A photographer who said "this is not soft" said it about the photograph, not about a build.

## Operator steps

1. `SELECT model_ver, analysis_ver, calib_ver, COUNT(*) FROM image_integrity WHERE project_id = ? GROUP BY 1,2,3;` - the group with the low numbers is the stale set.
2. Compare against the running build: `aura_brain_photo::integrity::MODEL_VER`, `ANALYSIS_VER`, and the `version` key at the top of `crates/aura-brain-photo/config/camera_calibration.toml`.
3. If only `calib_ver` moved, the re-analysis is cheap - the pixels are not re-read for the classical measures on a cached proxy, and no model runs.
4. If `model_ver` moved, the full pass runs. Budget from section 11: 220 ms per image on the processor path.
5. Nothing needs deleting. The pass is an `INSERT OR REPLACE` keyed on the photograph.

## When this is not the problem

A wedding imported before phase 09 shipped has no `image_integrity` rows at all. That is not drift; it is an unanalysed project, and the coverage figure in the Integrity panel says so.

## Related

* `AURA-ML-5015`, `AURA-ML-5018`, `AURA-ML-5022`, `AURA-ML-5028` - the same code for embeddings, faces, scenes and moments.
* `AURA-ML-5037` - the calibration table has no row for a body, which is a *gap* rather than a drift.
