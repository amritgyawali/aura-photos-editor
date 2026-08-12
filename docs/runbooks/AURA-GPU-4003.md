# AURA-GPU-4003 - Accelerator memory budget reached; the batch size was reduced

**Severity / recovery:** see `crates/aura-core/errors.toml` for the registered values.

## What the photographer sees

The registered sentence, once per model per run. Throughput drops; nothing fails and no image is skipped.

## What actually happened

The scheduler keeps a ledger of the memory each loaded session declared at warmup and never admits work that would exceed the budget - 70 per cent of free accelerator memory by default. When a request does not fit, the batch is halved and retried, down to a batch of one. The successful size is remembered per model per machine, so the downshift happens once rather than on every batch.

## What AURA does automatically

`infer.oom_downshift` is emitted with the model, the previous batch and the new batch. The job continues with the smaller batch. Results are identical: batching changes throughput, never numbers.

## Operator steps

1. Confirm from the telemetry line that the downshift settled. A single downshift is healthy; one on every batch means the remembered size is not being persisted, which is a defect worth reporting.
2. Close other applications holding graphics memory - browsers with hardware acceleration are the usual cause - and re-run.
3. If a machine downshifts to a batch of one on a model that should fit, capture `hardware_plan.json` from the support bundle: the declared working set for that model is probably wrong, which is a model-card defect.

## Related

- Error registry: `crates/aura-core/errors.toml`
- Runtime ADR: `docs/adr/ADR-0007-inference-runtime.md`
