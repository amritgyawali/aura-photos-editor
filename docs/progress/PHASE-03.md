# Phase 03 - progress log

One line per task: what was touched, what was tested, what it cost. Ordered as
the work happened, which follows section 8 of the phase document.

| Task | Files touched | Tests added | Notes |
|---|---|---|---|
| Entry waiver (ADR) | `docs/adr/ADR-0006-phase-03-entry-waiver.md` | - | Phase 02's three exit conditions need camera files and a CI run; recorded as a written waiver rather than a silent bypass |
| Runtime decision (ADR) | `docs/adr/ADR-0007-inference-runtime.md` | - | ONNX Runtime is not linked; a deterministic interpreter sits behind a `Backend` port. Filed as 0007 because the phase's suggested number was taken by the colour pipeline |
| Error registry | `crates/aura-core/errors.toml`, `src/errors/{gpu,ml}.rs`, 17 runbooks | `error_registry` (existing, now covers 17 new codes) | `AURA-GPU-4001..4005` and `AURA-ML-5001..5012` registered before any runtime existed |
| Shared priority | `crates/aura-core/src/contract/priority.rs` | `ipc_contract.rs` | The four scheduling classes move to the domain crate so `aura-infer` need not depend on preview infrastructure |
| Frozen contract | `crates/aura-infer/src/contract/infer.rs` | contract digests in `contracts.lock` | Section 5 written verbatim: `InferService`, `InferRequest`, `InferResult`, `HardwarePlan` |
| Protobuf wire | `aura-infer/src/onnx/wire.rs` | `wire.rs` (8) | Reader *and* writer, so the reader is proved by round trip |
| ONNX subset | `aura-infer/src/onnx/{model,graph}.rs` | `onnx_subset.rs` (11) | Nine messages, topological ordering, load-time refusal with the operator named |
| Operators | `aura-infer/src/onnx/ops/{conv,pool,linear,elementwise,shape}.rs` | `onnx_subset.rs` | Nineteen operators, fixed accumulation order, parallel over output rows only |
| Precision | `aura-infer/src/tensor.rs` | `precision.rs` (7) | Hand-written binary16 conversion and per-tensor affine int8, so fp16 and int8 are real arithmetic rather than three names for one path |
| Backends | `aura-infer/src/backend/{mod,reference,fake}.rs` | `parity.rs` (8) | The port, the interpreter, and the test double that crashes, drifts and runs out of memory on demand |
| Session pool | `aura-infer/src/session.rs` | `runtime.rs` (part) | Keyed by model and provider, least-recently-used eviction on an injected clock |
| Probe and plan | `aura-infer/src/{probe,plan,ep}.rs` | `hardware.rs` (7), `plan_file.rs` (8) | Twenty runs per candidate, 1e-3 correctness check, per-machine set-aside list, 15 s ceiling, atomic plan writes |
| Scheduler | `aura-infer/src/batch.rs` | `scheduler.rs` (7) | Memory ledger, preemption gate, cancellation token, chunking |
| Engine | `aura-infer/src/{service,warmup,source}.rs` | `runtime.rs` (11) | Batching that splits back in order, halving on refusal, deadlines, warmup |
| Model manifest | `aura-models/src/contract/manifest.rs` | contract digests | `models.lock` schema from section 5, plus class, working set, precision policy and opset |
| Integrity | `aura-models/src/verify.rs` | `verify.rs` (8) | ed25519 over the manifest, sha256 per file, in that order, offline |
| Transfers | `aura-models/src/{download,swap}.rs` | `registry.rs` (12) | Transport port, `.part` resume, verify-then-rename, previous version kept |
| Rollback | `aura-models/src/rollback.rs` | `install_state.rs` (5) | Pending until proved; a failed first use restores the previous file and records the rejection |
| Delta updates | `aura-models/src/delta.rs` | `delta.rs` (6) | `AURADLT1`, encoder and decoder together, both digests checked |
| Registry | `aura-models/src/registry.rs` | `registry.rs` | The `ModelSource` adapter: verify once, remember, refuse without a card |
| Signing tool | `tools/model-sign/` | exercised by `xtask models --generate` | Development key from a public seed phrase; the release key never enters the repository |
| Model set | `xtask/src/models.rs`, `models/*` | `xtask models` (CI gate) | Two placeholders, five files, a signed manifest, and the card check that blocks a merge |
| Export pipeline | `ml/export_onnx/{onnx_min,export,quantise,verify_parity}.py` | run in CI lane 3 by hand today | A second implementation of the format; `--verify` proves both produce byte-identical files |
| IPC (ADR) | `docs/adr/ADR-0008-inference-ipc-surface.md`, `aura-app/src/contract/ipc.rs`, `ui/src/ipc/types.ts` | contract digests | Six commands and one event stream, additive |
| App commands | `aura-app/src/{infer_commands,state}.rs` | covered by `ipc_contract.rs` | Plan and engine built on first use, per process rather than per project |
| Hardware panel | `ui/src/components/HardwarePanel.tsx`, `ipc/client.ts`, `App.tsx` | `HardwarePanel.test.tsx` (5) | Selected provider, unavailable list with reasons, override, warmup, model versions |
| Budgets | `perf/budgets.toml`, `aura-perf/tests/infer_budgets.rs` | `infer_budgets.rs` (7) | Cold load, admission overhead, quantised throughput, probe ceiling, warmup, memory overshoot |
| Benchmarks | `aura-infer/benches/model_bench.rs`, `justfile` | `just bench-models` | Per-precision cold load, single inference, batch throughput, admission cost |
| Gate | `aura-cli/src/phase03.rs`, `justfile`, `.github/workflows/ci.yml` | `verify --phase 03` | Nine checks in one run, including a forced squeeze and a real rollback |
| Lint | `scripts/check-banned.sh` | banned-pattern gate | Nothing outside `aura-infer` may link ONNX Runtime (acceptance criterion 7) |
| Docs | `docs/model-cards/{TEMPLATE,aura_tiny_embedding,aura_tiny_scene}.md`, `docs/runbooks/{hardware,model-update-failed,adding-a-model}.md`, `CHANGELOG.md` | - | Cards carry measured numbers only |

