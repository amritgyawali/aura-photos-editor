# AURA-ML-5014 - The similarity index snapshot could not be trusted and was discarded

**Severity / recovery:** see `crates/aura-core/errors.toml` for the registered values.

## What the photographer sees

The registered sentence, once, while the project opens. "Find similar" is unavailable for the length of one index build - under half a second on a 4,000-image wedding - and then behaves exactly as before.

## What actually happened

`aura_index::snapshot::Snapshot::load` refused the cached graph. The detail line says which of six checks failed:

| Detail | Meaning |
|---|---|
| `cannot read ...` | The file is absent. Normal on a first open, and on any machine where the cache directory was cleared. |
| `not an AURA index snapshot` | The magic is wrong. Something else wrote that path. |
| `format N, this build reads M` | The snapshot is from another release. |
| `built with different graph parameters` | `M`, `ef_construction` or `ef_search` changed, so the file describes a graph this build would not have built. |
| `vectors are model version N, this build uses M` | The embedding model was updated. |
| `body does not match its digest` | The file is truncated or corrupt - almost always a power cut during the write, although the write is atomic so this is rare. |

## What AURA does automatically

Rebuilds the graph from the `embeddings` rows in the catalog, then writes a fresh snapshot. The snapshot is a cache of a cache: the vectors are catalog truth, the graph is derived from them, and the file is derived from the graph. Nothing photographic depends on it.

A model or preprocessing version mismatch additionally raises `AURA-ML-5015`, which is the more interesting event: the snapshot being stale is a consequence, the vectors being stale is the cause.

## Operator steps

1. On a first open, or after clearing the cache, no action. This is the expected path.
2. If it repeats on every open of the same project, the write is failing rather than the read. Check free space on the cache volume and check that the cache directory is not inside a synchronised folder - `AURA-IO-1008` covers the second case explicitly and would normally have fired first.
3. If `body does not match its digest` appears on a machine more than once, treat it as a disk finding, not an index finding. The write goes to a temporary file and is renamed; a corrupt body after a successful rename means the storage lied about a flush.
4. Deleting the snapshot by hand is always safe.

## Related

- Error registry: `crates/aura-core/errors.toml`
- Version drift: `docs/runbooks/AURA-ML-5015.md`
- Cache placement refusal: `docs/runbooks/AURA-IO-1008.md`
