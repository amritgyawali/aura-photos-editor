# AURA-ML-5006 - Model uses an operation this build cannot run

**Severity / recovery:** see `crates/aura-core/errors.toml` for the registered values.

## What the photographer sees

The registered sentence. The step that needed the model is skipped and the rest of the analysis completes.

## What actually happened

The reference runtime executes a documented subset of ONNX opset 13, listed in `docs/adr/ADR-0007-inference-runtime.md`. The graph contains an operator outside that subset, or an opset version above the one this build implements. The refusal happens at load time and names the operator, so it can never surface as a wrong number midway through a batch.

## What AURA does automatically

The model is not loaded, `model.rejected` is emitted with the operator name, and the caller takes its documented fallback - the heuristic baseline for that stage, which every AI stage is required to have.

## Operator steps

1. Read the operator name from the log line. This is the whole diagnosis.
2. Developers: either implement the operator in `crates/aura-infer/src/onnx/ops/`, with tests, or re-export the model within the supported subset. `ml/export_onnx/export.py` checks the subset before it writes a file, so a model that reaches a customer with an unsupported operator means the export path was bypassed.
3. Customers: install the application update that ships the capability; a model pack alone cannot add an operator.

## Related

- Error registry: `crates/aura-core/errors.toml`
- Runtime ADR: `docs/adr/ADR-0007-inference-runtime.md`
- Adding a model: `docs/runbooks/adding-a-model.md`
