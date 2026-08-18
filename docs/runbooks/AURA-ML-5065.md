# AURA-ML-5065 - No skin reference was available, so the colour was set without one

**Severity / recovery:** see `crates/aura-core/errors.toml` for the registered values.

## What the photographer sees

Those photographs carry the `skin_locus_unavailable` reason and a lower white-balance
confidence, and many of them reach the review queue.

## What actually happened

Section 6.2 scores every illuminant hypothesis by how plausible it makes the *skin* of known
identities, and section 6.3 makes that a hard constraint on the solve. Both need a
`SkinLocus`, which is accumulated from a person's own well-lit frames across the wedding and
does not constrain anything below `MIN_LOCUS_SAMPLES` frames. A weak locus is worse than no
locus, because it looks like evidence.

The three ways to get here, in order of how often they happen:

1. **The face pass has not run, or found nobody.** Until phase 06's detector is trained this
   is the usual cause and it is a known condition, not a fault - see the phase 06 exit
   report's condition C1.
2. **The wedding genuinely has no well-lit frames of anybody yet.** Early in an import, the
   first frames analysed can all be from the darkest part of the day.
3. **The identities are all new.** A locus is per identity; merging two identities in the
   People panel merges their evidence and can push a person over the threshold.

## Operator steps

1. Check `PeopleService::hierarchy` coverage first. A wedding with no faces has no loci by
   construction and the fix is not in this phase.
2. Let the pass finish. Loci are accumulated across the whole project and the second pass
   over the same wedding is materially better than the first for exactly this reason.
3. Confirm in the review queue rather than in the log: the frames this affects are the ones
   the queue is for.
