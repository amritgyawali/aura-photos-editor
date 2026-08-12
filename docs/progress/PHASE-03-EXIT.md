# Phase 03 - exit report

**Status: green on this machine, with four caveats recorded in sections 4 and 6,
and three conditions inherited from phase 02 that are still open.**

Every gate runs and passes locally. There is no GPU backend in this build, so two
of section 11's throughput budgets cannot be measured at all; the models this
phase ships are placeholders with no trained weights; and CI has never run these
lanes on macOS or Linux.

Measured on: Windows 11, Rust 1.97.1, host toolchain `x86_64-pc-windows-gnu`
(ADR-0002 section 7), Intel i5-10300H with 8 GB, 2026-08-12.

## 1. What shipped

The single feature of this phase: one local AI runtime that picks the best
hardware path automatically, plus a signed model registry with delta updates.

- `aura-infer`: the frozen `InferService`, a hardware probe that measures a
  machine in 48 ms and writes `hardware_plan.json`, execution-provider
  negotiation with a per-machine set-aside list, a session pool, a batch
  scheduler with a memory ledger, cooperative cancellation, and warmup.
- The runtime underneath it: a deterministic interpreter over a documented subset
  of ONNX opset 13 - nineteen operators, a protobuf reader **and writer**, and
  three genuinely different numeric paths (fp32, fp16, int8). Pure safe Rust;
  the crate keeps `#![forbid(unsafe_code)]`. ONNX Runtime is not linked, for the
  four reasons in ADR-0007.
- `aura-models`: `models.lock` verified by ed25519 then sha256 then model card,
  resumable transfers against a transport port, verify-then-rename installs, a
  pending/active/rejected state machine with automatic rollback, and the
  `AURADLT1` block delta with its encoder.
- `tools/model-sign`: offline signing. The release key never enters the
  repository or CI; the development key is derived from a public seed phrase so
  anyone can re-sign the placeholders and nobody can mistake one for the other.
- Two placeholder models, five signed files, two model cards, and the
  `cargo xtask models` gate that refuses a model without a card.
- `ml/export_onnx`: a second, independent implementation of the same file format
  in Python, which produces **byte-identical** files to the Rust generator.
- Six IPC commands, one event stream, and a Settings > Hardware panel that lists
  unavailable providers with their reasons instead of hiding them.
- `aura-cli verify --phase 03` as the gate.

## 2. Gate results

| Gate | Command | Result |
|---|---|---|
| Build | `cargo build --workspace --all-targets` | pass, 0 warnings |
| Tests | `cargo test --workspace --all-targets` | **301 passed, 0 failed, 1 ignored** |
| Lints | `cargo clippy --workspace --all-targets -- -D warnings` | pass |
| Format | `cargo fmt --all -- --check` | pass |
| Banned patterns | `scripts/check-banned.sh` | `check-banned: clean` |
| Frozen contracts | `cargo xtask contracts --check` | `contracts: 13 entries, all locked` |
| Signed models and cards | `cargo xtask models` | `2 models, 5 files, signature and cards verified` |
| UI types | `npm run lint` (tsc strict) | pass |
| UI tests | `npm test` | **20 passed, 0 failed** |
| Phase 01 gate | `verify` | `phase-01 verify: all fixtures clean` |
| Phase 02 gate | `verify --phase 02` | `all fixtures clean`, worst dE2000 0.158 |
| Phase 03 gate | `verify --phase 03` | `phase-03 verify: all checks clean` |

Rust tests by crate: `aura-infer` 67, `aura-models` 31, `aura-raw` 55,
`aura-core` 25, `aura-preview` 20, `aura-cache` 18, `aura-ingest` 18,
`aura-perf` 19, `aura-catalog` 11, `aura-app` 14, plus the rest. The ignored test
is phase 02's 25 MP scaling probe, run deliberately rather than in CI.

### Phase gate output

```
registry: 2 models, 5 files, signature and digests verified
probe: 48 ms, providers ["cpu"], selected cpu, 4 unavailable
warmup: 2 models in 1 ms
throughput: 64 images in 71 ms (901.4/s), batch 8
parity: aura_tiny_embedding fp16=4.47e-4 int8=8.17e-3
parity: aura_tiny_scene fp16=6.43e-5 int8=4.12e-4
memory: budget 24 MB, 2 downshifts, peak 16 MB, all 8 images finished
cancel: stopped with AURA-ML-5011
providers: a drifting accelerator was set aside, processor path kept
rollback: 7 files copied, install staged, rejection restored the previous file
phase-03 verify: all checks clean
```

## 3. Acceptance criteria (section 13 of the phase document)

