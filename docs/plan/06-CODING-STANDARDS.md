# Coding Standards

## Rust

- Edition 2021, stable toolchain pinned in `rust-toolchain.toml`. `#![deny(warnings)]` in CI.
- `clippy::pedantic` on, with documented per-crate allowances. `rustfmt` enforced.
- **Errors:** one `AuraError` enum per crate boundary using `thiserror`; `anyhow` only in binaries.
  Never `unwrap()`/`expect()` in library code. Every error carries actionable context.
- **No panics across FFI.** LibRaw and ONNX Runtime boundaries catch and convert.
- **Newtype every id:** `ImageId`, `FaceId`, `MaskId`, `DecisionId`. No bare `u64` or `String` ids.
- **Units in names:** `exposure_ev`, `temperature_k`, `ms`, `bytes`, `deg`. Ambiguous names are review failures.
- **Public API documented** with `///` including at least one example for non-trivial functions.
- **Concurrency:** no `Mutex` held across `await`. Prefer channels and message passing. One SQLite writer.
- **Determinism:** no `HashMap` iteration in output paths (use `BTreeMap` or sort), seed all randomness,
  never branch on wall-clock time in decision code.

## TypeScript / React

- `strict: true`, no `any` (use `unknown` and narrow). ESLint plus Prettier enforced.
- IPC types are generated from Rust; hand-written duplicates are a review failure.
- State: TanStack Query for server state, Zustand for view state. No global mutable singletons.
- Virtualise every list of images. The grid must stay at 60 fps with 4,000 items.
- Accessibility: keyboard-first workflows (culling and QC review are keyboard-driven), visible focus, ARIA labels.

## Python (training only)

- Ruff plus Black plus mypy. Every training run writes a config snapshot, seed, dataset hash and metrics to
  an experiment record. No notebook is a deliverable; notebooks may explore, scripts must reproduce.
- Every model export runs the parity verifier and refuses on failure.

## Commits, branches, reviews

- Branch: `phase-NN/<slug>`. Conventional commits: `feat(phase-07): ritual disambiguation`.
- Pull requests use `templates/PR.md` and must state the phase, the invariants touched, the gates run and the benchmark deltas.
- **Review checklist:** correctness, error handling, determinism, performance budget, privacy, explainability
  (does every decision emit a reason?), user-override protection, test coverage, docs, telemetry.
- Two-hat rule: the reviewing role must differ from the implementing role.

## Performance discipline

- Every budget in a phase document becomes a benchmark test. A budget regression fails CI.
- Measure before optimising; commit the measurement. `tracing` spans on every pipeline stage.
- Allocation discipline in hot loops: reuse buffers, avoid per-image heap churn, prefer slices over vectors.

## Security and privacy in code

- Secrets only in the OS keychain, accessed through one module.
- No image content, file paths or personal data in telemetry or crash reports.
- All file writes inside project directories, validated against path traversal.
- Dependency audit in CI (`cargo audit`, `npm audit`), pinned versions, reproducible builds.
