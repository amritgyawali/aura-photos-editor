# Runbook - how to add a model

A model is not a file you drop into `models/`. It is a signed entry in a pinned
manifest with a card, a precision policy and a declared working set, and every one
of those is checked mechanically before it can run.

## Before you start

Read `docs/model-cards/TEMPLATE.md` and
`docs/adr/ADR-0007-inference-runtime.md`. The second one tells you which
operators exist; a model outside that set will not load, and finding that out
after training is expensive.

## 1. Export inside the subset

```bash
python ml/export_onnx/export.py --torch mymodule:build_model --out models
```

The exporter checks the written file against the operator list, which it reads
out of `crates/aura-infer/src/onnx/ops/mod.rs` - one source of truth rather than
two lists that drift. An operator outside the set fails the export.

If you need an operator we do not implement, implement it: an operator is a file
in `crates/aura-infer/src/onnx/ops/`, a match arm, and tests. Do not work around
it by exporting at a different opset.

## 2. Produce the variants

```bash
python ml/export_onnx/quantise.py models/mymodel_1.0.0.fp32.onnx --fp16
python ml/export_onnx/quantise.py models/mymodel_1.0.0.fp32.onnx --int8
```

Skip `--int8` for anything that touches skin, colour or retouch, and set
`forbid_int8` in its precision policy instead (section 12 of the phase document).
The policy is the only place that knowledge can live.

## 3. Check parity

```bash
python ml/export_onnx/verify_parity.py --all models
```

fp16 must be within 1e-3 of fp32 and int8 within 1e-2. If onnxruntime happens to
be installed, `--against-runtime` also compares AURA's own interpreter against it
on the same file, which is the only independent check of the interpreter that
exists in this project.

## 4. Write the model card

Copy `docs/model-cards/TEMPLATE.md` to `docs/model-cards/<name>.md` and fill in
all seven sections. Latency numbers come from a run, never from an estimate
(Article XXIII rule AI7); leave a reference machine's row empty rather than
guessing it.

## 5. Add it to the manifest

Add a `ModelEntry` in `xtask/src/models.rs` - name, version, task, class, input
spec, outputs, licence, card path, `min_app_version`, `working_set_mb`, precision
policy and opset - then:

```bash
cargo run -p xtask -- models --generate
```

That writes the files, computes the digests, renders `models/models.lock` and
signs it with the **development** key.

`working_set_mb` is the peak the model needs for one image. The scheduler admits
work against it, so a number that is too low is how a machine runs out of memory
at image 2,800 of 3,000. Measure it; do not estimate it.

## 6. Sign for release

The development key is derived from a public seed phrase and is worthless for
release - everyone who can read this repository can compute it. A release pack is
signed on the offline signing machine:

```bash
model-sign sign --key /media/offline/release.key models/models.lock
```

The private key never enters this repository, this machine or CI. The
corresponding public key is compiled into shipping builds through
`AURA_RELEASE_PUBLIC_KEY`.

## 7. Check what CI will check

```bash
cargo run -p xtask -- models      # signature, digests, cards, opset, and it must compile
cargo test -p aura-infer -p aura-models
cargo run --release -p aura-cli -- verify --phase 03
```

## What blocks a merge

- A missing or incomplete model card (`AURA-ML-5005`).
- An operator outside the subset (`AURA-ML-5006`).
- A digest that does not match its file, or a manifest edited after signing.
- A `working_set_mb` of zero.
- A model added without an entry in `xtask/src/models.rs` - a file in `models/`
  that nothing pins is invisible to the registry and will never load.

## Related

- Model card template: `docs/model-cards/TEMPLATE.md`
- Update failures: `docs/runbooks/model-update-failed.md`
- Runtime ADR: `docs/adr/ADR-0007-inference-runtime.md`
