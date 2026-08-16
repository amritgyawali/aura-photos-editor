# AURA-ML-5045 - One photograph's framing could not be judged

**Severity / recovery:** see `crates/aura-core/errors.toml` for the registered values.

## What the photographer sees

That photograph is left unmarked. It is not treated as clean or badly composed, and the
rest of the wedding continues.

## Common causes

The proxy or analyser could not produce a valid judgement, or a result failed validation
or storage. A missing pose or aesthetic head alone does not cause this error: those
failures write a visibly degraded row with `keypoints_unavailable` or
`aesthetic_unavailable`. A fatal failure writes no row so a later pass can retry honestly.

## Operator steps

1. Confirm the photograph can render at the composition proxy level.
2. Check the nested error code for preview, model, or database recovery steps.
3. Retry the single photo after the underlying problem is resolved.
4. Verify `composition_status.scored` increases and the card changes from “not checked”.

Do not insert a zero score or empty flag row; that would mean AURA checked the photograph
and found it clean.
