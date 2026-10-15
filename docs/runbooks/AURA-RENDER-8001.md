# AURA-RENDER-8001 - GPU output unavailable; the CPU path was used

**Severity / recovery:** see `crates/aura-core/errors.toml` for the registered values.

## What the photographer sees

AURA is using CPU output. The original photograph and saved recipe are unchanged. The
CPU remains the colour reference; a qualified GPU can differ by output-code rounding within
the documented tolerance. Other editing stages retain their existing implementations.

## Why AURA chose the CPU

A hardware adapter may be absent, explicitly disabled, fail the colour-reference check,
fail a driver operation, or be slower than CPU output once transfer costs are included.
The qualified path is described in [ADR-0122](../adr/ADR-0122-qualified-gpu-output-and-cpu-fallback.md).

Initialization and device failures are logged under `render.gpu` with this degraded code.
A failed device is disabled for the rest of the session, and output restarts on the CPU
from unchanged input pixels. Small frames also use CPU output below the measured crossover.

## Operator steps

1. Read `render_caps` for the backend, engine identity and fallback reason.
2. Check the `render.gpu` log for adapter, qualification and driver errors.
3. Remove `AURA_DISABLE_GPU=1` and restart to allow automatic detection again.
4. If the driver failed, restart after updating it. CPU processing remains available.
5. Reproduce GPU parity with `cargo test -p aura-render --test gpu_output -- --ignored
   --test-threads=1 --nocapture`. Do not widen the qualification tolerances to hide a mismatch.
