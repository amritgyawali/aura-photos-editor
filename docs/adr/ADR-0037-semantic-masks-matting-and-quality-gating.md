# ADR-0037 - Semantic masks: a measured spatial vocabulary, matted at the edge and gated by its own quality

- **Status:** accepted
- **Date:** 2026-08-20
- **Phase:** 18 - Local Mask AI: Automatic Semantic Masking
- **Supersedes:** nothing. Extends ADR-0013 (people intelligence), ADR-0019 (frame
  integrity), ADR-0029 (render pipeline) and ADR-0031 (exposure, white balance and skin).
- **Deciders:** CTO, ML Lead - Vision, Colour Scientist, Senior Engineer - Core Pipeline,
  Senior Engineer - GPU & Render, Product Manager

## Context

Every phase from 19 to 24 edits *part* of a photograph. Phase 19 sculpts light on a face,
phase 20 smooths skin without smoothing an eyelash, phase 21 removes a blemish and not a
mole, phase 22 recovers detail in hair, phase 23 crops around a subject and phase 24 fills
a gap in a background. None of them can say the word "there" without this phase.

A mask is therefore not a feature. It is the *spatial vocabulary* six later phases speak,
and the cost of getting it wrong is not a bad mask - it is six phases of edits applied to
the wrong pixels, each of which looks like its own bug.

Three properties make this phase different from the fourteen before it:

1. **A mask is visible at 100 % zoom.** Every earlier phase produced a number. A halo around
   a veil is not a number a photographer disputes; it is a delivered photograph that looks
   cheap.
2. **A mask has no single right answer at the boundary.** Where hair ends is a matter of
   degree, which is what matting is and why a hard segmentation is not enough.
3. **A wrong mask is silent.** A wrong exposure looks wrong. A face mask that includes the
   background behind the ear looks fine until phase 20 brightens it.

Eleven decisions follow. Six of them are about what this phase refuses to do, and the two
that matter most - decisions 2 and 8 - exist because of property 3.

## Decision 1 - The frozen contract lives in `aura-vision`, not in `aura-core`

**Chosen:** `crates/aura-vision/src/contract/mask.rs` holds `MaskKind`, `MaskPayload`,
`Mask`, `MaskOp`, `GpuMask`, `MaskOutline` and `MaskService`, and it is digested in
`contracts.lock` exactly as the eleven contracts in `aura-core` are.

**Rejected:** `crates/aura-core/src/contract/mask.rs`, which is where every judgement
contract since phase 15 has gone.

`aura-core` depends on no other workspace crate and a test asserts it. Section 5 of the phase
document freezes `fn upload_gpu(&self, mask: &Mask, level: RenderLevel) -> GpuMask`, and
`RenderLevel` is `aura-render`'s. Putting the contract in `aura-core` would mean either
`aura-core` gaining a render dependency - which breaks the one structural rule that has held
since phase 01 - or the signature losing the render level, which would make "which resolution
is this mask for" a convention rather than a type.

There is precedent and it is the right precedent: `SimilarityIndex` is in `aura-index`,
`RenderService` is in `aura-render`, `PreviewService` is in `aura-preview`. The rule was never
"contracts live in `aura-core`". It is "a contract lives in the crate that owns the *kind of
thing* it describes, and it is frozen there". A mask is a spatial artefact tied to pixels and
to the renderer, not a judgement about a wedding.

The cost is that a phase 25 or 27 consumer depends on `aura-vision` to name a mask.
`aura-brain-photo` already does, and `aura-vision` remains free of `aura-brain-*`, so the
dependency runs in the direction it already ran.

## Decision 2 - The masks that ship are **measured**, and both heads are placeholders that are never consulted

**Chosen:** every mask in this build is produced by deterministic geometry and colour
arithmetic over the pixels, seeded by phase 06's face and body boxes and phase 06's eye
landmarks. `segment::SEG_HEAD_TRAINED` and `matting::MATTING_HEAD_TRAINED` are both `false`,
so `Analyser::class_hint` and `Analyser::alpha_hint` return `None` and no photograph in this
build is segmented by a random projection.

