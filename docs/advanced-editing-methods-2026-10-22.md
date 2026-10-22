# Advanced editing methods and existing-tool repairs

The colour, spatial and selection laboratories each provide 100 methods. These
include 25 further independently implemented mechanisms per laboratory, alongside
25 additional descriptive diagnostics. The [capability ledger](reports/AURA-advanced-capabilities-2026-10-22.json)
describes each addition; the [validation report](reports/AURA-advanced-validation-2026-10-22.md)
records actual test coverage and remaining uncertainty.

Create or select a local mask, choose the method, adjust its displayed controls,
and apply. Start with low strength on a suitable selected area. Creative palette,
monochrome, dotscreen and deformation methods deliberately change the appearance;
they are not universal portrait fixes. Saved operations participate in native
revision checks and Undo/Redo.

Three further review screens provide **Texture fidelity**, **Colour stability**
and **Delivery integrity**. Run inspection explicitly on the saved edit. There are
95 measurements across all screens; switching groups retains the same report and
saving another revision invalidates it. Measurements describe a maximum 512-pixel
preview. They do not identify a person, infer skin defects, calibrate sensor noise
or assign a perfection score.

Existing tools receive specific repairs:

- Local colour stencils evaluate targets during blending, removing the retained
  RGB target plane. The saved allocation is 12 bytes per image pixel for the
  affected path, approximately 343 MiB on a 30-megapixel image. Existing source and
  statistics buffers can still require substantial memory. Fixed-capacity local
  scratch replaces repeated per-pixel allocation; luminance-only Wiener cleanup
  avoids unused chroma planes.
- Forty-four supported spatial methods use native-resolution local processing
  with selected-only immutable neighbours, tile halos and bounded stripes. Fine
  selected details that vanished in the former reduced grid can now be repaired.
  Local support is at most four pixels. The source stripe is bounded to 2,097,152
  RGB cells (24 MiB); source-row caches and tile state add scratch. The remaining
  56 global/geometric methods retain a maximum 256-edge grid. Unusually wide
  selections exceeding the native stripe budget also fall back to that grid.
- Selection controls validate only the active method's displayed parameters.
  Switching from a large patch radius to a tool without a radius control no
  longer disables a valid operation. Resource preflight rejects excessive work
  before loading a source preview.
- Existing horizontal autocorrelation centres each side of each lag on its own
  valid paired support. A cropped affine ramp now has correlation one; constant
  or unsupported pairs report zero.

Renderer cache identity advanced to `labs.selected-v5` to invalidate older output.
Known-answer, selection-isolation, finite/HDR and resource fixtures cover these
changes. Consult the report for final native, desktop and photo results.

These are original AURA implementations. Evoto's trained face/body models,
training data and complete numerical methods have not been recovered. Exact
Evoto output equivalence and subjective superiority have not been established.
