# AURA-GPU-4002 - Execution provider failed its check and was set aside for this machine

**Severity / recovery:** see `crates/aura-core/errors.toml` for the registered values.

## What the photographer sees

The registered sentence, and a provider marked "set aside" in Settings > Hardware with the date it happened. Work continues on the next provider in the order.

## What actually happened

Every candidate provider runs a small reference model and is judged on two things: it must not crash, and its output must match the processor result within 1e-3. A provider that fails either test is written into this machine's set-aside list inside `hardware_plan.json`, with the reason and the plan revision that recorded it. This is nearly always a driver defect rather than a model defect: the same model on the same file passes on the next path.

## What AURA does automatically

The provider is removed from `ep_order` for this machine only, the next one is selected, and `infer.plan_selected` is re-emitted with the new order. The list survives restarts, so a machine does not re-crash on every launch. It is cleared by an explicit Re-check hardware, never silently.

## Operator steps

1. Read the recorded reason. `mismatch` means wrong numbers, `crash` means the provider aborted, `timeout` means it never returned.
2. Update the graphics driver, then press Re-check hardware in Settings. That clears the entry for that provider and re-runs the check.
3. If it fails again after a driver update, leave it set aside and attach the plan file from the support bundle to the report. A per-driver denylist entry may then be shipped in `models.lock`.
4. Never edit `hardware_plan.json` by hand on a customer machine; press Re-check instead, so the new plan is written atomically.

## Related

- Error registry: `crates/aura-core/errors.toml`
- Runtime ADR: `docs/adr/ADR-0007-inference-runtime.md`
- Hardware troubleshooting: `docs/runbooks/hardware.md`
