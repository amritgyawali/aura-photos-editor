# AURA-GPU-4005 - Saved hardware plan was unreadable and the machine was measured again

**Severity / recovery:** see `crates/aura-core/errors.toml` for the registered values.

## What the photographer sees

The registered sentence, once, on a launch that takes a few seconds longer than usual.

## What actually happened

`hardware_plan.json` failed to parse, or carried a schema version this build does not understand. The plan is a cache of measurements, never a source of truth, so the correct response is to discard it and measure again rather than to refuse to start.

## What AURA does automatically

The unreadable file is replaced by a freshly probed plan, written atomically. Any set-aside provider list inside the old file is lost, which means a previously set-aside provider is re-tested once - by design, since the alternative is honouring a list we cannot read.

## Operator steps

1. Nothing, if it happened once after an update: a schema change does exactly this.
2. If it repeats on every launch, the application data directory is probably not being written - check permissions and free space.
3. Do not restore an old plan file from a backup. Measurements are cheap; a stale plan is not.

## Related

- Error registry: `crates/aura-core/errors.toml`
- Hardware troubleshooting: `docs/runbooks/hardware.md`
