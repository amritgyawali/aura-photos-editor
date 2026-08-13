# AURA-ML-5013 - The embedding model returned an unusable vector

**Severity / recovery:** see `crates/aura-core/errors.toml` for the registered values.

## What the photographer sees

The registered sentence. The photograph stays in the grid, stays selectable and exports exactly as it would have. What it does not do is appear in "find similar", in a burst group, or in a duplicate pair.

## What actually happened

`aura_vision::embed::finish` refused a vector that says nothing: either every one of its 512 components was zero, or one of them was not a finite number.

This refusal is the point. An all-zero vector is not a neutral answer - after L2 normalisation it stays zero, sits at cosine distance 1.0 from every other frame in the wedding, and therefore looks exactly like a legitimately unusual photograph. Storing it would mean one frame silently never grouping with anything, with no record of why. Invariant 9 forbids that shape of failure.

Three things produce it in practice:

* a preview that decoded to a uniformly black or blown frame, so every convolution output was clamped to zero;
* an int8 variant whose quantisation collapsed a very low-contrast frame;
* a genuine runtime fault, in which case an `AURA-ML-5xxx` from `aura-infer` is logged immediately before this one.

## What AURA does automatically

The photograph is skipped and counted in `EmbedReport::failed`. No row is written to `embeddings` or `descriptors`, so the next embedding pass will pick it up again - the pending set is a query, not a journal, and a frame with no row is always pending.

Nothing else in the run changes. One frame out of 4,000 is an item failure by design.

## Operator steps

1. Open the photograph. A frame that is genuinely black or genuinely blown is not a bug, and this code is the correct outcome for it.
2. Check whether an `AURA-RAW-2xxx` was logged for the same photograph. If the preview is wrong, the embedding is downstream of the real problem; fix the decode first.
3. Re-run the analysis pass. It costs one frame, not the wedding.
4. If the same frame fails on the int8 variant and succeeds on fp32, record it: that is a quantisation finding for the model card's known failure modes, and MLL owns the decision about whether the model's precision policy should forbid int8.

## Related

- Error registry: `crates/aura-core/errors.toml`
- Model card: `docs/model-cards/wedding_embedding.md`
- Preview troubleshooting: `docs/runbooks/previews.md`
