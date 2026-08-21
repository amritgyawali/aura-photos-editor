# AURA-ML-5097 - One photograph's small fixes could not be worked out

**Severity / recovery:** see `crates/aura-core/errors.toml` for the registered values.

## What the photographer sees

One photograph has no micro-retouch plan. It is delivered exactly as phases 14 to 20 left it, and
the rest of the wedding is unaffected.

## What actually happened

Either the proxy would not decode, or `MicroPlan::broken_guarantee` refused the plan the solver
produced. **A refused plan is stored as no plan rather than as a weak one**, because every
guarantee it checks describes either a photograph that would look worked on or a stored row that
would lie to phases 25, 27 and 28 about what happened to somebody's face.

The guarantees, and what each one means when it fires:

| Guarantee | What went wrong |
|---|---|
| a plan with no reason | invariant 2 was broken; a solver path returned without recording why |
| operations above `MAX_OPS` | eighty small fixes on one frame is a frame that needed a different exposure |
| budget outside `0..1` | the shared allowance accounting is wrong |
| an operation outside its own bounds | a solver produced a magnitude above a contract ceiling |
| an operation the matrix forbade | **the most serious one**: a delivery a studio did not agree to |
| a withdrawn family still carrying operations | the guard and the plan disagree about what shipped |
| an incoherent naturalness report | a measurement is outside its own range |

## What to do

1. Read the detail line. It names exactly one of them.
2. "an operation ran while the matrix forbade it" is a **bug, not configuration**. It should be
   unrepresentable; if it appears, the plan-building path has a branch that skips the matrix, and
   it needs a fix rather than a retry.
3. The others are safe to retry: the pass is resumable and the frame is pending by definition.
4. If a whole project fails, the proxies are the more likely cause - check `AURA-CACHE-*` and
   `AURA-RAW-*` in the same run.
