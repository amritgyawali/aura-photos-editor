# Phase 02 - progress log

One line per task: what was touched, what was tested, what it cost. Ordered as
the work happened, which follows section 8 of the phase document.

| Task | Files touched | Tests added | Notes |
|---|---|---|---|
| Kickoff, error registry | `crates/aura-core/errors.toml`, `src/errors/raw.rs`, `src/errors/io.rs`, 9 runbooks | `error_registry` (existing, now covers 9 new codes) | `AURA-RAW-2001..2008` and `AURA-IO-1009` registered before any decoder existed |
| Frozen contracts | `aura-raw/src/contract/{pixels,sidecar}.rs`, `aura-preview/src/contract/service.rs` | contract digests in `contracts.lock` | Section 5 shapes written as code first, per the phase ritual |
| Colour design (ADR) | `docs/adr/ADR-0003-colour-pipeline.md` | - | Working space, curve and profile chain decided before tier 2 |
| Colour implementation | `aura-raw/src/colour/{matrix,working_space,curve,profile,de2000}.rs` | `colour_maths.rs` (18) | dE2000 checked against Sharma/Wu/Dalal table 1 |
| Decode backend (ADR) | `docs/adr/ADR-0004-raw-decode-backend.md` | - | Pure Rust rather than LibRaw; licence, `forbid(unsafe_code)`, toolchain |
| Containers | `aura-raw/src/container/{tiff,jpeg,bmff,raf}.rs`, `format.rs`, `meta.rs` | `containers.rs` (17) | Bounds-checked, loop-proof, magic-based sniffing |
| Tier 1 | `aura-raw/src/{thumb,orientation,codec}.rs` | `tiers.rs` (part) | Embedded JPEG, orientation, quarter-size fallback with `AURA-RAW-2003` |
| Mosaic decode | `aura-raw/src/{cfa,losslessjpeg,demosaic}.rs` | `tiers.rs`, `fuzz_decode.rs` | SOF3 decoder; packed 10/12/14-bit; half-size bin |
| Tier 2 | `aura-raw/src/proxy.rs` | `tiers.rs` (part) | Both buffers, clipping statistics, embedded fallback with `AURA-RAW-2007` |
| Fixtures | `aura-raw/src/fixtures.rs` | used by every suite | Eight bench bodies, three encodings, a colour chart, a lossless-JPEG encoder |
| Watchdog and limits | `aura-raw/src/timeout.rs` | `tiers.rs` (timeout, ceiling) | Deadline per tier; dimensions checked before allocation |
| Cache | `aura-cache/src/{paths,lru,budget,store}.rs` | `cache.rs` (18) | Content-addressed, sharded, budgeted, digest-verified, self-rebuilding index |
| Priority queue | `aura-preview/src/{request,priority}.rs` | `priority.rs` (11) | Strict priority, de-duplication, promotion, cancel-on-scroll |
| Worker pool | `aura-preview/src/pool.rs` | covered by `service.rs` | `cores - 1` workers so a visible request always has a core |
| Service | `aura-preview/src/{service,source}.rs`, `aura-catalog/src/repo.rs` | `service.rs` (9) | Memory, disk, decode; recording; quarantine; telemetry |
| Tier 3 | `aura-raw/src/full.rs` | `tiers.rs` (tiled equivalence) | 512 px tiles with a 32 px halo, identical to a whole-image decode |
| IPC (ADR) | `docs/adr/ADR-0005-preview-ipc-surface.md`, `aura-app/src/contract/ipc.rs`, `ui/src/ipc/types.ts` | `ipc_contract.rs` (8) | Six commands, one event stream, pixels as a `data:` URL |
| App commands | `aura-app/src/{preview_commands,state}.rs` | `ipc_contract.rs` | One preview service per project |
| UI | `ui/src/stores/thumbnailStore.ts`, `components/grid/{Cell,VirtualGrid}.tsx`, `components/CacheSettings.tsx`, `App.tsx`, `ipc/client.ts` | `thumbnailStore.test.ts` (8) | Real pixels, LRU ceiling, cancel-on-scroll, cache panel |
| CLI and gate | `aura-cli/src/main.rs`, `justfile`, `.github/workflows/ci.yml` | phase-02 gate | `raw-fixtures`, `previews`, `verify --phase 02` |
| Budgets | `perf/budgets.toml`, `aura-perf/src/lib.rs`, `tests/preview_budgets.rs` | `preview_budgets.rs` (6) | Stage budgets plus size ceilings |
| Performance probe | `aura-raw/tests/scaling_probe.rs` | 1 (ignored) | Real cost per megapixel up to 25 MP; feeds the ADR-0004 waiver |
| Docs | `docs/camera-support.md`, `docs/runbooks/previews.md`, `CHANGELOG.md` | - | Honest per-format matrix |

