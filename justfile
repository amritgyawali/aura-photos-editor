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

# The full test suite: Rust plus UI.
test:
    cargo test --workspace --all-targets
    cd ui && npm test

# Budget benchmarks.
bench:
    cargo bench --package aura-ingest

# Generate the three reference weddings into tests/fixtures/generated.
fixtures:
    cargo run --release --package aura-cli -- fixtures --out tests/fixtures/generated

# The phase gate: fixtures, import, re-import, digest comparison, integrity.
phase-01-verify:
    cargo run --release --package aura-cli -- verify --work target/phase01-verify

# Re-lock the frozen contracts after an approved ADR.
relock:
    cargo run --package xtask -- contracts
