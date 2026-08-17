# AURA-ML-5054 - A decision was refused because it could not explain itself

**Severity / recovery:** see `crates/aura-core/errors.toml` for the registered values.

## What the photographer sees

Nothing changed. The decision this code names was **not applied and not recorded**, and the
rest of the pass carried on. In the Explain panel the subject shows whatever was decided
about it last.

## What it means

Invariant 2: every AI decision carries a confidence and reasons, and a decision without an
explanation is a bug. Phase 13 is where that stops being a convention. `aura-explain`
refuses to write a decision in two cases:

1. **No reasons at all**, or every reason with an empty sentence.
2. **A reason whose code is not in the shipped registry.** The registry is assembled from
   the frozen vocabularies of phases 09 to 12 plus phase 13's own five codes; see
   `docs/reason-codes.md`, which is generated from it.

## Which of the two it is

The `detail` says. "carried no reason" is the first; "cited reason code" is the second, and
the message names the code.

## Operator steps

1. **Case 2 is almost always a code nobody registered.** A deciding phase that adds a
   variant to its own enum is picked up automatically, so this failure means a code was
   constructed as a free string somewhere instead. Find the construction site and use the
   enum.
2. **Case 1 is a deciding phase with an empty reason path.** Look for a branch that returns
   a verdict without pushing a reason - usually an early return for a degenerate input, a
   moment of one frame, or an empty candidate list.
3. Re-run the pass. Nothing needs cleaning up: the refused decision was never written, so
   there is no half-recorded row to remove.

## What not to do

Do not add a placeholder reason to make the refusal go away. A row saying "decided" with the
sentence "no reason given" is exactly the row a support case finds when it is looking for
the reason, and it is worse than no row at all.
