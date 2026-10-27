# ADR-0140: Native fitting on the main reference-first screen

Date: 2026-10-23. Extends ADR-0139 without changing frozen gallery contracts.

## Context

The main reference-first screen calls `reference_style`, independently of the
studio colour-match and advanced lighting-bucket profiles. It previously required
eight folder references and evaluated only three scales of one aggregate-derived
guess. It used a camera preview to build the target correction, and saved a
reduced-resolution fit without native verification or a transactional stale-edit
check. Improvements to the separate distribution matcher did not fix this path.

## Decision

Add an explicit single-finished-photo route while retaining the gallery's minimum
reference count. Decode references with orientation and supported ICC conversion.
Add defaulted thumbnail, dimensions and monochrome evidence in version 2 of the
owned reference cache; continue reading version 1 caches. No frozen LookProfile
contract or database migration changes.

Share the bounded additive parameter search and Cartesian grade-wheel search.
Fit each target from its native decode and its own reconstructed correction or
profile baseline. Use strong structural correspondence only as a heuristic for
thumbnail scoring; use regularised aggregate appearance otherwise. Fit full
strength before scaling the result, protecting strength semantics.

Canonicalise and merge every candidate before rendering. Verify the merged
proposal at native resolution with clipping and per-person sampled colour guards.
Save only accepted changes using the transactional baseline hash. Carry
cancellation from the collection and unattended worker into candidate searches.

## Consequences

There is more compute per target; work is bounded by existing search budgets and
deduplicated by recipe hash. Tests must check actual saved pixels, repeated
application, zero restoration, manual protection, cancellation and RAW preview
independence. Correlation and aggregate distance do not establish universal grade
recovery. The experimental distribution matcher's separate unmet accuracy gates
remain unmet and are not relabelled as passes by this change.
