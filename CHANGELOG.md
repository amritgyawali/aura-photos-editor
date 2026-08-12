# Changelog

All notable changes to AURA. One entry per phase, newest first.

## Phase 02.1 - Proprietary mosaic codecs, X-Trans, and a parallel decode path

A follow-up to phase 02 that closes most of the camera-coverage gap ADR-0004
opened, and narrows the performance waiver it recorded. No frozen contract
changed, so `pipeline_ver` is unchanged and cached previews stay valid.

### Added

- `aura-raw::codecs`: independent safe-Rust implementations of three formats the
  first cut refused - Nikon's compressed NEF (Huffman coding plus the body's
  linearisation curve, read from MakerNote `0x0096`/`0x008C`), Sony's ARW2 block
  coding, and Olympus's adaptive predictive ORF. Each ships with an **encoder**,
  so every decoder is tested by round trip rather than by assertion.
- X-Trans support end to end: the 6x6 array is read from a DNG's `CFAPattern` or
  a RAF's block directory, binning uses a 3x3 block instead of a 2x2 quad, and
  interpolation widens to 5x5 because a 3x3 window on X-Trans can contain no red
  at all. Tiled tier 3 stays bit-identical to a whole-image decode.
- Fujifilm RAF block-directory parsing: sensor dimensions and colour layout, plus
  the uncompressed mosaic.
- `MosaicScheme`: which decoder a mosaic needs, decided once during the container
  walk. A file that declares no compression but stores too few bytes for its own
  bit depth is now recognised as compressed, which is how Olympus marks its
  scheme.
- 16 tests in `crates/aura-raw/tests/codecs.rs`, and the new encodings added to
  the tier-2 equivalence test and to the `verify --phase 02` cycle.

### Changed

- Demosaic, area-average resize, the colour rotation and the mosaic unpack are
  parallel over output rows. Each row writes into its own slice, so output is
  bit-identical whatever the thread count - invariant 4 rules out a parallel
  float reduction here. Small images stay serial.
- `docs/camera-support.md` and ADR-0004 rewritten around the new matrix.

### Known gaps

- Canon CRX (CR3) and Panasonic RW2 are still not decoded, and compressed RAF
  is not either. Reasons per format are in ADR-0004; all three fall back to the
  embedded preview with `AURA-RAW-2007`.
- A compressed NEF whose decode table we cannot read is refused rather than
  rendered through an invented curve.
- Sony's linearisation curve lives in an encrypted sub-directory. When it is not
  reachable the render uses a documented linear expansion.
- The ADR-0004 performance waiver is renewed, not closed: parallelism made tier 3
  2.1x faster and tier 2 1.4x faster at 25 MP, which is not enough to bring a
  45 MP frame inside budget. Measurements and the two remaining routes are in
  the ADR.

## Phase 02 - RAW decode engine and the three-tier preview pyramid

**Shipped:** instant, colour-correct previews for every RAW - the camera's
embedded JPEG for triage, a 2048 px proxy for AI, and on-demand full-resolution
decode for final render.

### Added

- `aura-raw`: container parsers (TIFF/EXIF, JPEG, ISO base media, Fujifilm RAF),
  format sniffing by magic bytes, CFA unpacking for 8/10/12/14/16-bit and
  lossless JPEG (SOF3), half-size and full demosaic, tiled full-resolution
  decode, EXIF orientation, and a per-file watchdog with memory ceilings.
- `aura-raw::colour`: linear Rec.2020 working space, Bradford adaptation, the
  neutral `filmic_lite` preview curve, the camera-profile resolution chain and a
  CIEDE2000 implementation checked against published worked examples.
- `aura-cache`: content-addressed preview cache keyed by BLAKE3 plus
  `pipeline_ver`, with LRU eviction, a hard budget, digest verification on read
  and an index that rebuilds itself by scanning.
- `aura-preview`: the frozen `PreviewService` trait, strict-priority scheduling
  with de-duplication and promotion, a worker pool that leaves one core free for
  the person, and the catalog-backed source.
- IPC: `get_preview`, `prefetch_previews`, `cancel_previews`, `preview_stats`,
  `set_cache_budget`, `purge_cache`, plus the `PreviewEvent` stream.
- UI: real pixels in the grid, an LRU thumbnail store with cancel-on-scroll, and
  a cache settings panel showing "previews use X GB of Y".
- `aura-cli`: `raw-fixtures`, `previews`, and `verify --phase 02`.
- Synthetic RAW fixtures: eight bench bodies, three mosaic encodings and a
  colour chart, so the decoder is tested without a single camera file.
- Docs: `docs/camera-support.md`, `docs/runbooks/previews.md`, ADR-0003
  (colour pipeline), ADR-0004 (decode backend), ADR-0005 (preview IPC).

### Changed

- `aura-catalog`: `preview` table now written and read (`upsert_preview`,
  `preview_row`, `count_previews`, `photos_without_preview`,
  `primary_file_for_photo`).
- `perf/budgets.toml`: phase 02 stage budgets, plus size budgets for the cache
  and for peak resident memory.
- Frozen contracts re-locked for the preview IPC additions (ADR-0005).

### Known gaps

- Proprietary mosaic compressions (compressed NEF and ARW, RW2, Canon CRX,
  X-Trans) are not decoded; those files render tier 2 from the embedded preview
  and are flagged `AURA-RAW-2007`. See `docs/camera-support.md`.
- The scalar CPU decoder misses the per-image budget at 45 MP; waived for this
  phase in ADR-0004 with measurements.
- No GPU path, no HEIF.

## Phase 01 - Foundation, catalog and wedding ingest

Workspace, error taxonomy with runbooks, SQLite catalog with the six-step
refusal chain, idempotent ingest with multi-camera clock alignment, the job
graph with leases, the typed IPC surface, the virtualised grid, fixtures, CI and
budgets. See `docs/progress/PHASE-01-EXIT.md`.
