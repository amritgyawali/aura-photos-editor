# AURA-ML-5111 - A change to a photograph's framing was refused

**Severity / recovery:** see `crates/aura-core/errors.toml` for the registered values.

## What the photographer sees

The crop they dragged, the aspect they picked or the acceptance they clicked did not stick, and a
message saying nothing was altered.

## What actually happened

One of four things:

* **There is no plan for that photograph yet.** `accept` and `set_override` both change a row
  that has to exist; a frame the pass has not reached has nothing to change.
* **The rectangle is not inside the photograph.** A crop whose corner is outside `0..1`, or one
  with no area. This is the only geometric refusal on the override path.
* **The aspect is not one of the five.** `original`, `4:5`, `5:4`, `1:1`, `16:9`. An unrecognised
  string is named rather than defaulted, because silently delivering the original framing is a
  crop nobody chose.
* **The change asks for nothing.** Every field absent. Use the revert if what was wanted was the
  original framing back.

**A photographer's own rectangle is not checked against the protected regions**, and that is
deliberate. The safety filter binds what automation may propose across four hundred frames; a
person cropping one photograph of their own may crop it as tightly as they like.

## What to do

1. If the plan is missing, run the pass over the project first.
2. If the rectangle was rejected, the panel's crop handles are clamped - a rectangle outside the
   frame means a caller other than the panel, or a stale photograph id.
3. `just phase-23-verify` exercises the accept, the override and the revert against a fixture
   catalog, including the refusal paths.
