# ADR-0012 - Extending the frozen IPC surface for similarity

- **Status:** accepted
- **Date:** 2026-08-13
- **Deciders:** CTO, SFE (Senior Frontend Engineer), MFE, UX, SEC
- **Phase:** 05

## Context

`crates/aura-app/src/contract/ipc.rs` and `ui/src/ipc/types.ts` are frozen
contracts, digested in `contracts.lock`. Section 9 of the phase document assigns
SFE a "debug 'find similar' panel with distance readout and cluster preview", and
section 8 step 8 calls it "invaluable for later phases". Acceptance criterion 2 -
"'Find similar' returns visually correct neighbours in under 5 ms on a
4,000-image project" - cannot be demonstrated to a human without it.

Changing a frozen contract requires an ADR first and a re-lock second. This is
that ADR, following ADR-0005 (phase 02), ADR-0008 (phase 03) and ADR-0010
(phase 04).

## Decision

Five commands, five DTOs and one event stream are added. Nothing existing
changes.

| Command | Returns | Why it exists |
|---|---|---|
| `find_similar` | `SimilarResultDto` | The query itself: neighbours, distances, and how long it took |
| `index_status` | `IndexStatusDto` | Vectors indexed, coverage, whether the snapshot was used, build time |
| `build_index` | `IndexStatusDto` | Force a rebuild after a model version change |
| `embed_project` | `EmbedProgressDto` | Embed everything not yet embedded, resumably |
| `image_descriptors` | `DescriptorsDto` | The cheap descriptors for one frame, for the debug panel's readout |

`IndexEvent` (`embedProgress`, `indexBuilt`, `queryTimed`) mirrors
`PreviewEvent`, `InferEvent` and `CloudEvent`, and carries exactly section 11's
three telemetry events: `embed.batch`, `index.build` and `index.query`.

### What the surface deliberately carries

**A distance and a similarity, both.** `SimilarNeighbourDto` has `distance`
(cosine distance, 0 is identical) and `similarity` (`1 - distance`). Two numbers
for one fact is normally a smell; here it is the difference between a debug panel
that reads "0.043" and one that reads "96 % alike", and the conversion being done
once in Rust means the two panels that will exist by phase 25 cannot disagree
about which direction is better.

**`dhashDistance` on every neighbour.** The Hamming distance between the query's
dHash and the neighbour's, so the panel can show *why* something is a near
duplicate without a second round trip. `0` means the two frames are bit-identical
after the perceptual hash - the thing phase 08 will act on.

**Every reason a query would return nothing.** `IndexStatusDto.coverage` is
embedded images over total images, and `staleModelVersions` lists any version
still sitting in the catalog that is not the current one. An empty result set on
a project that was never embedded is a support ticket; this answers it before it
is filed.

**`filterKind` on the event, not the filter.** `IndexEvent::queryTimed` carries
`k`, `ms` and `filterKind` (`none`, `time`, `camera`, `scene`, `composite`) -
section 11's `index.query` event exactly. The filter's *contents* are not
telemetry: a time window plus a camera identifies a shoot.

**Milliseconds as a float, so the DTOs are `PartialEq` and not `Eq`.** A 5 ms
budget measured in whole milliseconds is measured in units of the budget. Same
accommodation `CacheStatsDto` made in phase 02.

### What it deliberately does not carry

- **No command returns a vector.** There is no `get_embedding`. 512 halves per
  image across a 4,000-image grid is 4 MB of JSON per screen, and a web view has
  no use for a number it cannot compare. The comparison happens in Rust.
- **No cluster assignment.** Clustering is phase 07 (scenes) and phase 08
  (bursts). `find_similar` returns neighbours and distances; what a cluster *is*
  is not decided here.
- **No duplicate verdict.** `dhashDistance` is evidence. The policy that turns
  evidence into "this is a duplicate, delete it" is phase 08, and putting a
  threshold in an IPC DTO now would freeze a number phase 08 has to own.
- **No write path.** Nothing in this surface edits a photograph. `build_index`
  and `embed_project` rebuild caches and rows that are derived from pixels the
  product never modifies.

### The panel

`ui/src/components/SimilarPanel.tsx`, following `HardwarePanel.tsx` and
`AiKeysPanel.tsx`. The phase document names no path for it. It is a debug surface
and is behind the same feature flag as the rest of the phase
(`setting: index.enabled`), so acceptance criterion "rollback path exists: feature
flag off" covers the UI as well as the pipeline.

## Consequences

- `contracts.lock` re-locked for `ipc.rs` and `types.ts`.
- The two files must stay in step; `crates/aura-app/tests/ipc_contract.rs` grows
  the assertions that prove it, as it did in phases 02, 03 and 04.
- Phase 07's scene panel and phase 08's burst panel inherit `SimilarNeighbourDto`
  rather than each inventing a neighbour shape.
- `IndexEvent` is typed on both sides and not yet emitted to the UI, for the same
  reason `IngestEvent`, `InferEvent` and `CloudEvent` are not: the Tauri shell has
  not been launched on the development machine, so an emitter would be code
  nobody has run.