## Defects found and fixed while proving the gates

Five, each now covered by the test that found it.

1. **The binary16 decoder was wrong for subnormals.** `f16_bits_to_f32`
   renormalised the mantissa by hand and produced exactly half the correct value
   for anything below 2^-14. Caught by `binary16_round_trips_the_values_a_model_actually_holds`
   on 6.1e-5. Replaced with `mantissa * 2^-24`, which is exact in binary32.
2. **The memory ledger could not be relieved by halving.** The admission charge
   was a session's fixed working set plus the inputs, neither of which scales
   much with the batch - so a batch that did not fit still did not fit at half
   the size. The charge now adds the model card's per-image working set times the
   batch, which is what the card's number means. Caught by
   `a_forced_memory_squeeze_halves_the_batch_and_still_finishes`.
3. **The Python exporter looped forever on a negative attribute.** `Softmax`'s
   `axis = -1` shifted towards -1 without terminating, because Python integers do
   not wrap. Negative varints are now masked to 64-bit two's complement, which is
   what the Rust writer emits. Caught by running `export.py --verify`.
4. **The phase gate's rollback check installed one variant of three.** The
   transport directory held only the fp32 file, so the fp16 fetch reported
   `AURA-ML-5004` on a zero-byte source. That is the same mistake a real model
   pack would make, so the gate now copies every variant.
5. **The memory budget was thirteen bytes short of its own documentation.**
   `0.70f32` is 0.699999988, so 70 per cent of 1000 MB came out as 734,003,187
   rather than 734,003,200. Now integer arithmetic: `bytes / 100 * 70`.

## What the numbers were on this machine

Measured 2026-08-12, release build, `1.97.1-x86_64-pc-windows-gnu`, Intel
i5-10300H with 8 GB. Full tables in `docs/progress/PHASE-03-EXIT.md`.

- Cold model load: 76.5 us (fp32), 92.8 us (fp16), 85.6 us (int8).
- Single inference: 1.43 ms (fp32), 1.59 ms (fp16), 1.58 ms (int8).
- Batch of eight: 961 img/s (fp32), 872 img/s (int8).
- Admission overhead: 123 ns, against a 0.4 ms budget.
- Hardware probe: 48 ms, against a 15 s ceiling.
- AURA against ONNX Runtime 1.28 on the same file: worst 1.6e-7.
