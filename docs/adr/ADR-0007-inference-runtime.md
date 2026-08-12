# ADR-0007 - The inference runtime is a pure-Rust reference interpreter behind a backend port

- **Status:** accepted
- **Date:** 2026-08-12
- **Deciders:** CTO, MLL (ML Lead - Vision), SRG (GPU & Render), SRC, SEC, PERF
- **Phase:** 03

> `docs/plan/phases/PHASE-03-INFERENCE-RUNTIME-MODEL-REGISTRY.md` section 4 asks
> for this decision to be written as `ADR-0003-inference-runtime.md`. That number
> was taken by the colour pipeline in phase 02, and ADR numbers are immutable
> (Article XIV, N2), so the phase's ADR is filed here as ADR-0007. No other
> renumbering is implied.

## Context

Phase 03 must give phases 05 to 29 one way to run a model, so that model work
never becomes per-GPU firefighting. The phase document names ONNX Runtime as the
implementation: an `ort` wrapper with execution-provider negotiation across
TensorRT, CUDA, DirectML, CoreML and CPU.

Four facts about this repository and this machine collide with that:

1. **The build cannot link C or C++.** ADR-0002 section 7 records it and
   ADR-0004 already turned on it: this machine has MSVC without a Windows SDK,
   so the workspace is built with the `x86_64-pc-windows-gnu` host toolchain.
   ONNX Runtime ships prebuilt as MSVC-ABI shared libraries. A GNU-ABI build of
   the workspace cannot link them, and building ORT from source needs a C++
   toolchain that is not present.
2. **`ort`'s default feature downloads a binary during `cargo build`.** A build
   that reaches the network is not reproducible from a tag (D5, C2) and puts an
   unaudited destination inside the default-deny egress policy (S3). Vendoring
   the binary instead means shipping a ~200 MB opaque artefact per platform in a
   repository whose supply-chain lane is `cargo-deny` over crates.
3. **Determinism.** ORT's CPU provider parallelises reductions; the accumulation
   order therefore depends on the thread count and the machine. D1 and D7 make
   floating-point order part of our contract. Our own runtime can - and does -
   fix the accumulation order, so two machines with different core counts return
   bit-identical tensors.
4. **We own every model.** Unlike a general-purpose inference host, AURA runs a
   closed set of models that this project trains and exports (phases 05 to 29).
   The operator surface is therefore something we choose, not something we
   inherit from the ONNX zoo.

## Decision

`aura-infer` freezes the `InferService` contract from section 5 of the phase
document **verbatim**, and puts every implementation behind an internal port:

```text
callers (phases 05-29)
        |  InferService  (frozen, section 5)
        v
   InferEngine  --- HardwarePlan, BatchScheduler, SessionPool, VramLedger
        |  Backend  (internal port, may change without an ADR)
        +-- ReferenceBackend   pure Rust, deterministic, always available
        +-- FakeBackend        test double: forced OOM, forced crash, fp drift
        +-- OrtBackend         not built in this phase; feature `ort-backend`
```

`Backend` is deliberately **not** a frozen contract. It is an implementation
detail of `aura-infer`, so an ORT backend can be added later without touching a
single caller or re-locking `contracts.lock`.

### What the reference backend is

A deterministic interpreter over a documented subset of ONNX opset 13, written in
safe Rust in `crates/aura-infer/src/onnx/`:

| Concern | Module |
|---|---|
| Minimal protobuf wire reader and writer | `onnx/wire.rs` |
| `ModelProto` / `GraphProto` / `NodeProto` / `TensorProto` subset | `onnx/model.rs` |
| Shape inference and graph validation | `onnx/graph.rs` |
| Operators | `onnx/ops/*.rs` |
| Tensor storage and views | `tensor.rs` |

Supported operators, chosen because they are what the phase 05-11 backbones need
and nothing more: `Conv`, `Relu`, `Clip`, `MaxPool`, `AveragePool`,
`GlobalAveragePool`, `Gemm`, `MatMul`, `Add`, `Mul`, `Concat`, `Reshape`,
`Flatten`, `Transpose`, `Softmax`, `Sigmoid`, `BatchNormalization`,
`QuantizeLinear`, `DequantizeLinear`. Any other operator is refused at load time
with `AURA-ML-5006` naming the operator, never at inference time and never
silently approximated.

The reader ships with a **writer**, for the same reason phase 02's codecs ship
with encoders: with no third-party runtime in the build, a round trip through an
independent implementation of the same wire format is the strongest available
proof that the reader is correct.

### Execution providers on a machine with no GPU

The five execution providers in the frozen contract stay in the contract. What
changes is that a provider is *registered* only if a backend claims it:

- `ExecutionProvider::Cpu` is claimed by `ReferenceBackend` and is always
  available. This is the "always-available CPU path with quantised models" the
  phase requires.
- `TensorRt`, `Cuda`, `DirectMl` and `CoreMl` are probed, found unavailable on a
  build without the ORT backend, recorded in `hardware_plan.json` with the reason,
  and shown in Settings > Hardware as unavailable rather than hidden.

Negotiation, scoring, blacklisting, override, persistence and the fall-through to
CPU are fully implemented and fully tested - the tests register `FakeBackend`
under a GPU provider so that every branch a real GPU would take is exercised on a
machine that has none. That is Article A3 in practice: two implementations, one
of which is a test double.

