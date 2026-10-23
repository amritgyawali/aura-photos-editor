# AURA sampled colour tools — 19 October 2026

Adds a colour-range selection tool available to every native retouch operation,
plus local saturation and vibrance. These build on the previously delivered
[optical and colour tools](AURA-optical-and-colour-tools-2026-10-19.md).

## Use

1. Open **Auto edit → choose a photo → Retouch**.
2. Choose **Local saturation & vibrance**, or another retouch tool.
3. Set the area using an ellipse, brush, gradient or saved detected-region mask.
   **Select entire photo** removes geometric/matte restrictions and retains
   brightness and colour limits.
4. Enable **Limit by sampled color**, then **Pick selection color on photo**.
   Click the colour to select. Numeric sample coordinates are also available.
   A yellow square marks the sample position when selection guides are shown.
5. Adjust **Color tolerance**, **Color range falloff** and **Color lightness
   sensitivity**. Use **Preview selection mask** to inspect actual coverage.
6. Adjust saturation/vibrance and strength; apply, update or undo normally.

| Control | Meaning |
|---|---|
| Colour tolerance, 0–100 | Full coverage inside this D65 Lab distance from the sampled pixel |
| Colour range falloff, 0–100 | Smooth transition from full coverage to zero outside the tolerance |
| Lightness sensitivity, 0–100% | Weight of the Lab lightness difference; colour axes are always compared |
| Local saturation, −100–100% | Chroma reduction or increase around the pixel's linear luminance |
| Local vibrance, −100–100% | Adaptive chroma change, with less effect on already saturated colours |

The sample is read from the image immediately before the operation, so earlier
edits can change which colours match. It is independent of clone/heal sources and
the zoom-blur centre. The initial sample is the image centre until moved.
Colour and brightness limits multiply the geometric selection after shape
inversion. An attached segmentation mask further restricts the result.

## Implementation and limits

- Recipe validation bounds all coordinates and range controls. Old recipes omit
  the new optional selection field and keep their original behaviour.
- Native mask preview, saved coverage, rendering and delivery use the same
  colour-range calculation. Full-quality inspection is recommended: a proxy can
  sample a different mixture of pixels at a fine boundary.
- The range uses Euclidean D65 Lab distance as a mask metric, using the existing
  working-space conversion. It is not a CIEDE2000 quality measurement.
- Local saturation/vibrance operate in linear Rec.2020. A uniform chroma limit
  avoids creating negative channels; luminance and neutral pixels are preserved.
  This does not guarantee that boosted colours fit every output device gamut.
- A colour range is not object recognition. Similar colours on skin, clothing
  and scenery can all match. Use a brush or detected region to separate them.
- The sample is a single pixel. Avoid edges, noise and specular highlights when
  choosing a representative material colour; review the mask before applying.
- Presets retain saturation/vibrance parameters but omit image-specific colour
  samples and selections. Current-photo selections remain when loading settings.
- No external model, dependency, network operation or catalogue migration.
  Originals remain read-only. Automatic portrait planning does not add this tool.
- Older binaries reject the new field and tool. Remove affected steps with the
  current binary before downgrading. Full-resolution performance on all reference
  hardware remains unqualified.

Design: [ADR-0129](../adr/ADR-0129-sampled-colour-range-and-local-saturation.md).

## Validation

| Check | Result |
|---|---|
| Full UI suite | 824 passed in 96 files with two test workers |
| Full recipe/render suites | 483 passed, 12 ignored |
| New colour-range/saturation signal tests | Eight passed, included in the full native count |
| Final signal rerun | Eight passed after the one-ULP identity guard refinement |
| Native integration | Three workflows passed: new sampled colour tool plus the earlier optical and material tools; import, saved recipes, changed pixels, Undo/Redo, verified JPEG export and source-byte preservation |
| Frontend production build | Passed; existing bundle-size warning remains |
| Frozen contracts | All 83 entries locked; no relock |
| Strict recipe/renderer lint | No new findings; blocked by existing skin-connectivity and under-eye-renderer findings |

The first default-concurrency UI run had one five-second timeout in the new
authoring test while Rust builds were active; 823 other tests passed. The complete
suite passed with `--maxWorkers=2 --minWorkers=1`, without increasing timeouts or
weakening assertions. The focused editor run also passed before that rerun.

Signal coverage includes sampled hue discrimination, lightness weighting, smooth
falloff, shape inversion order, brush erasure, gradients, brightness intersection,
saved-matte holes, independent sample/source coordinates, upstream pixel changes,
legacy recipes, invalid parameters, luminance, neutrals, gamut limits, adaptive
vibrance, determinism and exact disabled/zero identity. The full native suite ran
before the final one-ULP identity guard refinement; all eight affected signal tests
passed again after that change.

Strict `--lib --no-deps` Clippy remains blocked by
`retouch_skin/connectivity.rs:37` (`needless_pass_by_value`) and
`retouch_undereye.rs:152` (`too_many_lines`). Those existing findings were not
suppressed or counted as passing checks.

The rebuilt Windows desktop passed five checks: on-photo sampling and validation,
saved colour coverage with excluded pixels unchanged, saved-mask review and update,
exact Undo/Redo restoration, and preservation of source file bytes.
Logs use
`.work-checks/aura-colour-range-*`. Ignored native tests require opt-in fixtures,
performance runs or hardware and are not counted as passes.

This addition does not establish complete Evoto parity or superior photographic
quality. The feature ledger records these as additional AURA capabilities with
their own implementation evidence.
