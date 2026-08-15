# AURA-ML-5036 - The camera calibration table was refused

**Severity / recovery:** see `crates/aura-core/errors.toml` for the registered values.

## What the photographer sees

The registered sentence, and no technical marks anywhere. This is the one failure in phase 09 that stops the pass rather than degrading it.

## What actually happened

`camera_calibration.toml` did not parse, or one of its rows broke a rule the loader enforces:

* `version` missing, zero, or not an integer;
* a body with `expected_mtf50` outside `0.05..0.95`, or `read_noise_e` at or below zero;
* a body with `highlight_headroom_ev` outside `0.0..2.5` - a body claiming three stops of recoverable headroom is a typo, and believing it would call a blown frame recoverable;
* a duplicate `make`/`model` pair, which would make the answer depend on file order;
* the `[fallback]` section missing, which is the row every uncalibrated body uses.

## Why this halts when almost nothing else in this phase does

Same argument as `AURA-ML-5024` for scene profiles and `AURA-ML-5031` for moment profiles, and it applies more strongly here.

A **half-loaded calibration table silently changes every technical number in the product**. Sharpness is normalised by the expected MTF50 for the body, noise is normalised by the read noise, and the exposure verdict is decided against the measured headroom. A table that loaded nine bodies out of twenty would judge a wedding shot on two bodies by two different standards and produce a review queue sorted by camera - which looks exactly like "the product hates this camera" and nothing like a config error.

So the loader refuses, leaves the previous table in place, and reports.

## What AURA does automatically

Halts the integrity pass. Everything already analysed stays; nothing new is written. The rest of the product - grid, previews, faces, scenes, moments - is unaffected, because none of them read this file.

## Operator steps

1. The message names the file, the key and the rule, in that order, because that is the order they are fixed in.
2. Compare against the shipped copy at `crates/aura-brain-photo/config/camera_calibration.toml`. The shipped file is embedded in the binary; only an installation override can be broken this way.
3. Delete the override to fall back to the embedded table. That is the fastest fix and it costs only the local measurements.
4. If the **embedded** table is what was refused, the build is broken - `cargo test -p aura-brain-photo calibration` catches this in CI, so a binary that reaches a photographer with a bad embedded table should be impossible.

## When this is not the problem

A body simply missing from the table is `AURA-ML-5037` and it degrades. This code is only for a table that could not be trusted as a whole.

## Related

* `AURA-ML-5024` - a refused scene profile or ritual taxonomy.
* `AURA-ML-5031` - a refused moment profile.
* `AURA-ML-5037` - a body with no row.
