# AURA-ML-5008 - Inference exceeded its deadline

**Severity / recovery:** see `crates/aura-core/errors.toml` for the registered values.

## What the photographer sees

The registered sentence. The step is retried later in the run rather than abandoned.

## What actually happened

Interactive requests carry a deadline so that a slow model can never hold the interface. The deadline elapsed while the request was queued or running. On a saturated machine this is normal backpressure; repeated at idle it means a model is far slower than its card claims.

## What AURA does automatically

The request is cancelled at the next chunk boundary - cancellation is cooperative, so memory is released within one chunk - and the work is re-queued at batch priority. `infer.run` records the queue time separately from the run time, which is what tells the two causes apart.

## Operator steps

1. Compare `queue_ms` against `latency_ms` in the telemetry line. High queue time means contention; high latency means the model itself.
2. Contention: check whether a large batch job is running, and whether the machine fell back to the processor path (AURA-GPU-4001).
3. Model slowness: compare against the latency in the model card for this machine class. A model that is more than 50 per cent slower than its card is a card defect or a regression - report it with the benchmark output.

## Related

- Error registry: `crates/aura-core/errors.toml`
- Runtime ADR: `docs/adr/ADR-0007-inference-runtime.md`
