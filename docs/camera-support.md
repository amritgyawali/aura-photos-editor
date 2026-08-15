# Camera support matrix

What AURA can do with a file, by container and by compression. This page is
deliberately blunt: a photographer deciding whether to trust us with a wedding
deserves to know exactly where the edges are.

The reasoning behind the gaps is in
[ADR-0004](adr/ADR-0004-raw-decode-backend.md). The short version: this build
decodes in pure, safe Rust rather than linking LibRaw, which buys memory safety
and a clean licence and costs coverage of proprietary compressions.

## What the three tiers mean

| Tier | What you get | Where it is used |
|---|---|---|
| 1 | The JPEG the camera stored inside the file, oriented and scaled | The grid, triage, culling |
| 2 | A 2048 px proxy rendered by AURA through a documented colour path | Every AI model, colour decisions |
| 3 | Full sensor resolution, tiled | Final render only |

A file that reaches **tier 2 from its mosaic** is fully supported. A file that
falls back to **tier 2 from its embedded preview** still browses, culls,
exports and is analysed - but the pixels carry the camera's own rendering rather
than AURA's, which is recorded in the sidecar (`source: embedded`), flagged with
`AURA-RAW-2007`, and shown as a badge in the Explain panel.

## The matrix

| Container | Tier 1 | Tier 2 from mosaic | Tier 3 | Notes |
|---|---|---|---|---|
| DNG, uncompressed or packed CFA | yes | **yes** | **yes** | The best case. Carries its own colour matrices. |
| DNG, lossless JPEG (SOF3) | yes | **yes** | **yes** | The common Adobe-converted case. |
| DNG, 6x6 X-Trans CFA | yes | **yes** | **yes** | A Fujifilm file converted by Adobe. Proxy is a third of the sensor, not a half. |
| DNG, lossy JPEG (34892) | yes | no | no | Preview-only. |
| Canon CR2 | yes | **yes** | **yes** | Lossless JPEG mosaic. CR2 slice reassembly is untested against a real body - see the caveat below. |
| Canon CR3 | yes | no | no | CRX compression is not implemented. `PRVW`/`THMB` previews and `CMT1` metadata are read. |
| Nikon NEF, uncompressed | yes | **yes** | **yes** | |
| Nikon NEF, compressed | yes | **yes** | **yes** | Huffman coding plus the body's linearisation curve, read from MakerNote `0x0096`/`0x008C`. A file whose decode table we cannot read is refused rather than guessed. |
| Sony ARW, uncompressed | yes | **yes** | **yes** | |
| Sony ARW2 (compressed) | yes | **yes** | **yes** | Sixteen photosites per sixteen bytes. Lossy in the file, not in us. Linearisation is approximate unless the file exposes tag `0x7010` - see below. |
| Olympus/OM ORF, packed | yes | **yes** | **yes** | |
| Olympus/OM ORF, compressed | yes | **yes** | **yes** | The adaptive predictive scheme. |
| Panasonic RW2 | yes | no | no | Proprietary compression, not implemented. |
| Pentax PEF | yes | packed only | packed only | |
| Samsung SRW | yes | packed only | packed only | |
| Fujifilm RAF, uncompressed | yes | **yes** | **yes** | Sensor size and the 6x6 layout come from the RAF block directory. |
| Fujifilm RAF, compressed | yes | no | no | Fujifilm's own entropy coder is not implemented. |
| TIFF | yes | if CFA and supported compression | same | |
| JPEG | yes | from the JPEG itself | no | A camera JPEG has no mosaic to render. |
| HEIF / HEIC | no | no | no | Refused with `AURA-RAW-2001`. |

**Caveat on the untested rows.** Every "yes" above is exercised two ways: a
generated file in that exact encoding, written by an encoder that walks the
format's state machine forwards and read back by the decoder that walks it
backwards; and the eight synthetic bench bodies for the colour path. What that
does *not* prove is that the published format is the format the camera writes.
Rows naming a real manufacturer describe what the decoder does with that
manufacturer's documented layout; they have not been verified against a file from
that body, because this repository contains no camera files. Verify before
promising a photographer their body is supported - one frame is enough.

**A note on Sony's linearisation.** ARW2 stores eleven-bit samples that are not
linear; the curve that expands them lives in an encrypted sub-directory. When the
file exposes tag `0x7010` we use the body's own curve. When it does not we use a
plain linear expansion, which is a documented approximation rather than an
invented curve - tone response in the mid-greys will be slightly off, hue and
saturation will not.

## Colour profiles

Separate from decoding, and separate again from being *correct*:

| Profile source | When it applies | Consequence |
|---|---|---|
| Embedded in the file | DNG with `ColorMatrix1`/`ColorMatrix2` | Best available |
| Bundled table | The eight synthetic bench bodies | Exact by construction |
| Generic matrix | Everything else | `AURA-RAW-2006`, `profile=generic` badge, colour may drift |

No matrices are invented for real cameras. Adding one needs a ColorChecker frame
from that body and a sign-off from the Colour Scientist role
([ADR-0003](adr/ADR-0003-colour-pipeline.md)), after which `pipeline_ver` is
bumped so cached proxies rebuild.

## Adding support for a format

1. Get one sample file. A frame of a wall is fine; never use a client's imagery.
2. Add it to the fuzz corpus first, and confirm it fails loudly rather than
   crashing.
3. Add a module under `crates/aura-raw/src/codecs/`, and a variant to
   `MosaicScheme` in `meta.rs` so the container walk can name it. Bit reading
   goes through `codecs::bits::BitReader`; nothing in a codec indexes a slice
   directly.
4. Write the **encoder** next to the decoder and wire it into
   `MosaicEncoding` in `crates/aura-raw/src/fixtures.rs`. This is not optional:
   it is what makes the round-trip test in `tests/codecs.rs` possible, and a
   round trip is the only proof available without camera files.
5. Add the encoding to the cycles in `aura-cli`'s `write_raw_fixtures` and
   `verify_colour`, so the phase gate exercises it.
6. Update this page and the ADR consequences table.

## Which bodies are calibrated for technical judgement

Separate from decoding, and worth not confusing with it. A body can decode perfectly and
still have no **calibration** row - the measurements that decide what counts as sharp,
how much noise to expect at an ISO, and how much clipped highlight can be brought back.

Those live in `crates/aura-brain-photo/config/camera_calibration.toml`, twenty bodies are
named there, and a body that is not gets a deliberately cautious fallback plus a lowered
confidence on every verdict. `AURA-ML-5037` is the code, and its runbook explains exactly
what the fallback costs and how to add a row.

**The twenty shipped rows are derived from published sensor specifications rather than
measured from bodies**, because there are still no camera files in this repository -
condition C2 in `progress/PHASE-09-EXIT.md`, blocked by the same missing input as phase
02's ColorChecker.

## Related

- Decode backend: [ADR-0004](adr/ADR-0004-raw-decode-backend.md)
- Frame integrity and camera calibration: [ADR-0019](adr/ADR-0019-frame-integrity-and-eye-intent.md)
- What the technical marks mean: [frame integrity](frame-integrity.md)
- Colour pipeline: [ADR-0003](adr/ADR-0003-colour-pipeline.md)
- Troubleshooting: [previews runbook](runbooks/previews.md)
