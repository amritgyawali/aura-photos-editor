# AURA-ML-5018 - Stored faces were produced by a different detector or recogniser version

**Severity / recovery:** see `crates/aura-core/errors.toml` for the registered values.

## What the photographer sees

The registered sentence with a background progress indicator. The People panel stays open and usable; identities, names and roles are all still there.

## What actually happened

Every row in `faces` carries three versions, and they are separate on purpose:

| Column | Bumped by | What a bump invalidates |
|---|---|---|
| `model_ver` | a new `face_detect` release, or a change to the letterbox, padding, resampler or tile layout | every `face_scan` row: the frames must be re-scanned |
| `embed_ver` | a new `face_embed` release | every template: they are no longer comparable |
| `quality_ver` | a new `face_quality` release, or a change to the gate's factors or weights | `faces.votes`, and therefore the grouping |

A project whose faces carry more than one distinct `(model_ver, embed_ver)` pair is mid-migration, and this code reports it.

The reason is arithmetic, not administrative. Cosine distance between a template from one recogniser and one from another *returns a number*. It is in the right range, it looks plausible, and it means nothing. A build that quietly compared them would merge strangers and split families, and nobody could reproduce it.

## What AURA does automatically

Keeps every row, reports the mismatch, and re-scans the stale frames in the background. Nothing is deleted before its replacement exists. Grouping runs on the templates that are current, and `SubjectHierarchy::coverage` reports how much of the wedding that is.

## Operator steps

1. Let it finish. A re-scan costs what the original pass cost - on the processor path, roughly three minutes per thousand images without tiling.
2. If it does not clear, check that the model pack installed: `cargo xtask models` verifies signatures, digests and cards, and a pack that failed to install leaves the previous version active on purpose (`AURA-ML-5009`).
3. A `quality_ver`-only bump does **not** need a re-scan of the pixels, only a regroup: the templates are untouched and `faces.votes` is recomputed. That is why the column is separate from the other two.
4. Do **not** compare clustering results across versions to decide whether the new model is better. `tests/eval/identity_eval.rs` and `ml/models/face/eval_identity.py` exist for that, and they hold the two sets separate.

## Related

- Error registry: `crates/aura-core/errors.toml`
- The embedding equivalent: `docs/runbooks/AURA-ML-5015.md`
- Model rollback: `docs/runbooks/AURA-ML-5009.md`
- Model cards: `docs/model-cards/face_detect.md`, `docs/model-cards/face_embed.md`, `docs/model-cards/face_quality.md`
