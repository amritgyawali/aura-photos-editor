# AURA-ML-5037 - This camera body has not been calibrated

**Severity / recovery:** see `crates/aura-core/errors.toml` for the registered values.

## What the photographer sees

The registered sentence, once per unknown body, in the Problems panel. Every photograph from that body is still checked and still marked; the marks are simply made more cautiously, and the Integrity card for those frames carries an `uncalibrated` note among its reasons.

## What actually happened

`camera_calibration.toml` has no row for the make and model in the EXIF, so the `[fallback]` row was used. Section 12's fourth failure mode, and it is expected rather than exceptional: a new body ships every few months and this product cannot have measured it in advance.

The fallback normalises by **sensor resolution alone**: expected MTF50 scaled from the pixel count, a read-noise figure interpolated from bodies of similar vintage, and one stop of highlight headroom, which is the conservative end of what modern sensors have.

## What that costs, precisely

Three things, and all three are in the safe direction:

1. **`confidence` is reduced** on every verdict from that body. A verdict drawn against a guessed calibration says so.
2. **The soft threshold moves away from flagging.** The uncalibrated path requires a bigger sharpness deficit before it will raise `SUBJECT_SOFT`, because the phase's documented failure mode is false rejection and a guessed baseline is exactly where a false rejection would come from.
3. **The exposure verdict cannot say `Recoverable` on the strength of headroom alone.** With no measured headroom, a clipped highlight is `Marginal` rather than `Recoverable`.

Cross-camera fairness (section 10.1's last gate, 0.05 between a 24 MP and a 61 MP body on the same scene) is measured against *calibrated* bodies. The fallback keeps two uncalibrated bodies consistent with each other; it does not promise the same tightness against a calibrated one.

## What AURA does automatically

Uses the fallback, emits `integrity.camera_uncalibrated {make, model}`, and lists the body in `IntegrityOutline::uncalibrated`. The wedding is fully analysed.

## Operator steps

1. `SELECT DISTINCT make, model FROM camera WHERE camera_id IN (SELECT camera_id FROM photo WHERE project_id = ?);` and compare against the table.
2. Adding a row is the fix, and it is a measurement rather than a guess. The harness is `cargo test -p aura-brain-photo --test calibration_harness -- --ignored`, which takes a slanted-edge target and an ISO ladder from the body and prints a row ready to paste.
3. Bump the table's `version` when a row is added. That is a `calib_ver` bump, so every affected verdict is re-analysed - see `AURA-ML-5033`.
4. Do **not** copy a row from a similar body. Two bodies from one manufacturer with the same pixel count can differ by a third of a stop of headroom, and a wrong measured row is worse than an honest fallback because it does not lower the confidence.

## When this is not the problem

A body with a row that still scores badly is a calibration that is wrong rather than missing. Check `expected_mtf50` for that row against a known-sharp frame before assuming the analyser is at fault.

## Related

* `AURA-ML-5036` - the table itself was refused.
* `AURA-ML-5023` - the same shape for a scene with no profile.
* `docs/camera-support.md` - which bodies decode, which are calibrated.
