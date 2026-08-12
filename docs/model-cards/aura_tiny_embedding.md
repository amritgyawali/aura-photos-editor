# Model card - `aura_tiny_embedding` `1.0.0`

| Field | Value |
|---|---|
| Name | `aura_tiny_embedding` |
| Version | 1.0.0 |
| Task | Fixed-length embedding of a 32x32 RGB patch |
| Class | `embedding` |
| Owner | MLL (ML Lead - Vision) |
| Licence | proprietary |
| Opset | 13 |
| Precision policy | any: fp32, fp16 and int8 are all permitted |

## Purpose

**This is a placeholder, not a product model.** It exists so that phase 03 can
prove a runtime, a session pool, a batch scheduler, a signing chain and an update
path against something real before any trained weights exist. The first trained
embedding model arrives in phase 05 and will replace this one.

It is a legitimate model in every mechanical sense - it loads, runs, batches,
quantises and verifies exactly like a real one - and it is meaningless in every
semantic sense: its outputs carry no information about a photograph. Nothing may
use it to make a decision about an image, and nothing in the product does.

## Architecture

A four-layer convolutional stem with a linear head, generated from a fixed seed:

```text
pixels [N,3,32,32]
  Conv 3->8, 3x3, pad 1   -> Relu -> MaxPool 2x2
  Conv 8->16, 3x3, pad 1  -> Relu -> GlobalAveragePool
  Flatten -> Gemm 16->32
embedding [N,32]
```

1,336 parameters. Every operator is inside the runtime subset documented in
`docs/adr/ADR-0007-inference-runtime.md`; `Gemm` with `transB=1` and
`GlobalAveragePool` are the two that a future exporter is most likely to emit
differently, so they are the two to check first if a real model fails to load.

## Training data

None. The weights come from a seeded xorshift generator, implemented identically
in `crates/aura-infer/src/onnx/fixtures.rs` and `ml/export_onnx/export.py`, so
both produce byte-identical files. There is no consent scope, no licence question
and no wedding-level split to enforce, because no photograph was involved.

## Latency

Measured with `cargo bench -p aura-infer --bench model_bench` on 2026-08-12,
release build, `1.97.1-x86_64-pc-windows-gnu`. Criterion medians.

| Machine | Provider | Precision | Cold load | Per image | Batch throughput |
|---|---|---|---|---|---|
| Intel i5-10300H, 8 GB, Win 11 (development machine) | cpu | fp32 | 76.5 us | 1.43 ms | 961 img/s at batch 8 |
| Intel i5-10300H, 8 GB, Win 11 (development machine) | cpu | fp16 | 92.8 us | 1.59 ms | not measured |
| Intel i5-10300H, 8 GB, Win 11 (development machine) | cpu | int8 | 85.6 us | 1.58 ms | 872 img/s at batch 8 |
| RTX 4070 laptop (Win 11, 32 GB) | | | | | not measured - no such machine |
| M3 Pro MacBook (18 GB) | | | | | not measured - no such machine |
| Intel iGPU desktop (Win 11, 16 GB) | | | | | not measured - no such machine |

The development machine is not one of the three reference machines. Every number
above describes a 1,336-parameter placeholder on a laptop processor and predicts
nothing about a real model; `docs/progress/PHASE-03-EXIT.md` says so as well.

## Quality gate

None, and deliberately none. A quality gate is a statement about a task, this
model has no task, and inventing a threshold for it would put a number in the
repository that means nothing. The gate that does apply is mechanical and is
enforced by `cargo xtask models --check` and by
`crates/aura-infer/tests/parity.rs`:

- fp16 within 1e-3 of fp32 (measured: 1.7e-4 output drift);
- int8 within 1e-2 of fp32 (measured: 4.4e-3 output drift);
- two runs of the same file are bit-identical.

## Known failure modes

- **Its outputs are meaningless.** The only real failure mode, and the one that
  matters: any code that treats this embedding as information is wrong.
- It has no calibration, so it has no confidence to report and never abstains.
- It has never seen a photograph, an unusual aspect ratio, or a batch larger than
  64.

## Fallback

There is no fallback because there is no decision. When phase 05 replaces this
model, the stage that consumes it gains a heuristic baseline, and that baseline -
not this file - becomes what runs when the model is unavailable.