| Criterion | Proof | Result |
|---|---|---|
| The same model produces equivalent results within tolerance on TensorRT, CUDA, DirectML, CoreML and CPU | `parity.rs` (8 tests), gate `parity:` lines | pass **for the providers this build has**; see caveat 1 |
| First run probes hardware in <= 15 s and writes a plan; Settings shows the selected EP and lets the user override | `hardware.rs`, `plan_file.rs`, `HardwarePanel.tsx`, gate `probe:` line (48 ms) | pass |
| An unsigned, mismatched or corrupt model is refused with a clear message and the previous version keeps working | `registry.rs` (12 tests), `verify.rs` (8 tests) | pass |
| Under a forced VRAM squeeze the app degrades batch size instead of crashing, and finishes the job | `a_forced_memory_squeeze_halves_the_batch_and_still_finishes`, gate `memory:` line | pass |
| `just bench-models` produces a table for all three reference machines and is stored as a CI artefact | `just bench-models`, section 4 below | **partial**: one machine, and it is not one of the three; see caveat 2 |
| Every shipped model has a model card; CI fails without one | `cargo xtask models`, `docs/model-cards/` | pass |
| No code outside `aura-infer` links ONNX Runtime directly | `scripts/check-banned.sh` | pass, trivially: nothing links it at all |

Additional criteria from section 10.1:

| Criterion | Proof | Result |
|---|---|---|
| Parity: every model matches CPU fp32 within tolerance on all available EPs | `parity.rs`, `verify_parity.py` | pass |
| OOM: shrink the budget, the scheduler halves batches and completes | gate `memory:` line, `infer_budgets.rs` | pass |
| Corrupt model, wrong signature, truncated download, mid-download kill | `registry.rs` (12 tests) | pass |
| Rollback: a model that throws on first real use is reverted | gate `rollback:` line, `install_state.rs` | pass |
| Preemption: interactive request served within 80 ms during a saturated batch | `an_interactive_request_is_served_while_a_batch_is_running` | pass |
| Cold start: warmup of the startup models within 2.5 s | `cold_start_warms_the_startup_models_inside_its_budget`, gate `warmup: 1 ms` | pass at placeholder scale |
| No-GPU machine: everything runs on CPU int8 with correct results | the whole suite; this is the only machine class that exists here | pass |

## 4. Performance

Criterion, release build, `cargo bench -p aura-infer --bench model_bench`, medians.

| Stage | fp32 | fp16 | int8 |
|---|---|---|---|
| Cold model load | 76.5 us | 92.8 us | 85.6 us |
| Single inference (embedding) | 1.43 ms | 1.59 ms | 1.58 ms |
| Batch of eight (embedding) | 8.32 ms (961 img/s) | not measured | 9.17 ms (872 img/s) |
| Single inference (scene) | 155 us | not measured | not measured |
| Admission overhead | 123 ns | - | - |

Against the section 11 budgets:

| Metric | Budget | Measured |
|---|---|---|
| Cold model load (cached engine) | <= 900 ms | **0.077 ms** on a 1,336-parameter placeholder |
| Embedding throughput, RTX 4070 | >= 220 img/s | **not measurable** - no GPU backend, no such machine. Waived in ADR-0007 |
| Embedding throughput, M3 Pro | >= 110 img/s | **not measurable** - no such machine. Waived in ADR-0007 |
| Embedding throughput, CPU int8 | >= 18 img/s | **634 img/s** single, 872 img/s batched, asserted in `infer_budgets.rs` |
| Scheduler overhead per request | <= 0.4 ms | **0.000123 ms**, asserted over 1,000 admissions |
| VRAM overshoot events | 0 in a 2-hour soak | **0**, structurally: the ledger refuses before allocating, asserted twice |
| Hardware probe | <= 15 s (section 13) | **48 ms** |
| Warmup of the startup models | <= 2.5 s (section 10.1) | **1 ms** at placeholder scale |

**Caveat 1 - one real provider.** Cross-provider parity is tested by registering
a second backend under `ExecutionProvider::Cuda` that wraps the same interpreter,
and by making it drift, crash and run out of memory on demand. That proves the
*machinery*: the comparison, the tolerance, the set-aside list and the
fall-through. It does not prove that a real CUDA driver agrees with us, because
there is no CUDA driver here.

**Caveat 2 - one machine, and it is not a reference machine.** Every number above
comes from an Intel i5-10300H laptop with 8 GB. The three reference machines in
Article VIII rule P2 - RTX 4070, M3 Pro, Intel iGPU desktop - have not run any of
this. The model cards leave those rows empty rather than estimating them.

**Caveat 3 - placeholder models.** 1,336 and 278 parameters. They exercise every
path in the runtime and predict nothing about phase 05's backbones. No number in
this report is extrapolated from them, and the cards say so too.

**Caveat 4 - `Instant`-free timing is coarse in one place.** The gate's warmup
line reads 1 ms because the injected clock is read in milliseconds around an
operation that takes microseconds. The benchmark measures the same work properly;
the gate line is a smoke check, not a measurement.