## Follow-up: the codecs ADR-0004 had refused

Added after the first exit report, in the same phase because they change no
frozen contract and no cached pixel: `pipeline_ver` is untouched.

| Task | Files touched | Tests added | Notes |
|---|---|---|---|
| Shared bit reader | `aura-raw/src/codecs/{mod,bits}.rs` | covered by every codec test | One MSB reader and one flat Huffman table, so the bounds checking is written once |
| Nikon compressed NEF | `aura-raw/src/codecs/nikon.rs`, `meta.rs` | `codecs.rs` (3) | Six published trees, the MakerNote decode table, and an encoder for the round trip |
| Sony ARW2 | `aura-raw/src/codecs/sony.rs` | `codecs.rs` (4) | Sixteen photosites per sixteen bytes; linear fallback when the curve is unreachable |
| Olympus ORF | `aura-raw/src/codecs/olympus.rs` | `codecs.rs` (2) | Adaptive predictive coding; detected by "declares uncompressed, stores too little" |
| X-Trans | `meta.rs`, `demosaic.rs`, `container/raf.rs` | `codecs.rs` (4), `tiers.rs` (1) | 3x3 binning, 5x5 interpolation, RAF block directory, tiled equivalence |
| Scheme dispatch | `meta.rs` (`MosaicScheme`), `cfa.rs`, `format.rs` | existing suites | The decoder choice is made once, during the container walk |
| Parallel decode | `demosaic.rs`, `cfa.rs` | existing suites (output is bit-identical) | Rows are independent; each writes its own slice |
| Gate coverage | `aura-cli/src/main.rs`, `fixtures.rs` | phase-02 gate | Seven encodings cycled across the eight bench bodies |

## Defects the tests caught

1. **`Orientation::Transverse` transposed the wrong axes.** The source lookup
   used `(height - 1 - y, width - 1 - x)` where the destination `x` indexes rows
   of the source, so any non-square image underflowed and panicked. Caught by
   `every_orientation_is_a_permutation_of_the_same_pixels` on a 2x3 frame; a
   square test fixture would have missed it entirely.
2. **White balance was applied the wrong way round.** DNG's `AsShotNeutral`
   stores the camera's reading of a neutral subject, so the multipliers are its
   reciprocals. The first implementation used the values directly, which tints
   every frame by the square of the true cast - plausible enough on a daylight
   frame to survive review. Caught by the ColorChecker test, which is deliberately
   written with a non-unit white balance.
3. **The gamut rotation was missing from the 8-bit output.** The working buffer
   is Rec.2020 and the display buffer is sRGB; the first version applied the
   curve without the rotation, desaturating every saturated colour by exactly the
   gamut difference. Caught by the chart round trip.
4. **A camera JPEG was being curved twice.** The embedded-preview fallback ran
   the display-referred camera JPEG through the scene-referred preview curve,
   doubling its contrast. Fixed by carrying a `curved` flag through the render,
   which is also why `PixelSource` matters downstream.
5. **Cache hit rates were measured as lifetime totals.** The phase gate compared
   absolute miss counts against a threshold, but the counters persist in the
   on-disk index, so a second pass looked like 25 misses. Now measured as a delta
   across the pass.
