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

Phase 18's segmenter is a placeholder and no mask coverage is wired into this pass, so on a real
photograph there is nothing to intersect. **This build therefore proposes no removals on a real
photograph at all**, which is the correct behaviour and not a limitation to work around. It is
condition C1 of the phase 24 exit report.

## The second reason, which survives a trained segmenter

There is a further and more durable cause, found while building this phase and recorded here
because it will still be true on the day phase 18's model is trained.

**Phase 18's twenty mask classes contain no word for a ring or a cake.** `Protected::ALL` names six
kinds; `Face`, `Skin` and `Dress` map onto phase 18's vocabulary exactly, `Hands` maps onto `Skin`
(a superset, which can only refuse more than asked), and `Rings` and `Cake` map onto nothing at all.

A coverage assembled from phase 18 is therefore never *complete*, and a candidate that clears every
kind the segmenter could look for still comes back `Unknown` rather than `Clear`. Treating it as
clear would be the same mistake this whole error code exists to prevent, made one level up and much
harder to see: the product would be claiming a region is free of the rings on the strength of never
having looked for them.

`Coverage::partial` is the shape that carries this, `Coverage::is_complete` is what
`CleanupOutline::mask_covered` counts, and `api::coverage_from_masks` is the one place the two
vocabularies meet.

**What closes it** is a mask class for rings and one for cake, which is a phase 18 change rather
than a phase 24 one. Until then `mask_covered` is zero on every project, and that is the honest
figure rather than a bug in this phase.

## What to check

`CleanupOutline::mask_covered` is the number to read. At zero, the project's blocked histogram says
nothing about what is in the photographs - every candidate was blocked for want of evidence rather
than for want of safety.
