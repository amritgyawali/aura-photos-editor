# AURA-ML-5059 - A support bundle was refused

**Severity / recovery:** see `crates/aura-core/errors.toml` for the registered values.

## What the photographer sees

No file was written and nothing was sent anywhere.

## What it means

A support bundle is an anonymised slice of the decision ledger plus the config and model
versions. It is refused in two cases:

1. **The wedding has no recorded decisions.** An empty bundle is worse than none: it looks
   like evidence that nothing happened, when what happened is that nothing was recorded.
2. **The project does not exist**, or its ledger cannot be read.

## What a bundle contains, and what it cannot

Every identifier is replaced with a short handle - `image_0001`, `decision_0007` - assigned
in first-appearance order and never stored, so the structure survives and the wedding does
not. Reason codes, weights, evidence *rectangles*, confidences and version lists travel.

There is no code path that can put a photograph in a bundle. `Evidence` has no variant that
can hold image bytes, so the guarantee is a property of the shape rather than of the
exporter - and `tests/eval/explain_eval.rs` scans the finished file for key prefixes and
path shapes anyway.

## Operator steps

1. For the empty case: run the cull first. A wedding that has not been culled has decided
   nothing.
2. For the missing-project case: check the project id.
3. Before sending, open the file. It is JSON and it is meant to be read; a photographer who
   can see what is in it is a photographer who will send it.
