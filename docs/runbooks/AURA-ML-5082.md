# AURA-ML-5082 - A mask edit was refused

**Severity / recovery:** see `crates/aura-core/errors.toml` for the registered values.

## What the photographer sees

The brush stroke or the algebra operation does not take effect and the panel says so. The mask
is exactly as it was.

## What actually happened

`edit_mask` was given a composition it could not run. The three ways that happens:

* an operand naming a mask that is not in the store - usually a stale id from a panel that was
  open across a re-analysis;
* a stroke plane whose dimensions are zero;
* a program whose stack underflows, which means the ops arrived in an order that is not a valid
  postfix program.

In every case `MaskService::compose` returns the **empty mask** rather than a partial result.
That is deliberate: the empty mask is the identity for union and the annihilator for
intersection, so applying it changes nothing, where a full-frame mask would have applied the
edit to the whole photograph.

## Operator steps

1. Close and reopen the mask panel. A stale id resolves on reload.
2. If it repeats, the composition is being built wrongly - `aura-cli verify --phase 18` runs the
   algebra against fixtures and will fail on the same program.

## What would make this impossible

A `compose` that returned a `Result`. Section 5 of the phase document freezes it returning a
`Mask`, and making the implementation *total* rather than amending the signature is what
ADR-0037 records. The empty mask is what makes totality safe.
