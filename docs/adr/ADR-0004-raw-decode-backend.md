# ADR-0004 - The RAW decode backend is pure Rust, not LibRaw

- **Status:** accepted
- **Date:** 2026-08-12
- **Deciders:** CTO, SRC (Senior Engineer - Core Pipeline), SEC, DEVOPS
- **Phase:** 02

## Context

Phase 02 section 4 specifies `crates/aura-raw/src/{ffi,libraw_sys}.rs`: a LibRaw
FFI wrapper with `unsafe` isolated into one module. LibRaw is the obvious choice
- it reads essentially every camera ever made - and it is the choice this ADR
declines, for four reasons that are each independently sufficient.

1. **Licence.** LibRaw ships under LGPL-2.1 or CDDL. `deny.toml` allows MIT,
   Apache-2.0, BSD, ISC, Unicode-3.0, Zlib and MPL-2.0 and nothing else, and the
   supply-chain lane fails the build on anything outside that list. Changing the
   licence policy for a desktop product we intend to sell is a decision several
   orders of magnitude larger than a decoder choice.
2. **`forbid(unsafe_code)`.** Every crate root in this repository carries it. An
   FFI wrapper would be the first exception, in the one module that meets
   untrusted bytes from a stranger's memory card.
3. **Toolchain.** Linking LibRaw needs a C++ toolchain and a vendored build on
   Windows, macOS and Linux. The development machine for this phase has MSVC
   without a Windows SDK (ADR-0002 section 7) and cannot link C at all.
4. **The same crates are unavailable.** The pure-Rust alternatives that wrap or
   reimplement LibRaw's coverage - `rawloader`, `imagepipe` - are LGPL-2.1 as
   well, so they fail the same licence gate.

## Decision

`aura-raw` decodes in pure, safe Rust, written in this repository:

| Concern | Module |
|---|---|
| TIFF/EXIF directories | `container/tiff.rs` |
| JPEG marker walking | `container/jpeg.rs` |
| ISO base media (CR3) | `container/bmff.rs` |
| Fujifilm RAF header and block directory | `container/raf.rs` |
| Format sniffing by magic | `format.rs` |
| Which decoder a mosaic needs | `meta.rs` (`MosaicScheme`) |
| Bit-packed and 16-bit CFA | `cfa.rs` |
| Lossless JPEG (SOF3) | `losslessjpeg.rs` |
| Shared MSB bit reader and flat Huffman | `codecs/bits.rs` |
| Nikon compressed NEF | `codecs/nikon.rs` |
| Sony ARW2 block coding | `codecs/sony.rs` |
| Olympus compressed ORF | `codecs/olympus.rs` |
| Demosaic (Bayer and X-Trans), resize | `demosaic.rs` |
| Colour | `colour/` |

### The proprietary schemes

The three manufacturer codecs were added after the first cut of this ADR, which
had refused them. They are implemented from the format descriptions the
open-source photography community has maintained for two decades - dcraw,
LibRaw, RawTherapee and darktable all document the same trees, predictors and
state machines - and written fresh in safe Rust rather than ported, so nothing
from an LGPL codebase is copied into ours. What is shared is the format itself,
which is a fact about a file and not a work of authorship.

Each one ships with an **encoder** in `fixtures.rs`. That is the load-bearing
part of the argument: with no camera files in the repository, a decoder can only
be proved against something, and a second implementation walking the same state
machine forwards is a far stronger something than one sample file would be. A
mistake in Olympus's adaptive width or Nikon's split predictor desynchronises
the pair immediately.

What the round trip cannot prove is that the published format is the format the
camera writes. That still needs one file per body, and it stays an entry
condition for phase 03.

Baseline JPEG decode uses `zune-jpeg` and encode uses `jpeg-encoder`; both are
permissively licensed, pure Rust and pass `cargo deny`.

The crate keeps `#![forbid(unsafe_code)]`. Every read from a file is
bounds-checked, every allocation is preceded by a dimension check against
`DecodeLimits`, and the container parsers have a fuzz suite.

## Consequences: the honest support matrix

This decision buys safety and shippability, and it costs camera coverage. The
full per-format matrix is `docs/camera-support.md`; the summary is:

| Capability | Coverage today |
|---|---|
| Tier 1 (embedded preview) | Every container we can open: DNG, CR2, CR3, NEF, ARW, ORF, RW2, PEF, SRW, RAF, TIFF, JPEG |
| Tier 2/3 from the mosaic | Uncompressed and bit-packed CFA (8/10/12/14/16-bit), lossless JPEG (SOF3), Nikon's compressed NEF, Sony's ARW2, Olympus's compressed ORF, and X-Trans arrays wherever the container gives us the layout |
| Tier 2 fallback | Any file whose mosaic we cannot decompress renders its proxy from the embedded preview, tagged `source = embedded`, with `AURA-RAW-2007` |
| Not decoded at all | Canon CRX (CR3), Panasonic RW2, compressed RAF, HEIF/HEIC |

