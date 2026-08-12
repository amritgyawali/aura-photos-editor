# AURA-GPU-4004 - Hardware probe failed; a conservative plan was used

**Severity / recovery:** see `crates/aura-core/errors.toml` for the registered values.

## What the photographer sees

The registered sentence on first run, and a Settings > Hardware panel showing the processor path with a "measured: no" note.

## What actually happened

The probe has a hard ceiling of 15 seconds for the whole enumeration and micro-benchmark. If a provider hangs, or the plan cannot be written to disk, the probe is abandoned and the conservative plan is used instead: processor only, the smallest batch sizes in the table, and half the detected cores.

## What AURA does automatically

The conservative plan is used for the session but is not persisted, so the next launch measures again rather than inheriting a pessimistic guess. `infer.plan_selected` records `probed: false`.

## Operator steps

1. Re-check hardware from Settings. A one-off failure during a driver install is common and does not repeat.
2. If it fails repeatedly, check that the application data directory is writable; a read-only profile directory presents exactly this way.
3. Attach the log lines for the probe span to the report. Each provider is timed individually, so the one that hung is named.

## Related

- Error registry: `crates/aura-core/errors.toml`
- Hardware troubleshooting: `docs/runbooks/hardware.md`
