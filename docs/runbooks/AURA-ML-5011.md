# AURA-ML-5011 - Inference cancelled by the photographer

**Severity / recovery:** see `crates/aura-core/errors.toml` for the registered values.

## What the photographer sees

The registered sentence. This is not a fault: it is the answer to a Stop button, and it is registered so that the reason a batch ended is always recorded rather than inferred.

## What actually happened

A cancellation token was set while requests were queued or executing. Cancellation is cooperative and is checked at chunk boundaries, so a running chunk finishes - typically single-digit milliseconds - and everything after it is dropped.

## What AURA does automatically

Queued requests are discarded, accelerator memory is released within one chunk, and finished work stays finished. Nothing is recomputed when the job is resumed, because every completed inference has already been recorded.

## Operator steps

1. Nothing. If cancellation takes longer than 250 milliseconds, that is a defect against Article VIII rule P9 and is worth reporting with the log timestamps.
2. If a cancelled job resumes by recomputing work that had already finished, that is a resumability defect - Article X rule R2 - and is more serious than the slow cancel.

## Related

- Error registry: `crates/aura-core/errors.toml`
