# AURA-ML-5091 - A crop or straightening override was refused

**Severity / recovery:** see `crates/aura-core/errors.toml` for the registered values.

## What the photographer sees

A crop they dragged, or an angle they set, that did not stick. The photograph is unchanged -
not partly changed, which is the point.

## What actually happened

`GeometryService::set_framing` refuses an override that
`GeometryOverride::problem` rejects:

* the rectangle covers no area;
* the rectangle leaves the frame;
* the angle is outside `-45..45` degrees, or is not finite.

The predicate lives in `aura-core`, beside the shape, and `aura_geometry::guard` turns it
into this error. The split is the one every phase since 09 has kept: the contract owns what a
sound value is, and the implementing crate owns the error registry - so the solver, the
store, the IPC layer and the evaluation harness cannot disagree about it.

The third case is the one that fires in practice, and it is usually a UI unit bug rather than
a photographer's choice: past forty-five degrees the axis being levelled is the other one,
and a straightening tool that accepts eighty-nine degrees is a straightening tool that turns
photographs onto their side.

## What to do

Re-frame within the image. If the intended crop genuinely leaves the frame, the photograph
needs phase 24's content-aware fill, which is out of scope here by section 2.2 - and until it
exists there is nothing to put in the corners.

## Note

**Reverting is not a refusal.** `GeometryOverride::revert` is a valid override whose
rectangle is the whole frame and whose angle is zero; it records that the photographer chose
the frame as shot, and it survives a re-analysis exactly as any other override does. A revert
implemented as *clearing* the row would be a revert the next pass undoes.