**Rejected:** blending the placeholder segmentation head's output into the class decision at
a low weight so that "the model is wired up".

This is the fourth phase to make this call and the argument has not changed since phase 16
made it about the tone head: a random projection blended at any weight is a random
contribution at that weight, and in the panel it is indistinguishable from a learned one.
Here it is worse than in phase 16, because of property 3 above. A wrong tone parameter is
visible in the histogram. A wrong class label on the pixels behind somebody's ear is visible
only after phase 20 has smoothed them.

What the deterministic path can honestly do is quite a lot, and it is what section 6.1
actually describes:

- **Skin** is seeded by detected faces and grown in a colour space, constrained to connected
  regions. Section 6.1's own sentence, and it handles arms, shoulders and hands.
- **Face, eyes, sclera, iris, lips, eyebrows, teeth** are geometry: phase 06 already amended
  `FaceRef` in phase 09 to carry the bounding box and both eye landmarks, and the rest of the
  face is a measured proportion of that box refined by local luminance and chroma.
- **Hair and facial hair** are a bounded search above and around the face box for pixels
  that are neither skin nor background by the same connected growth.
- **Subject** is the union of the person boxes, alpha-refined.
- **Sky, greenery, water, floor and window** are colour, position and texture priors -
  measurable properties of a photograph, not predictions about it.
- **The skin-safe zone** is the union of skin and face, dilated, which is exactly what phase
  16's guard needs and nothing more.

What it *cannot* do is name a class that has no geometric or colorimetric signature -
"clothing" versus "dress" is the clearest case. `MaskKind::Dress` is produced only when a
person's non-skin, non-hair region below the face reaches the bottom of the frame and is
predominantly low-chroma and high-luminance, and it carries a lower confidence when it does.
The head that would decide it properly is the one that is not trained.

The gates in section 10.1 are measured against synthetic frames whose regions are **painted
into the pixels** and read back through the real pipeline - phase 10's pattern, for the fifth
time. That proves the arithmetic and says nothing about a wedding photograph. It is condition
C1 of the exit report and a Sev 2 trigger.

## Decision 3 - Matting is a guided-filter alpha estimate in the uncertain band, not a network

**Chosen:** `trimap` erodes and dilates the coarse mask into three regions, and `matting`
solves a local linear alpha model - the guided filter's own closed form - inside the unknown
band only, using the frame's own colours as the guide.

**Rejected:** a matting network in the band, which is what section 6.1 asks for.

Two reasons, in the order they mattered. The interpreter in `aura-infer` implements a
documented opset 13 subset with no `Resize` and no `ConvTranspose` (ADR-0007), so a matting
decoder cannot be executed in this build at all. And the guided filter is not a placeholder
standing in for the real thing: it *is* a real matting algorithm with a closed form, it is
what most matting networks are refined by anyway, and its failure mode is a slightly soft
edge rather than a confidently wrong one.

The matting head is still registered, signed and carded, and `MATTING_HEAD_TRAINED` is
`false`. When it is trained it replaces the estimate inside the band and the trimap, the
band width, the storage and the compositing are all unchanged - which is the point of
separating them.

## Decision 4 - The unknown band is a fraction of the subject's size, not a fixed pixel count

**Chosen:** the trimap's erode and dilate radius is `BAND_FRACTION` of the square root of the
mask's own area, clamped to `[BAND_MIN_PX, BAND_MAX_PX]`.

**Rejected:** a fixed radius in pixels, tuned at 2048 px and scaled with the render level.

A fixed radius is right for one subject size and wrong for two. The band has to cover the
hair on a full-length portrait of a bride who occupies a fifth of the frame *and* the hair on
a head-and-shoulders that occupies half of it; those differ by a factor of five, and a radius
that covers the first leaves the second's flyaways outside the band, where alpha is exactly
one or exactly zero and no amount of refinement can help. Phase 14 made the same call about
the tiling halo and for the same reason: a constant that is correct at one scale is a seam
at another.

