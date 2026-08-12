# ADR-0005 - Extending the frozen IPC surface for previews

- **Status:** accepted
- **Date:** 2026-08-12
- **Deciders:** CTO, TLC, SFE (Senior Frontend Engineer)
- **Phase:** 02

## Context

`crates/aura-app/src/contract/ipc.rs` and `ui/src/ipc/types.ts` are frozen
contracts, digested in `contracts.lock`. Phase 02 puts real pixels in the phase
01 grid, which the grid cannot do without new commands. Changing a frozen
contract requires an ADR first and a re-lock second; this is that ADR.

## Decision

Six commands and one event stream are added. Nothing existing is changed or
removed, so every phase 01 caller keeps working.

| Command | Returns | Why it exists |
|---|---|---|
| `get_preview` | `PreviewPayload` | One cell's pixels, decoded if necessary |
| `prefetch_previews` | queued count | A batch, at background priority |
| `cancel_previews` | cancelled count | The user scrolled away |
| `preview_stats` | `CacheStatsDto` | "Previews use X GB of Y" |
| `set_cache_budget` | `CacheStatsDto` | The settings slider |
| `purge_cache` | `CacheStatsDto` | One-click reclaim |

`PreviewEvent` (`ready`, `failed`, `cacheStats`) mirrors `IngestEvent`.

### Pixels cross as a `data:` URL, not as an array

`PreviewPayload.dataUrl` is a complete `data:image/jpeg;base64,...` string.

A 512 px thumbnail is 786,432 bytes of RGB. Serialised as a JSON array of
numbers that is roughly 3 MB of text per cell, parsed into a JavaScript array,
then copied into an `ImageData`. Base64 of the JPEG is about 25 kB and can be
assigned straight to `img.src`, where the browser decodes it on its own threads.
For a 4,000-cell grid the difference is the whole feature.

The base64 encoder is twenty lines in `preview_commands.rs` rather than a
dependency; encoding is not a place that benefits from a supply-chain review.

### Full-resolution renders never cross the boundary

`level` accepts `thumb` and `proxy`. A tier 3 decode is hundreds of megabytes of
tiles and belongs to the render pipeline in a later phase, not to a web view.
Asking for anything else yields the thumbnail.

### One preview service per project

`AppState::previews(project_id)` creates a service per project, cached in the
state. Each wedding therefore has its own cache directory, its own budget and its
own accounting, which is what a photographer expects when they archive one job
and keep another open.

## Consequences

- `contracts.lock` is re-locked in the same commit as this ADR; the two files
  must move together or CI fails, which is the mechanism working as intended.
- `ipc_contract.rs` gains assertions for each new type and each new event
  variant, so a Rust field without a TypeScript field fails the build rather
  than becoming `undefined` in a photographer's grid.
- The UI gains `ui/src/stores/thumbnailStore.ts`. The phase document names this
  file `apps/desktop/src/stores/thumbnailStore.ts`; the repository's UI lives at
  `ui/` (ADR-0002 section 6), so the store lives at the same relative path under
  that root.