### Precision variants are real, not simulated

Cross-EP parity is meaningless if every path is the same arithmetic. The
reference backend therefore runs three genuinely different numeric paths over the
same graph:

| Precision | What it does | Parity tolerance (section 6.4) |
|---|---|---|
| `Fp32` | The reference. Fixed accumulation order. | exact |
| `Fp16` | Every weight and activation round-tripped through binary16. | <= 1e-3 |
| `Int8` | Per-tensor affine quantisation of weights and activations. | <= 1e-2 |

`verify_parity` compares `Fp16` and `Int8` against `Fp32` over a fixed sample set
and fails on the tolerances above. When an ORT backend lands, it joins the same
harness with no change to the harness.

### Model integrity

- `models/models.lock` is the pinned model set; `models/manifest.sig` is a
  detached ed25519 signature over its bytes.
- Verification order is fixed and offline (S6): signature, then sha256 per file,
  then opset and operator support, then load.
- Dependencies added: `ed25519-dalek` (BSD-3-Clause) and `sha2` (MIT OR
  Apache-2.0). Both are pure Rust and both are inside the `deny.toml` allow list.
  sha256 is used rather than blake3 because the phase document pins `sha256` in
  the `models.lock` schema, which is a published format we do not get to change.
- The private release key never enters the repository or CI. `tools/model-sign`
  is an offline operator tool that reads the key from a file the operator
  supplies.

### Downloads

`aura-models` implements the resumable, verified, atomically-swapped update path
against a `Transport` port rather than against HTTP directly:

- `FileTransport` (implemented) serves byte ranges from a local directory or a
  mounted share. It is what the offline installer bundle uses, and it is what the
  tests use to simulate truncation, mid-download kills and corrupt bytes.
- `HttpTransport` is **not built in this phase**. No crate in the workspace opens
  a socket, so the default-deny egress rule (S3) is still structurally true rather
  than merely configured. Phase 04 introduces the first network dependency, under
  its own ADR, and the model CDN moves onto it there.

Delta updates use `AURADLT1`, a block-level delta format documented in
`crates/aura-models/src/delta.rs` with an encoder and a decoder in the same
module, proved by round trip over the placeholder models.

## Consequences

### Accepted, with numbers

The performance budgets in section 11 of the phase document are written for
hardware acceleration. On the reference backend they are not met, and this ADR
records a **performance waiver** in the same form as ADR-0004's:

| Metric | Budget | Reference backend |
|---|---|---|
| Embedding throughput, RTX 4070 | >= 220 img/s | not measurable; no GPU backend in this build |
| Embedding throughput, M3 Pro | >= 110 img/s | not measurable; no macOS machine |
| Embedding throughput, CPU int8, 8 cores | >= 18 img/s | measured, recorded in `docs/progress/PHASE-03-EXIT.md` |
| Cold model load (cached) | <= 900 ms | measured, recorded in the exit report |
| Scheduler overhead per request | <= 0.4 ms | measured and asserted in `aura-perf` |
| VRAM overshoot events | 0 | structurally 0: the ledger refuses before allocating |

The waiver covers the two GPU throughput rows only. It expires when a GPU backend
lands (`ort-backend`, or a wgpu compute path from the render graph). The two rows
that are about *our* code rather than about hardware - scheduler overhead and
VRAM overshoot - are asserted in CI now and are not waived.

The placeholder models in this phase are small by construction; they exist to
exercise the path, not to predict phase 05 throughput. Nothing in the exit report
extrapolates from them.

### Also accepted

- Any model exported by `ml/export_onnx` must stay inside the supported operator
  set, or the export fails its own check before it ever reaches the app. This is
  a constraint on future model architecture and MLL owns it.
- `PIPELINE_VER` (phase 02) and the model versions in `models.lock` are now two
  independent keys into the same determinism guarantee. A model change does not
  invalidate a preview and a preview change does not invalidate a model, and the
  provenance block records both.

## Options rejected

- **Link ONNX Runtime now.** Rejected on the four grounds above; the strongest is
  that the machine physically cannot link it. Deferred, not refused: the port
  exists so that adding it later is additive.
- **`tract` (Sonos, pure Rust, Apache-2.0).** The closest alternative and a real
  option. Rejected for this phase because it brings ~40 transitive crates for a
  workload we can state in one page of operators, because its inference is
  parallelised with a reduction order we do not control, and because the
  operator-refusal behaviour we need (`AURA-ML-5006` naming the operator, at load
  time) would be a wrapper on top of it anyway. Worth revisiting when the model
  zoo outgrows our operator set.
- **Candle / burn.** Both are training-shaped frameworks; we need a loader and an
  executor, not autograd.
- **Skip the runtime and stub `InferService`.** A stub does not exercise session
  pooling, batching, VRAM accounting or parity, which are the parts of this phase
  the later phases actually depend on. It would move all the risk into phase 05
  where it is most expensive.
- **Make `Backend` a frozen contract.** Rejected deliberately. Freezing the port
  would mean an ADR to add an ORT backend. The frozen surface is what *callers*
  see, and callers only see `InferService`.