## Decision 5 - Binary classes are stored as run-length, alpha classes as 8-bit at quarter resolution, and the budget is a test

**Chosen:** `MaskPayload::Rle` for classes whose boundary is not perceptually load-bearing
(sky, greenery, floor, clothing, the skin-safe zone), `MaskPayload::Alpha8 { w, h, data }` at
one quarter of the analysis resolution for the ones whose boundary is (subject, hair, face,
skin). The RLE is over a row-major bitmap with a varint run length, and
`PAYLOAD_BUDGET_BYTES` is 180 KB for every class of one photograph, asserted by the phase
gate on the synthetic wedding.

**Rejected:** storing every class as alpha, and storing every class as RLE.

All-alpha is 20 classes times a quarter-resolution plane, which is 1.3 MB per photograph
before compression and about 1.3 GB for a thousand-image gallery - the failure mode section
12 names. All-RLE loses the boundary, which is the entire value of the four classes that have
one.

The split is by *kind*, declared once in `MaskKind::stored_as`, so it cannot drift per call
site. A caller that needs alpha from an RLE class gets a hard edge and `EdgeQuality::Binary`
in the report rather than a silent lie about how soft the boundary is.

## Decision 6 - Quality is two numbers, and the low one *reduces strength* rather than refusing

**Chosen:** every mask carries `confidence` (how sure the class assignment is) and
`edge_quality` (how well-determined the boundary is). `quality::allowance` maps the pair onto
a `[0, 1]` strength ceiling that phases 19 to 24 multiply their own strength by, and below
`AGGRESSIVE_FLOOR` the aggressive operations - skin smoothing, generative cleanup - are
disabled entirely and the reason is recorded.

**Rejected:** a single quality number, and a hard refusal below a threshold.

Two numbers because they fail independently and are fixed by different things. A face mask
over a crowd can be confidently the right class with a badly determined boundary (motion
blur, backlight); a mask over a dark suit against a dark background can have a crisp boundary
and no confidence about which side of it is clothing. Collapsing them loses which of the two
a photographer is looking at, and the panel says which.

Not a hard refusal because the alternative is worse in the common case. A veil at
`edge_quality = 0.55` is a mask that can carry a third of a stop of local exposure perfectly
well and cannot carry skin smoothing. Refusing at a threshold turns a graded response into a
cliff, and a cliff is what makes half a gallery silently unedited.

## Decision 7 - A user-edited mask is never regenerated, and the flag is checked inside the statement that would overwrite it

**Chosen:** `masks.user_edited = 1` is inside the `WHERE` of the `DELETE` that a regeneration
pass starts with, exactly as `moments.user_locked` is in phase 08 and `identities.user_locked`
is in phase 06. A model-version bump regenerates every mask *except* the edited ones, and the
edited ones keep their old `model_ver` and say so.

**Rejected:** regenerating everything and re-applying the photographer's brush strokes on top.

Re-applying a brush stroke requires the stroke to still mean the same thing under a mask that
moved, and it does not: a stroke that removed the background from between two strands of hair
is a stroke about *those* pixels. Phase 06 settled this for identities, phase 08 for moments,
phase 09 for dismissals. Third time, same answer.

## Decision 8 - Masks are generated lazily for selected frames, and the coverage denominator is *selected* frames

**Chosen:** `MaskOutline::coverage` is measured against the frames phase 12 kept, not against
every photograph in the project. `MaskService::ensure` is the only way a mask comes into
existence and it is called on demand.

**Rejected:** a project-wide pass after phase 12, and a coverage denominator of every photo.

Section 6.3's own argument for the first half: rejected frames never need masks, and not
computing four thousand of them is a large part of why the phase meets its time budget.

