# AURA-ML-5016 - The project is larger than the documented in-memory index ceiling

**Severity / recovery:** see `crates/aura-core/errors.toml` for the registered values.

## What the photographer sees

The registered sentence. "Find similar" still works and still returns correct neighbours; it takes single-digit milliseconds longer per query than it would on a smaller project.

## What actually happened

`aura_index::hnsw::IN_MEMORY_CEILING` is 20,000 vectors. Past that, the graph is not built and queries fall back to the exact flat scan in `aura_index::metrics::exact_knn`.

The ceiling is a documented number rather than a discovered one, which is what section 12 of the phase 05 document asks for. A graph that grows until the machine swaps does not fail - it gets slower in a way that looks like the whole application is broken, on the largest and therefore most valuable projects. A stated ceiling with a slower but exact fallback is the honest trade.

The arithmetic behind 20,000: each vector is 512 halves widened to `f32` in memory, so 2 KB, plus a 64-entry neighbour list at layer zero and 32 above it, so roughly 300 bytes of graph. Call it 2.5 KB per image: 50 MB at the ceiling, on a machine the product already asks for 16 GB from. Doubling the ceiling is not the problem; the problem is that nobody has measured recall or build time at 40,000 vectors, and a ceiling nobody has measured past is not a ceiling.

## What AURA does automatically

Falls back to the exact scan. Note what this trades: the fallback is *more* accurate than the graph, not less - it is the answer the graph approximates. What it costs is time, linearly in project size, and the loss of the sub-millisecond time-windowed query that burst grouping relies on.

## Operator steps

1. Nothing is wrong. Confirm the project size and move on.
2. If this is a common shape of project rather than an outlier, that is a product finding, not a support one: the on-disk index named in section 12 as the mitigation for projects over 20,000 images is unbuilt, and it should be scheduled rather than improvised.
3. Splitting one 30,000-image catalog into two projects is a legitimate workaround and costs nothing, because cross-project similarity is not a feature the product offers.

## Related

- Error registry: `crates/aura-core/errors.toml`
- ADR: `docs/adr/ADR-0011-embeddings-and-similarity-index.md`
