# AURA-ML-5007 - Input tensor shape does not match the model contract

**Severity / recovery:** see `crates/aura-core/errors.toml` for the registered values.

## What the photographer sees

The registered sentence. One photograph is set aside on that step; the batch continues.

## What actually happened

Every model entry in `models.lock` declares its input shape, layout, value range and colour space. The tensor handed to `InferService` disagreed with the declaration - the usual cause is a proxy that was produced at an unexpected size, or a caller that forgot the batch dimension.

## What AURA does automatically

The request is refused before execution, so no partially-shaped tensor is ever fed to a graph. The photograph is recorded in the Problems list with this code and the run continues.

## Operator steps

1. The log line carries both shapes: expected from the manifest, actual from the caller. The mismatch is usually visible immediately.
2. If the actual shape has the right pixels but the wrong layout, the caller is passing NHWC where the manifest declares NCHW; that is a caller defect, not a model defect.
3. If one photograph out of thousands fails, check its proxy - a preview built from an embedded JPEG can differ in aspect ratio from a rendered one.

## Related

- Error registry: `crates/aura-core/errors.toml`
- Preview troubleshooting: `docs/runbooks/previews.md`
