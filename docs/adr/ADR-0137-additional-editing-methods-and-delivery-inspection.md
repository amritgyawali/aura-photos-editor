# Additional editing methods and delivery inspection

Date: 2026-10-22. Status: implemented; validation belongs to the dated report.

AURA adds 25 colour methods, 25 spatial repair methods, 25 selection workflows,
and 25 read-only inspection methods. The capability registry identifies every
addition separately from the earlier two hundred. Existing ordered stacks keep
their 32-operation limit. Old recipes have no new operations by default.

Colour and spatial operations reuse the existing revision-protected save
commands and atomic recipe history. New enum values serialize through their
existing extension keys. Selections reuse the cancellable matte-authoring path.
The renderer identity advances from `labs.selected-v3` to `labs.selected-v4` so
previous cached output cannot qualify this implementation.

Inspection runs on the saved render at a maximum 512-pixel edge, validates the
revision before and after computation, and never writes recipe history. Three
additional review screens group distribution statistics, surface structure, and
lighting geometry. Statistics are descriptive, with units and support counts;
they are not learned defect classifiers or a photographic perfection score.

New structure measurements use asinh luminance for stable signed/HDR derivative
calculations. The DCT uses complete 8-by-8 blocks with orthonormal scaling and
excludes DC. Broad light trends use orthogonal centred coordinate bases.
Working-range excursion energy measures the linear working cube, not sRGB gamut.
All histogram and percentile domains are identified in the interface.

The installed Evoto worker has the same SHA-256 as the previous bounded native
research. REA PE-resource evidence and a refreshed complete file inventory are
retained. This run's connected REA session has no configured deep-analysis
provider. Earlier static findings remain static evidence; the current inventory
does not establish recovered model architecture, weights, training data, source,
or numerical processing parity.

No Evoto binaries, models, shaders, or UI assets are included in AURA. The added
operators are independent mathematical implementations. The optional matching
variant uses an explicitly labelled previously observed watermarked screenshot
as an image reference, and preserves the original qualified recipe through undo.
Its distance metrics measure AURA's reference-fitting objective, not Evoto output
fidelity. Full native Evoto exports remain unavailable in the existing evidence.

Spatial repairs retain their existing 256-pixel correction grid. Some colour
methods allocate whole-frame working planes and can consume substantial memory
at native resolution. Numerical fixtures, actual desktop IPC, real-photo proxies,
and complete RAW exports provide different evidence; the final report must not
substitute one for another or imply exhaustive parameter testing.
