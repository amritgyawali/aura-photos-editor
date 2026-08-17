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

# The phase 06 gate: the migration, a synthetic wedding scanned for faces, detection
# recall and bokeh rejection against known positions, sealed templates, a cancelled and
# a resumed pass, grouping with reasons, a photographer's decision surviving a regroup,
# the project-scoping refusal, byte-identical face ids across two scans, and an erasure
# that leaves nothing behind. Never touches a network - nothing in phase 06 can.
phase-06-verify:
    cargo run --release --package aura-cli -- verify --phase 06 --work target/phase06-verify

# The phase 07 gate: the migration, the shipped taxonomies and profiles, the scene
# classifier through the real graph, a synthetic wedding labelled and segmented, the
# chapter band and the 45-second boundary error, a photographer's chapter surviving a
# re-analysis, a photographer's scene surviving a re-classification, the coarse labels
# reaching the people graph, the cloud cost policy without a network, determinism, and
# the version check. Never touches a network - nothing in the local half of phase 07 can.
phase-07-verify:
    cargo run --release --package aura-cli -- verify --phase 07 --work target/phase07-verify

# The phase 08 gate: the migration, the thresholds, a grouping whose counts
# reconcile, the five burst patterns measured by ARI, a duplicate set with a
# confidence, two shooters on one instant, a hand split that survives a
# re-grouping, an undo, and two runs that agree. Never touches a network and
# never opens an image file.
phase-08-verify:
    cargo run --release --package aura-cli -- verify --phase 08 --work target/phase08-verify

# The phase 09 gate: the migration, the calibration table and its fairness
# direction, both heads through the real inference service, a wedding's frames
# scored with reasons and evidence crops, a bokeh portrait that is not soft, a
# kiss that is exonerated, shake told from a pan, a candle that is not a blown
# highlight, a photographer's dismissal surviving a re-analysis, the
# within-moment ranking, a stale calibration version healing itself, and two
# runs that agree. Never touches a network - nothing in phase 09 can.
phase-09-verify:
    cargo run --release --package aura-cli -- verify --phase 09 --work target/phase09-verify

# The phase 10 gate: the migration, the emotion weight table and its cultural
# inversion, both heads through the real inference service, a wedding's frames
# read with reasons and face crops, a composed rite frame that is not ranked
# below a smiling one, a peak found and a flat moment refused, a reaction linked
# across two cameras and a burst not linked to itself, a photographer's peak
# choice surviving a re-score, a preference refused across two weddings, a stale
# weights version healing itself, and two runs that agree. Never touches a
# network.
phase-10-verify:
    cargo run --release --package aura-cli -- verify --phase 10 --work target/phase10-verify

# The phase 11 gate: migration and rule coverage, both composition heads through
# the real inference service, explainable fixture judgements and crop hints,
# intentional style and colour-distraction cases, resumable persistence,
# photographer dismissals, version invalidation, telemetry, determinism and all
# quantitative geometry/agreement gates. Never touches a network.
phase-11-verify:
    cargo run --release --package aura-cli -- verify --phase 11 --work target/phase11-verify

# The composition metrics, from the Python side. `--self-test` proves the metric
# harness rejects constant predictions before it is trusted with real labels.
composition-eval:
    python ml/models/composition/eval_composition.py --self-test

# The emotion metrics, from the Python side. `--self-test` proves the metrics
# reject a reader that learned nothing; `--fit-ranker` fits the Bradley-Terry
# coefficients and `--fit-calibration` the per-scene isotonic maps that ship as
# the identity today.
emotion-eval:
    python ml/models/emotion/eval_emotion.py --self-test

# The integrity metrics, from the Python side. `--self-test` proves the metrics
# reject a scorer that learned nothing; `--fit-calibration` fits the per-scene
# isotonic maps that ship as the identity today.
integrity-eval:
    python ml/models/integrity/eval_integrity.py --self-test

# The burst-grouping metrics, from the Python side. `--self-test` proves the
# metrics reject both degenerate groupers; `--ablate` prints what each labelled
# pattern is there to test.
burst-eval:
    python ml/eval/burst_eval.py --self-test
    python ml/eval/burst_eval.py --ablate

# Weight-space parity for every fp32/variant pair, plus the cross-runtime check
# against onnxruntime when it happens to be installed for Python.
parity:
    python ml/export_onnx/verify_parity.py --all models

# Build previews for an existing catalog, the way the app does.
previews CATALOG PROJECT LEVEL="thumb":
    cargo run --release --package aura-cli -- previews --catalog {{CATALOG}} --project {{PROJECT}} --level {{LEVEL}}

# The phase 13 gate: migration 13 and its trigger, the band table, the reason
# registry against all four vocabularies, a real cull recorded end to end, the
# append-only refusal, a supersession, a compaction, every decision replayed, the
# grounding check, the bundle scan and three budgets.
phase-13-verify:
    cargo run --release --package aura-cli -- verify --phase 13 --work target/phase13-verify

# The calibration metrics, from the Python side. `--self-test` proves the
# estimator catches an overconfident predictor and that a fit improves held-out
# ECE; `--outcomes FILE --fit --diagram OUT.svg` reports on real outcomes when
# there are any.
calibration-report:
    python ml/eval/calibration_report.py --self-test

# Regenerate the public reason-code reference from the registry. A phase that
# adds a code and forgets this fails tests/eval/explain_eval.rs.
reason-codes:
    cargo run --package aura-explain --example emit_reason_codes > docs/reason-codes.md

# Re-lock the frozen contracts after an approved ADR.
relock:
    cargo run --package xtask -- contracts
