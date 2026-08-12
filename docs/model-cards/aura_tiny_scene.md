# Model card - `aura_tiny_scene` `1.0.0`

| Field | Value |
|---|---|
| Name | `aura_tiny_scene` |
| Version | 1.0.0 |
| Task | Six-way classification of a 32x32 RGB patch, ending in a softmax |
| Class | `embedding` |
| Owner | MLL (ML Lead - Vision) |
| Licence | proprietary |
| Opset | 13 |
| Precision policy | **int8 forbidden**; fp32 and fp16 permitted |

## Purpose

**This is a placeholder, not a product model.** It exists to exercise two things
the embedding placeholder cannot: an output that is a probability distribution,
and a precision policy that refuses a variant.

The softmax matters structurally. Invariant 2 says every AI decision carries a
confidence, and a scene classifier is the first place in the plan where that
confidence is a real number the product shows a photographer. Having one in the
runtime now means phase 07 inherits a path that already produces calibrated-shaped
output rather than raw logits.

Nothing in the product uses its predictions, and nothing may: its six classes
have no meaning.

## Architecture

```text
pixels [N,3,32,32]
  Conv 3->8, 3x3, stride 2, pad 1 -> Relu -> GlobalAveragePool
  Flatten -> Gemm 8->6 -> Softmax (axis -1)
scene_probs [N,6]
```

278 parameters. The `Softmax` subtracts the row maximum before exponentiating,
which is the difference between a confidence of 0.97 and a `NaN` on a logit of
90 - see `crates/aura-infer/src/onnx/ops/elementwise.rs`.

## Training data

None; seeded weights, as with `aura_tiny_embedding`. See that card for the
generator and why it is implemented twice.

## Latency

Measured with `cargo bench -p aura-infer --bench model_bench` on 2026-08-12,
release build, `1.97.1-x86_64-pc-windows-gnu`. Criterion median.

| Machine | Provider | Precision | Cold load | Per image | Batch throughput |
|---|---|---|---|---|---|
| Intel i5-10300H, 8 GB, Win 11 (development machine) | cpu | fp32 | not measured separately | 155 us | not measured |
| RTX 4070 laptop (Win 11, 32 GB) | | | | | not measured - no such machine |
| M3 Pro MacBook (18 GB) | | | | | not measured - no such machine |
| Intel iGPU desktop (Win 11, 16 GB) | | | | | not measured - no such machine |

## Quality gate

None; see `aura_tiny_embedding.md` for why a placeholder does not get an invented
threshold. The mechanical gates that do apply:

- fp16 within 1e-3 of fp32 (measured: 6.0e-6 output drift);
- **no int8 variant exists**, because the precision policy forbids one. That is
  the point of this model in the manifest: it proves `PrecisionPolicy` is load
  bearing rather than decorative, and it is the shape every skin-critical and
  colour-critical model in phases 15 to 22 will take (section 12 of the phase
  document);
- outputs sum to 1.0 within 1e-5 and are finite on extreme logits, asserted in
  `crates/aura-infer/tests/onnx_subset.rs`.

## Known failure modes

- **Its class probabilities are meaningless.** Six numbers that sum to one and
  describe nothing.
- No calibration, so its confidence is a shape rather than a probability, and it
  never abstains.
- Quantisation to int8 is refused rather than measured: this model is the example
  of a policy, not a study of quantisation error.

## Fallback

None needed - no decision depends on it. When a real scene classifier lands in
phase 07 it inherits this card's structure and the heuristic scene baseline
becomes its fallback.
