# AURA-GPU-4001 - No hardware accelerator available; the processor path was selected

**Severity / recovery:** see `crates/aura-core/errors.toml` for the registered values.

## What the photographer sees

The one-sentence message registered for this code, plus a "Processor" row in Settings > Hardware where they expected a graphics card. Analysis still completes; it takes longer.

## What actually happened

The hardware probe enumerated the execution providers named in `docs/adr/ADR-0007-inference-runtime.md` and none of the accelerated ones claimed the machine. On the current build this is the expected outcome on every machine, because no GPU backend is compiled in - see the ADR section "Execution providers on a machine with no GPU". It is also the correct outcome on a real machine with no discrete GPU, with an unsupported driver, or with a provider that was previously set aside by AURA-GPU-4002.

## What AURA does automatically

The plan is written with `ep_order = ["cpu"]`, batch sizes come from the processor column of the cost model, and `infer.plan_selected` is emitted with the probe scores. Nothing is refused and no feature is hidden: the processor path runs every model, in the quantised variant where the model's precision policy allows it.

## Operator steps

1. Open Settings > Hardware and read the reason recorded against each unavailable provider. "Not compiled in this build" is expected today; "driver too old" or "set aside after a failed check" is not.
2. If a supported card is present and the reason names a driver, update the driver and press Re-check hardware.
3. If the provider was set aside, follow `AURA-GPU-4002.md` before re-checking.
4. Record the processor throughput from `just bench-models` before escalating a "slow AI" report; the budgets in the phase document assume acceleration.

## Related

- Error registry: `crates/aura-core/errors.toml`
- Runtime ADR: `docs/adr/ADR-0007-inference-runtime.md`
- Hardware troubleshooting: `docs/runbooks/hardware.md`
