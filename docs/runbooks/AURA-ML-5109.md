# AURA-ML-5109 - One photograph could not have its restoration worked out

**Severity / recovery:** see `crates/aura-core/errors.toml` for the registered values.

## What the photographer sees

One photograph with no restoration decision on it. Everything else in the wedding is unaffected,
and the frame still renders - it simply renders without denoising or sharpening.

## What actually happened

One of five things:

* the 2048 px proxy would not decode, which is a phase 02 problem rather than a phase 22 one;
* phase 09 has no integrity verdict for the frame, so there is no measured noise to choose a
  tier from - the plan records `restore_no_noise_reading` and does nothing rather than guessing;
* the plan the solver produced broke one of the nine checks in
  `RestorePlan::broken_guarantee`, which is a defect in this crate and is refused rather than
  stored;
* the self-check could not be measured, because the render the plan was applied through failed;
* the catalog write failed, which surfaces as `AURA-DB-3006` underneath this code.

**No row is written in any of the five cases.** A written-but-empty plan would read to phases 25,
27 and 28 as "AURA decided this photograph needed nothing", which is a different and much worse
statement than "AURA has not looked at this photograph yet". The next pass tries again.

## What to do

1. Re-run the pass. The failure is per-frame and the pass is resumable.
2. If the same frame fails twice, check whether it has a phase 09 verdict:
   `aura-cli verify --phase 09` reports coverage.
3. If the message names a broken guarantee, that is a defect in `aura-restore`, and the sentence
   in the log names which of the nine checks failed.