**The waiver.** The two GPU throughput budgets are waived in ADR-0007 with their
reasons and an expiry condition: the waiver ends when a GPU backend lands, either
`ort-backend` or a wgpu compute path from the render graph. The budgets that are
about our own code - admission overhead and memory overshoot - are asserted in CI
now and are not waived.

## 5. An independent check of the interpreter

Everything else in this phase compares our code against our own encoder. Two
checks do not, and they are the strongest evidence in the report:

- **Two implementations of the file format agree byte for byte.**
  `python ml/export_onnx/export.py --verify models` rebuilds both placeholder
  models from the same seeded generator, written independently in Python, and
  compares the bytes: `8534 bytes, identical to the Rust generator` and `1672
  bytes, identical`. A single wrong protobuf field number would break it.
- **Our interpreter agrees with ONNX Runtime 1.28.** That runtime is not linked
  into the application, but it happens to be installed for Python on this
  machine, so `verify_parity.py --against-runtime` runs both over the same
  deterministic batch:

  | Model | Worst absolute difference |
  |---|---|
  | `aura_tiny_embedding` fp32 | 1.639e-07 |
  | `aura_tiny_scene` fp32 | 2.980e-08 |

  That is float32 rounding, on nineteen operators, against a mature third-party
  implementation. It does not make the interpreter fast and it does not make the
  placeholders meaningful; it does mean the arithmetic is right.

## 6. Known issues and deliberate gaps

- **No GPU backend.** TensorRT, CUDA, DirectML and Core ML are probed, reported
  unavailable with a reason, and shown as such in Settings. ADR-0007 gives the
  four reasons and the conditions for adding one.
- **No trained model.** The two models are placeholders; the first real weights
  arrive in phase 05.
- **No network transport.** `FileTransport` only. Nothing in the workspace opens
  a socket, which is why the default-deny egress rule is structurally true rather
  than configured. Phase 04 brings the first network dependency.
- **The Tauri shell does not register the new commands.** It does not register
  phase 02's preview commands either: the shell has never been launched or built
  on this machine, so wiring it would be code nobody has run. `aura-app` exposes
  all six commands and the CLI exercises the same code paths.
- **`InferEvent` is defined and typed on both sides but not emitted**, for the
  same reason `IngestEvent` was not in phase 01 and `PreviewEvent` was not in
  phase 02. The UI subscribes already; warmup runs synchronously behind
  `warmup_models`.
- **Telemetry is logged, not dashboarded.** `infer.plan_selected`, `infer.run`,
  `infer.oom_downshift`, `model.update` and `model.rejected` are emitted through
  `tracing` with the fields section 11 specifies; the local metrics dashboard is
  a later phase.
- **`verify_parity.py`'s output-space check needs onnxruntime.** It is present on
  this machine and absent in CI, where the check reports that it was skipped
  rather than passing silently.
- **No self-hosted GPU runners, no nightly EP matrix.** The `DEVOPS` task in
  section 9 needs hardware nobody in this project owns.
- **No perceptual audit, no demo recording.** Both need a real wedding and a
  human; unchanged from phase 02.

## 7. Rollback

- **Feature flag.** Nothing in phases 01 and 02 calls `InferService`. Deleting
  `models/` leaves the application working exactly as it did before this phase,
  with the Hardware panel reporting `AURA-ML-5002` for the missing manifest.
- **Model versions.** Every version is pinnable: `models.lock` is the switch, and
  a rejected version is restored automatically without a download.
- **Plan.** `hardware_plan.json` can be deleted at any time; the next launch
  measures again.
- **Catalog.** No migration was added. Nothing in this phase writes to the
  catalog.
- **Contracts.** ADR-0007 and ADR-0008 additions are additive; reverting them
  requires re-locking `contracts.lock`, which CI enforces.

## 8. Inherited debt from phase 02

ADR-0006 started this phase under a written waiver. The three conditions are
carried forward verbatim so they cannot disappear between phases:

1. one real RAW per supported manufacturer decoded and added to the fixture
   corpus;
2. a photographed ColorChecker from at least one real body, rendered and signed
   off by COL;
3. the CI matrix run on Windows, macOS and Linux.

The first real camera file is a **Sev 2 trigger**: it reopens phase 02's
acceptance criteria immediately, whatever phase is in flight.

## 9. Gate decision

Phase 04 may start once:

1. the CI matrix has run these lanes on Windows, macOS and Linux - now covering
   three phase gates rather than two. This is the same condition phase 02 left
   open, and it is the cheapest of the three to satisfy: it needs a push;
2. a decision is recorded on where the GPU backend comes from. Phase 04 is the
   cloud gateway and does not need one, but every phase from 05 onwards spends
   the waiver in ADR-0007, and the longer it stands the more model work is
   planned against numbers nobody has measured;
3. the model-card latency tables have at least one row from a reference machine.
   Phase 05 trains a model whose scheduling depends on it.

Everything else on the phase 03 checklist is proven above.
