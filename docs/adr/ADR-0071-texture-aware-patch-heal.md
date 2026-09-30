# ADR-0071 — Texture-aware patch healing

- Date: 2026-09-30
- Status: accepted

Add opt-in `patch_heal` to the native retouch stack. Existing Heal and Auto blemish
operations retain their rendering behavior. This is AURA's independent repair
algorithm, with no competitor models, licensing dependency or network service.

Small ellipses can choose an automatic donor. Search examines 64 nearby offsets,
excluding overlapping or out-of-frame source bounds. A ring outside the target
compares donor texture after subtracting the mean color difference. Additional
penalties discourage color mismatches and isolated donor-center defects. The
damaged target center is excluded from matching. Fixed traversal is deterministic;
when no donor fits, pixels stay unchanged. This is bounded patch matching, not
semantic blemish detection or permanent-mark protection.

Manual source coordinates support fractional offsets through bilinear sampling.
Painted selections and ellipses with either radius above 10% require a source in
both UI and native validation. A single offset applies to the whole operation;
separate repairs should use separate operations when different donors are needed.

Tone correction interpolates the target-minus-donor boundary difference on a
grid of at most 64 by 64 nodes using 96 Jacobi relaxation passes. The grid uses
physical pixel spacing, with outside-mask nodes fixed. Its interpolated correction
is added to full-resolution donor texture and blended with strength and feather.
This coarse harmonic correction is related to the public formulation in
[Pérez, Gangnet and Blake, Poisson Image Editing](https://www.cs.jhu.edu/~misha/Fall07/Papers/Perez03.pdf).
It is an approximation, not a converged full-resolution Poisson solver. Narrow
mask details, large repairs and sharp illumination changes can expose its limits.
No claim is made about the internal algorithms of commercial healing tools.

Processing stays in linear Rec.2020. All source reads complete before repaired
pixels are committed, making overlapping manual donors independent of scan order.
Unavailable source pixels and pixels outside the authored mask stay unchanged.
Negative candidate channels are clamped before blending. Memory grows with the
selected bounding rectangle, in addition to the existing full-frame render memory.

The tool participates in existing previews, editable recipes, undo/redo and export.
Tests cover isolated-spot removal, tone slopes, fine-texture transfer, overlapping
donors, disabled/neutral settings, borders, mask isolation and source validation.
Five real portraits exercise the desktop workflow. These establish implementation
behavior, not equivalence to Retouch4me or a ranking against other products.