Two compressions are still refused, both deliberately:

- **Canon CRX.** A wavelet transform with adaptive Golomb-Rice coding, an order
  of magnitude more code than the three schemes above and with no way to
  validate it here. It is the top of the phase 03 decoder backlog, because CR3
  is what current Canon bodies write.
- **Panasonic RW2.** The bit reader is a backwards ring buffer whose priming
  offset varies by generation. Writing it without a file to check against would
  be guessing, and a guess that decodes to plausible-looking noise is worse than
  a documented refusal.

One more is refused for a different reason: a **compressed NEF whose decode
table we cannot read**. The linearisation curve is per body, so there is nothing
to fall back on but an invented curve, and an invented curve is a silent colour
error. It raises `AURA-RAW-2007` like any other unsupported mosaic.

Every gap is recorded rather than hidden: it raises a registered code, it appears
in the Problems list as a degraded render, and it is visible in telemetry.

## Alternatives considered

- **Vendor LibRaw and change `deny.toml`.** Rejected: LGPL dynamic-linking
  obligations in a signed, notarised desktop binary are a legal question, not an
  engineering one, and this phase is not the place to answer it.
- **Ship a LibRaw-linked build behind a feature flag.** Deferred, not rejected.
  The seam exists: `EngineMeta.libraw` is already in the frozen sidecar and is
  `null` in this build, so a future LibRaw-linked build writes into the same
  shape and the two can be told apart in any dataset. Turning that seam on
  requires an ADR amendment, a licence decision and a re-run of the golden suite.
- **Convert everything to DNG on import with Adobe's converter.** Rejected:
  requires a per-machine third-party install, doubles the storage of a 220 GB
  wedding, and makes ingest depend on a program we do not control.

## Deviations from the phase document that follow from this

- `src/ffi.rs` and `src/libraw_sys.rs` do not exist. The modules listed in the
  table above replace them.
- The GPU-assisted proxy fast path (section 9, `SRG`) is not built either: with
  no GPU pipeline in the workspace yet, the CPU path is the only path, and the
  budget asserted in `perf/budgets.toml` is the CPU-only figure of 250 ms rather
  than the GPU-assisted 120 ms. `aura-render` arrives with the render graph in a
  later phase; the proxy path is written so the resize and curve are the only two
  places that would move onto a GPU.

## Performance waiver (PERF + CTO, 2026-08-12, renewed and narrowed)

The decode path is now parallel over output rows. Demosaic, area-average resize,
the colour rotation and the mosaic unpack all split by row, each row writing into
its own slice of the output, so the result is bit-identical whatever the thread
count - which invariant 4 requires and which a parallel reduction over floats
would not have given us.

Measured with `cargo test --release -p aura-raw --test scaling_probe -- --ignored`,
best of three runs on an 8-core Windows host, `RAYON_NUM_THREADS` forcing each
column:

| Sensor | Tier 2 serial | Tier 2 parallel | Tier 3 serial | Tier 3 parallel |
|---|---|---|---|---|
| 0.10 MP | 3 ms | 3 ms | 8 ms | 5 ms |
| 1.57 MP | 40 ms | 43 ms | 152 ms | 78 ms |
| 6.29 MP | 236 ms | 164 ms | 964 ms | 455 ms |
| 25.17 MP | 460 ms | 323 ms | 3,157 ms | 1,519 ms |

Below about 1 MP the scheduling costs more than it saves, which is why both
parallel paths fall back to a serial loop under a fixed sample count. At 25 MP
tier 2 is 1.4x faster and tier 3 is 2.1x faster.

**The waiver does not close.** Extrapolating the 25 MP figures, a 45 MP frame
needs roughly 580 ms for tier 2 against a 250 ms budget and roughly 2.7 s for
tier 3 against 1.8 s. Parallelism bought a factor; it did not buy the factor
required.

**Renewed for phase 02**, on the same grounds as before plus one new one: the
pipeline is correct, bounded and proven; tier 2 is a background batch behind a
cache, so the number that decides whether the product feels fast is the cached
read (8 ms budget, measured in single-digit milliseconds); and the remaining
cost is now dominated by memory traffic rather than by arithmetic, which is a
different fix from the one this change made.

Two routes remain, and they are not equivalent:

1. **SIMD on the per-sample loops** (bit unpacking, the tone curve, the sRGB
   quantiser). Output-preserving, so it can land at any time.
2. **Fusing the bin and the resize** so a 25 MP sensor never materialises a
   12.5 MP intermediate. This is the larger win and it is the one that would
   close the gap - but it changes proxy pixel values, so it needs a
   `PIPELINE_VER` bump, ML-lead sign-off and a model re-validation. That is a
   phase decision, not a performance decision.

The waiver expires when the render graph lands. Closing it means taking route 2
with sign-off, or route 1 plus a GPU resize, re-running the probe above, and
updating this table.
