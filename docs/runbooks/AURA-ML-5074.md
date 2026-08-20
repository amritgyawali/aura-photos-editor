# AURA-ML-5074 - A style bucket had too few pairs and borrowed from its parent

**Severity / recovery:** see `crates/aura-core/errors.toml` for the registered values.

## What the photographer sees

The bucket matrix shows a cell in the "borrowed" colour rather than the "taught" one, the
profile report names it under *weak buckets*, and the recommendation says which wedding to
add.

## What actually happened

PHASE-17 section 6.2 shrinks every bucket toward its scene group and every group toward the
global profile by `n / (n + k)`, so a bucket with few samples contributes little of its own.
When the resulting confidence is below `aura_core::contract::style::APPLY_ABOVE`, the bucket
does not answer at all and the parent does - `FallbackLevel::Group` or `FallbackLevel::Global`
is recorded on the decision and counted in the `style.bucket_fallback` telemetry event.

**This is the design working.** A bucket with eight pairs that decided on its own would
produce a visibly different look from the bucket next to it, on evidence nobody would call
sufficient. The alternative considered - a hard minimum-sample cut-off - was rejected because
it is a cliff (ADR-0035 decision 5).

## Operator steps

1. Read the profile report's recommendation. It is generated from the actual gap and names one
   wedding to add.
2. Check `StyleOutline::bucket_ratio`. A project below about 0.3 has had its scene conditioning
   do very little, which is worth telling the photographer plainly: they are currently getting
   close to a single global style.
3. This code is never a reason to lower `APPLY_ABOVE` for one customer. It is a named constant
   with a written reason and changing it changes every profile in the product.
4. Eighty buckets over three hundred pairs is under four each by construction. A photographer
   who shoots one kind of wedding will legitimately have twelve populated buckets and
   sixty-eight empty ones, and that profile is not weak - `StyleProfile::strength` measures
   coverage against twelve rather than against eighty for exactly this reason.
