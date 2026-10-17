# ADR-0112: Bound memory retained by live-preview pairs

Accepted 2026-10-11.

A native three-photo repeat-edit verification encountered a fatal allocation failure
while other development tools were running. The checkpoint cache had a 1 GiB limit,
but its separate latest-pair list retained strong references to buffers evicted from
that cache. Six full-resolution before/after pairs could therefore retain additional
memory outside the declared limit. The interface and finished-preview caches also
competed with transient rendering and inference buffers.

Checkpoint entries are limited to 256 MiB, and latest pairs have an independent
128 MiB limit in addition to their entry-count limit. A pair above that limit is
not retained for approximate live preview; exact previews and exports still render
normally. Pair accounting conservatively counts both buffers even if shared with
checkpoint entries. Eviction releases strong references from the latest-pair list.
Finished native previews and the interface preview cache each use approximately
128 MiB. Native disk caching remains available for evicted finished previews.

No resolution, rendering algorithm or export setting changes. An eviction may
require recomputation and can cost time. A unit regression checks that evicted
buffers are actually released and oversized pairs are rejected. Native repeat-edit
and full-export checks must be repeated after the change; passing cache unit tests
alone does not establish desktop stability on arbitrary hardware or file sizes.
