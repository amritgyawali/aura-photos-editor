# ADR-0132: Twenty practical repairs and creative looks

Status: accepted. Date: 2026-10-20.

Twenty more tools in the shape ADR-0131 set: each a `Tool` variant, an ordinary saved retouch
operation with its own selection, strength and Undo, rendered by
`crates/aura-render/src/retouch_effects/` (`practical.rs` and `creative.rs`), never planned by an
automatic pass, and held to the same four rules - it changes only its selection, nothing at zero
strength, the same pixels on every machine, and reads the frame as it was before it ran.

| Practical (`practical.rs`) | Creative (`creative.rs`) |
|---|---|
| Dust & scratches - median replacement past a threshold | Tilt-shift - blur away from a sharp band, plus colour |
| JPEG artifact cleanup - 8 x 8 boundary smoothing below a step limit | Duotone - luminance between a shadow and a chosen colour |
| Remove banding - smoothing where flat, with a fixed half-level dither | Posterize, halftone print, pencil sketch |
| Auto levels - the selection's own 0.5% and 99.5% to full range | Oil paint - Kuwahara over summed-area tables |
| Remove colour cast - the selection's own grey world | Cross-process, infrared |
| Even out lighting - divide by the selection's own light field | Mirror symmetry |
| HDR detail - local contrast in log luminance | Lens flare, god rays |
| Bokeh highlights - highlights scattered as rimmed discs | Snow and rain |

## Decisions

- **The practical tools measure the selection, never a constant.** Auto levels reads the
  selection's own percentiles, the colour-cast removal its own average, the lighting flattener its
  own light field at a third of its size, the dust remover the median of each speck's own
  neighbourhood. A JPEG step is an artefact only below a step limit relative to its level, so a
  real edge on a block boundary stays - a test puts one there.
- **Every highlight is found.** Bokeh first sampled highlights on a grid of half its disc radius,
  and a single bright point between two samples was missed. Each grid cell now contributes its
  brightest pixel, carrying the light of the whole cell.
- **A reversed smoothstep falls.** The shared `smoothstep` guarded a zero span with `.max(1e-9)`,
  which turns a negative span positive: every call written with its edges reversed - to fade out
  rather than in - returned 0 or 1. It broke bokeh discs, inverted the halftone (ink everywhere
  but the dots), the lens-flare ghosts and the weather. The tests for halftone and weather passed
  anyway because they were too loose to tell a dot from its negative; they now assert that a dark
  tone gets more ink than a light one and that snow is flakes rather than a white sheet. None of
  ADR-0131's twenty-nine tools used reversed edges. The guard keeps the span's sign, and a test
  asserts both directions.
- **Randomness is a hash of position and placement.** Snow, rain, banding dither: the same recipe
  is the same picture on every machine and every core count (invariant 4).

## What this does not do

None of these tools finds its area on its own or generates picture content. The weather, flare,
rays and bokeh are drawn from the photograph's own light and a deterministic pattern; the oil
paint and sketch are filters of the photograph. Visual quality against any other product has not
been measured.
