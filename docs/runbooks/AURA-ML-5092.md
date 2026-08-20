# AURA-ML-5092 - One photograph's skin could not be retouched

**Severity / recovery:** see `crates/aura-core/errors.toml` for the registered values.

## What the photographer sees

One photograph carries no retouch. Everything else in the wedding is unaffected, and the frame
is still delivered - with phases 15 to 19's work on it and no skin work.

## What actually happened

Either the proxy would not decode, or the plan the solver produced broke one of the seven
guarantees `RetouchPlan::broken_guarantee` checks:

1. it carries no reason (invariant 2);
2. it carries more than `MAX_OPS` operations;
3. an operation is outside its own bounds - an under-eye lift above `MAX_UNDEREYE_LUMA_EV`, an
   evening operation naming a band other than the mid band, a strength outside `0..1`;
4. it carries phase 19's `ShineReduce`, which this phase never emits;
5. an operation overlaps a protected feature;
6. the texture report is incoherent - a pass below its own floor, a floor below
   `POLISHED_FLOOR`, more re-solves than allowed;
7. a withdrawn retouch still carries operations.

**A refused plan is stored as no plan rather than as a weak one**, and the next pass tries
again. Three of the seven describe a photograph that would look visibly retouched, which is the
failure this phase exists to avoid; the other four are ways a stored row would lie to phases 21,
25 and 27.

## What to do

1. Re-run the pass. The work remaining is a query, so a retry costs one frame.
2. If one frame fails repeatedly, the detail on the error names the guarantee. Number 5 in
   particular usually means the protect set and the detector disagree about a region, which is
   the conservative outcome working as intended rather than a bug.
3. If it is a decode failure, an `AURA-RAW-2xxx` code will be in the log beside it, and that is
   the real fault.
