# AURA-ML-5073 - A style training run was refused

**Severity / recovery:** see `crates/aura-core/errors.toml` for the registered values.

## What the photographer sees

"Teach My AI" stops before the progress bar starts and says AURA could not find enough
matching originals and finished photographs in that folder. No profile is created.

## What actually happened

The archive scan produced fewer than `aura_style::train::MIN_PAIRS` accepted pairs. This is a
refusal rather than a warning because the alternative is a profile fitted on a handful of
frames, which is section 12's fourth failure mode - "users expect one-click magic from 20
photos" - arriving as a shipped feature rather than as a support ticket.

The three causes, in the order they occur in practice:

1. **The folder has finals but no originals**, or the reverse. The scan needs both.
2. **The originals are in a RAW format this build does not decode.** See
   `docs/camera-support.md`; Canon CRX, Panasonic RW2 and compressed RAF are the open ones.
3. **Everything was rejected by the residual check**, which is `AURA-ML-5072` repeated and
   whose runbook covers it.

## Operator steps

1. Ask which folder was chosen and what is in it. `aura-cli` prints the file-type census the
   scan produced.
2. If both kinds are present, look at the per-pair rejections - the refusal carries the
   accepted and rejected counts in its context.
3. A run refused at, say, 240 accepted pairs is a *product* decision rather than a technical
   one. `MIN_PAIRS` is a named constant with a written reason; it is not tuned per customer.
4. Nothing is written on a refusal. Re-running after adding a second wedding is safe and does
   not duplicate work: matched pairs are keyed on their content hashes.
