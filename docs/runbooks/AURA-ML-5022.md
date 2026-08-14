# AURA-ML-5022 - Stored scene labels came from a different version

**Severity / recovery:** see `crates/aura-core/errors.toml` for the registered values.

## What the photographer sees

The registered sentence, once, above a timeline that keeps working. The re-labelling happens in the background and edits made in the meantime are kept.

## What actually happened

`image_scenes` carries **four** version columns and this code fires when any of them disagrees with the running build:

| Column | Bumped by | Invalidates |
|---|---|---|
| `model_ver` | a change to the scene classifier | the scene posterior and the top-3 |
| `preprocess_ver` | a change to the pixels or context features the classifier sees | the same, for a different reason |
| `taxonomy_ver` | a change to a ritual taxonomy file | the ritual slug only |
| `embed_ver` | a change to the phase 05 embedding trunk | everything, because the trunk is frozen and the head sits on it |

Four columns rather than one, for the reason phase 06 gives for its three: they invalidate different things and re-running everything on a taxonomy edit costs a wedding thirty-five seconds for no reason.

Comparing a posterior produced by one version with a posterior produced by another returns a plausible number that means nothing. That is the failure this code exists to make impossible to have silently, and it is the same argument as `AURA-ML-5015` for embeddings and `AURA-ML-5018` for faces.

## What AURA does automatically

Reports the code with the offending versions and the row count, keeps serving the stale labels, and queues a re-classification of exactly the affected rows. `StoryOutline::scene_ver` reports which version the story underneath was built with, so a caller can decline to draw a conclusion from a mixed set.

**No two versions are ever compared.** `aura_brain_wedding::scene::classifier` filters to the current version before it computes anything.

## Operator steps

1. Read the logged versions. `taxonomy_ver` alone means somebody edited a ritual file; nothing else is stale.
2. `embed_ver` disagreeing means phase 05 re-embedded. The scene head is trained against a specific trunk, so this is the expensive case and the one that must not be skipped.
3. A wedding that keeps reporting this after a full re-classification has rows that are failing to write. Look for `AURA-DB-3006` beside it.
4. Do not "fix" it by widening the comparison. The whole point of the code is that the comparison is refused.

## Related

- Error registry: `crates/aura-core/errors.toml`
- The same failure for embeddings: `docs/runbooks/AURA-ML-5015.md`
- The same failure for faces: `docs/runbooks/AURA-ML-5018.md`
- `docs/adr/ADR-0015-wedding-scene-taxonomy-and-story-segmentation.md`
