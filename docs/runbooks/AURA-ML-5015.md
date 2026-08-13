# AURA-ML-5015 - Stored vectors were produced by a different embedding version

**Severity / recovery:** see `crates/aura-core/errors.toml` for the registered values.

## What the photographer sees

The registered sentence, with a background progress indicator. Grouping, "find similar" and duplicate detection stay available on the vectors that are current while the rest are recomputed.

## What actually happened

Every row in `embeddings` carries `model_ver` and `preprocess_ver`. A project whose rows carry more than one distinct pair is mid-migration, and this code reports it.

Section 12 of the phase 05 document names this as the failure mode worth engineering against, and the reason is arithmetic rather than administrative: cosine distance between a vector from one model and a vector from another *returns a number*. It is a plausible number in the right range, and it means nothing. A build that quietly compared them would produce burst groups and duplicate pairs that no one could reproduce or explain.

Two things bump a version:

* **the model** - a new `wedding_embedding` release, which is what a model pack update does;
* **the preprocessing** - a change to the crop, the resampler, the channel order or the normalisation in `aura_vision::embed::model`. `PREPROCESS_VER` exists so that this is a version bump rather than silent drift, because the interpreter cannot fuse preprocessing into the graph (ADR-0011 section 4).

## What AURA does automatically

Keeps every row, reports the mismatch, and re-embeds the stale ones in the background. Nothing is deleted before its replacement exists. The index is built from the rows at the majority version and reports the remainder in `IndexStats::stale`, so a short result list has a visible explanation rather than looking like a bug.

The snapshot is invalidated as a consequence, which is `AURA-ML-5014`.

## Operator steps

1. Let it finish. A re-embed is the same cost as the original pass - about two and a half minutes per thousand images on a processor-only machine - and it is off the critical path.
2. If it does not clear, check that the model pack actually installed: `cargo xtask models` verifies the signature, the digests and the cards, and a pack that failed to install leaves the old version active on purpose (`AURA-ML-5009`).
3. To force the issue, `EmbeddingStore::purge_version` forgets one version's rows and the next pass rebuilds them. This is the rollback switch, and it is safe: the rows are derived from pixels that were never modified.
4. Do **not** compare scores across versions to decide whether the new model is better. That comparison is what the evaluation harness in `tests/eval/embedding_eval.rs` and `ml/models/embed/eval_retrieval.py` are for, and they hold the two sets separate.

## Related

- Error registry: `crates/aura-core/errors.toml`
- Snapshot rejection: `docs/runbooks/AURA-ML-5014.md`
- Model rollback: `docs/runbooks/AURA-ML-5009.md`
- Model card: `docs/model-cards/wedding_embedding.md`
