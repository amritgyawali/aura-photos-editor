# AURA-ML-5010 - Model file could not be parsed

**Severity / recovery:** see `crates/aura-core/errors.toml` for the registered values.

## What the photographer sees

The registered sentence. If an older version exists it keeps working.

## What actually happened

The file's digest matched the signed manifest, so the bytes are the intended bytes, and the parser still could not read them. That combination means the published artefact itself is malformed - a truncated export, a file saved by a tool that writes a protobuf field we do not implement, or a model exported outside the supported subset in a way the exporter failed to catch.

## What AURA does automatically

The model is refused at load, `model.rejected` is emitted with `reason = "parse"` and the offset or field that failed, and the previous version stays active.

## Operator steps

1. This is an artefact defect, not a customer-machine defect. Do not ask the customer to re-download first; the digest already proved the transfer was correct.
2. Developers: run the file through `ml/export_onnx/verify_parity.py`, which reads it with the same subset reader and reports the failing field.
3. Re-export from the training pipeline, re-sign, and ship a new version. Do not hand-patch a model file.

## Related

- Error registry: `crates/aura-core/errors.toml`
- Runtime ADR: `docs/adr/ADR-0007-inference-runtime.md`
