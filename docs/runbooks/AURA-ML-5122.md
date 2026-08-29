# AURA-ML-5122 - Nothing could be shown safe to remove, because the regions that prove it are absent

**Severity / recovery:** see `crates/aura-core/errors.toml` for the registered values.

## What the photographer sees

No proposals at all on the affected photographs, with the Cleanup panel saying AURA cannot yet tell
where people, dresses and rings are. The photographs are unchanged and fully usable.

## What actually happened

The semantic denylist works by intersecting a candidate region with the masks phase 18 produces for
faces, skin, hands, dress, rings and cake. Those masks did not arrive.

**An absent mask fails the denylist check rather than passing it.** This is the sentence to
understand, because it inverts what phases 19 to 23 do: those phases *gated* an operation down when
an input was missing, and the safe direction was less. Here the safe direction is none, and
"gated to zero" and "blocked" would look identical in a panel while meaning completely different
things - one says the product checked and found nothing to worry about, the other says it could not
check. Only the first is a claim. ADR-0049 section 3.

## Why this is expected in this build

Phase 18's segmenter is a placeholder and `MaskField` is not wired into this pass, so on a real
photograph there is nothing to intersect. **This build therefore proposes no removals on a real
photograph at all**, which is the correct behaviour and not a limitation to work around. It is
condition C1 of the phase 24 exit report.

## What to check

`CleanupOutline::mask_covered` is the number to read. At zero, the project's blocked histogram says
nothing about what is in the photographs - every candidate was blocked for want of evidence rather
than for want of safety.
