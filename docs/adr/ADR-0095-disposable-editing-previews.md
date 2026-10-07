# ADR-0095: Disposable editing previews

Status: accepted
Date: 2026-10-07

## Problem

A real 2048 x 3072 portrait with 24 saved retouch operations took about 20 seconds
per 1600-pixel native retouch preview, even when the same recipe was requested twice.
The display payload was 6.8 million base64 characters. The same edits requested at
768 pixels took 3.3 seconds. The retouch canvas normally displays a much smaller image.

## Decision

Interactive Develop, Studio and native retouch renders use disposable proxies with a
768-pixel longest edge. Smaller requests retain their smaller size; small originals
are never enlarged. Saved and draft selection overlays use the same preview size.
Original coordinates, brush paths, mattes and recipe values remain unchanged.

The application owns a catalog-scoped, shared, bounded LRU of edited preview pixels:
32 MiB and at most 16 entries. Its key includes photograph ID, preview size and the
renderer hash (original content, canonical recipe, engine and output specification).
Before/after views and draft recipes have distinct keys when their pixels differ.
Undo/Redo can reuse a recent matching recipe; edited results cannot cross photographs,
sizes or output spaces. No failed render is cached, and the lock is released during
rendering. Cache state is disposable and does not persist across app restarts.

Analysis and export requests bypass this limit and this edited cache. Delivery continues
to request `RenderLevel::Full` from `CatalogFrames`, which decodes the original at full
resolution and never substitutes a preview. Originals are opened read-only; export
writes a separate file. No frozen IPC or render contract changes are required.

## Consequences and verification

Fit previews load faster and carry fewer pixels through IPC. Fine pore/blemish decisions
can differ with resolution; the small preview is not full-resolution quality inspection.
Export remains expensive because it applies the complete recipe to the original.
First use may still build the existing 2048-pixel source proxy; cached edited previews
avoid redoing it and all editing stages on subsequent matching requests.

`crates/aura-app/tests/preview_proxy.rs` checks preview sizing, cache isolation by size
and recipe, Undo/Redo, overlay alignment, unchanged recipes and originals, full analysis,
and real verified full-resolution JPEG delivery after priming the preview cache.
The cache unit test covers LRU eviction and budget enforcement. The native application
must also be benchmarked on the same real portrait and its final export inspected.

## Native validation, 2026-10-07

Built and launched the changed checkout at `C:\Users\amrit\aura-c`, preserving the
current isolated catalog and its 24-operation recipe. On the same portrait:

| Measurement | Before | After |
| --- | ---: | ---: |
| First retouch preview | 20.24 s | 3.59 s |
| Repeated retouch preview | 19.86 s | 0.12 s |
| Preview dimensions | 1066 x 1600 | 512 x 768 |
| Base64 RGB characters | 6,822,400 | 1,572,864 |

The recipe hash remained identical across restart and Undo/Redo. First uncached Undo
took 5.08 seconds; subsequent cached Undo/Redo UI actions took 0.89-1.04 seconds.
Saved selection overlays matched the 512 x 768 preview. Full JPEG export remained
2048 x 3072, read-back verified, and byte-identical to the pre-update export (883,858
bytes). The original's SHA256 remained unchanged. Full export took 150 seconds while
other code checks were running; this is not an export-speed improvement.

The preview integration test, edited-cache eviction/budget test, existing JPEG and PNG
import/export regressions, formatting check, and library Clippy checks with warnings
denied passed. Native evidence and the preserved catalog are under
`C:\Users\amrit\aura-edit-20261007` (`preview-before.json`, `preview-after.json`,
`preview-update-verification.json`, and `preview-updated/verification.json`).