The denominator is the half worth arguing about. Every phase since 09 has reported coverage
against *every* photograph, deliberately, because a verdict needs only pixels. This one is
different and the difference is real: a mask over a rejected frame is not a gap, it is a
frame nobody asked about. Reporting 12 % coverage on a wedding where every keeper is masked
would send a photographer looking for a bug that is a design decision. The outline carries
`selected` and `masked` separately so the denominator is visible rather than implied - phase
08 made the same call for the same reason, and phase 08's rule is the one being followed:
"say what the denominator is".

## Decision 9 - Instance scoping is an overlap test against phase 06's boxes, and an unassigned component is `None` rather than a guess

**Chosen:** each connected component of a person-bearing class is assigned to the phase 06
face or body box it overlaps most, and only when the overlap exceeds `ASSIGN_MIN_IOU`.
Everything else is `identity: None`.

**Rejected:** assigning every component to its nearest box.

Nearest-box assignment is what makes the bride's skin mask include the guest standing behind
her at a wedding, which is section 10.1's own test and the failure that makes phase 25's
per-person gallery consistency worse than no per-person consistency at all. An unscoped skin
component is still a skin component; it is just not *hers*, and the operations that need it
to be hers can see that it is not.

## Decision 10 - Compositing happens in linear light, and the shaders are held to it by the same test that holds the render path

**Chosen:** `mask_composite.wgsl` and the reference path both blend
`out = a * edited + (1 - a) * base` on linear Rec.2020 values, and
`crates/aura-render/tests/colour_discipline.rs` - the grep-as-a-test phase 14 shipped -
covers the two new shaders unchanged.

**Rejected:** compositing on the encoded values, which is what every fast path wants to do
because the buffer is already there.

A 50 % mask blended after the transfer function is a 73 % blend in light, and the error is
largest exactly at the boundary where the mask is soft - which is to say, in the halo. This
is the COL sign-off in section 9 and it is one line in one test rather than a review item.

## Decision 11 - `aura-vision` gains a catalog dependency, and phase 06's structural defence becomes a grep-as-a-test

**Chosen:** `mask::store` lives in `aura-vision`, as section 4 names it, so `aura-vision`
gains `aura-catalog` and `rusqlite`. `crates/aura-vision/tests/no_template_writes.rs` asserts
by grep that nothing in this crate writes to `faces`, `face_templates` or any other table
`aura-people` owns.

**Rejected:** putting the mask store in `aura-people` or `aura-brain-photo` to keep
`aura-vision` free of the catalog.

Phase 06 wrote that a face template cannot be written from `aura-vision` because the crate
"has no catalog dependency, so it *cannot* write one; the separation is structural rather
than a rule people remember". That sentence stops being true here, and pretending otherwise
would be worse than replacing it. What replaces it is the mechanism this repository already
trusts twice - `crates/aura-render/tests/colour_discipline.rs` and
`crates/aura-brain-photo/tests/no_recipe_writes.rs` are both greps as tests, and both catch
the second module to break a rule rather than the first person to forget it.

The alternative kept the letter of phase 06's sentence and split one module across two
crates, which is how the compression ends up disagreeing with the codec.

## Consequences

- Six later phases have one spatial vocabulary and no phase may keep its own segmenter.
- Nothing in this phase moves a pixel. `MaskService` produces masks; phases 19 to 24 consume
  them. The render graph's `SkipReason::MaskGeneratorAbsent` stays reachable, because a
  renderer that was not handed mask planes still cannot evaluate a semantic mask - wiring the
  planes into the graph is phase 19's first task and it changes no shape frozen here.
- Both shipped heads are placeholders, `SEG_HEAD_TRAINED` and `MATTING_HEAD_TRAINED` are
  `false`, and every number in section 10.1 is measured on painted synthetic frames. C1.
- Storage is bounded by construction and the bound is asserted, so the failure mode in
  section 12 is a test rather than a support case.
- A mask that is not good enough reduces what may be done with it, and says so, in a field
  the panel renders. Property 3 above is answered by decision 6 and by nothing else.
