# AURA developer commands. `just` with no argument lists them.

default:
    @just --list

# Install the git hooks so a red CI on formatting never happens.
setup:
    git config core.hooksPath .githooks
    cd ui && npm install

# Run the desktop app against the dev server.
dev:
    cd ui/src-tauri && cargo tauri dev

# Everything CI lane 1 runs, locally, in the same order.
gates:
    cargo fmt --all -- --check
    bash scripts/check-banned.sh
    cargo clippy --workspace --all-targets -- -D warnings
    cargo run --package xtask -- contracts --check
    cargo run --package xtask -- models

# The full test suite: Rust plus UI.
test:
    cargo test --workspace --all-targets
    cd ui && npm test

# Budget benchmarks.
bench:
    cargo bench --package aura-ingest --bench ingest_throughput
    cargo bench --package aura-preview --bench decode

# Budget assertions. Release, because a budget is a claim about the binary a
# photographer runs, and the payload builder is an order of magnitude slower
# unoptimised.
# One thread, deliberately. A budget suite whose cases run concurrently measures
# the harness: three 4,000-vector index builds sharing four cores produce a number
# about `cargo test`, not about the code. Added in phase 05.
budgets:
    cargo test --release --package aura-perf --all-targets -- --test-threads=1

# The per-machine model table PERF and the scheduler's cost model both read.
bench-models:
    cargo bench --package aura-infer --bench model_bench

# Regenerate and re-sign the placeholder model set. Needs an ADR first if it
# changes a shipped model: models.lock is a pinned, signed artefact.
models:
    cargo run --package xtask -- models --generate

# Check the model set the way CI does: signature, digests, cards, opset.
models-check:
    cargo run --package xtask -- models

# Generate the three reference weddings into tests/fixtures/generated.
fixtures:
    cargo run --release --package aura-cli -- fixtures --out tests/fixtures/generated

# Generate the synthetic RAW bench set into tests/fixtures/raw.
raw-fixtures:
    cargo run --release --package aura-cli -- raw-fixtures --out tests/fixtures/raw

# The phase 01 gate: fixtures, import, re-import, digest comparison, integrity.
phase-01-verify:
    cargo run --release --package aura-cli -- verify --work target/phase01-verify

# The phase 02 gate: RAW fixtures, import, both preview tiers, a cached second
# pass, and the ColorChecker measurement on all eight bench bodies.
phase-02-verify:
    cargo run --release --package aura-cli -- verify --phase 02 --work target/phase02-verify

# The phase 03 gate: model integrity, hardware probe, warmup, parity, a forced
# memory squeeze, cancellation, a misbehaving provider and a rollback.
phase-03-verify:
    cargo run --release --package aura-cli -- verify --phase 03 --work target/phase03-verify

# The phase 04 gate: the migration, key safety, the payload, a cassette-backed
# call, a cached re-run, one repair, an offline wedding, a cap, and a key-leak
# scan of everything stored. Never touches a network.
phase-04-verify:
    cargo run --release --package aura-cli -- verify --phase 04 --work target/phase04-verify

# The phase 05 gate: the migration, two cards of fixtures, a resumable embedding
# pass, the index, a five-millisecond query, a time window, a camera filter, the
# snapshot and its refusals, an incremental second card, and determinism. Never
# touches a network - nothing in phase 05 can.
phase-05-verify:
    cargo run --release --package aura-cli -- verify --phase 05 --work target/phase05-verify

# Weight-space parity for every fp32/variant pair, plus the cross-runtime check
# against onnxruntime when it happens to be installed for Python.
parity:
    python ml/export_onnx/verify_parity.py --all models

# Build previews for an existing catalog, the way the app does.
previews CATALOG PROJECT LEVEL="thumb":
    cargo run --release --package aura-cli -- previews --catalog {{CATALOG}} --project {{PROJECT}} --level {{LEVEL}}

# Re-lock the frozen contracts after an approved ADR.
relock:
    cargo run --package xtask -- contracts
