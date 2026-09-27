# ADR-0065 - Lightroom panel parity: five optional recipe blocks and four render stages

**Status:** accepted
**Date:** post-plan, after PHASE-31
**Deciders:** CTO, TLC, PM, COL
**Supersedes:** nothing. **Amends:** the frozen edit recipe (`aura_recipe::Global`, ADR-0029) and
the render stage order (`aura_render::graph::ORDER`).

## Context

AURA's recipe covered Lightroom's Basic panel, the point curve, the HSL mixer, black-and-white,
detail, lens and geometry. It had no parametric tone curve, no RGB curves, no colour grading, no
camera calibration and no Effects panel (post-crop vignette and grain). Every well-known look a
photographer asks for - teal and orange, film, matte, moody - is built in Lightroom with at least one
of those, and the edit profiles had to approximate them with temperature and HSL, and the vignette
with a radial mask.

The recipe is a frozen contract. Changing it risks every stored render hash, which ADR-0029 section 5
makes the proof that a delivered file can be re-created.

## Decision 1 - Five blocks on `Global`, absent from the canonical form while neutral

`parametric`, `channel_curves`, `colour_grade`, `calibration` and `effects` are added to `Global` with
`#[serde(default, skip_serializing_if = "...::is_neutral")]`. A recipe that does not use them
serialises byte-for-byte as it did, so **no stored `recipe_hash` or `render_hash` moves** and an older
build reads a newer recipe by ignoring what it does not know. A test asserts the canonical form of a
neutral recipe does not contain any of the five names.

The units are Lightroom's: amounts `-100..100`, grading hue in degrees and saturation `0..100`, the
parametric splits `25/50/75`, vignette midpoint and feather `50`, grain size `25` and roughness `50`.
That is what lets the XMP writer use Lightroom's own attribute names (`ParametricShadows`,
`SplitToningShadowHue`, `ColorGradeBlending`, `PostCropVignetteAmount`, `GrainFrequency`,
`ToneCurvePV2012Red`, `RedHue`, `GrayMixerRed`, ...) and read them back, so an edit opened in
Lightroom carries these panels across rather than silently dropping them.

## Decision 2 - Omitted means neutral, in the merge

Because a neutral block is omitted, a proposal that returns one to neutral *omits* it. The merge
treated an omitted leaf as "not mentioned", which would have left a previous colour grade behind
forever when a look without one was applied. `schema::OPTIONAL_BLOCKS` names the five, and the merge
removes a block the proposal omitted - unless a person set something inside it, in which case the
removal is refused and reported like any other protected field.

## Decision 3 - Four stages, placed where Lightroom places the work

- `Calibration` directly after `CameraMatrix`: calibration adjusts the camera profile's primaries, so
  every later colour control works on its result. It is a white-preserving 3x3 plus a shadow tint.
- `ColourGrade` after `Monochrome`, so a black-and-white conversion can be split-toned.
- `PostCropVignette` and `Grain` after `Geometry` and before `OutputTransform`.

The parametric and RGB curves are not stages. The parametric curve composes with the point curve into
the one luminance table `Stage::Curve` already samples, and the channel curves run inside the same
stage, so the inner loop still does one lookup per pixel.

`restoration_order.rs` asserted that only geometry and the output transform follow sharpening. The
post-crop vignette and grain are the documented exception: grain must follow sharpening or the
sharpener amplifies it, and the vignette is a smooth gain drawn on the delivered frame. Both are
identical on a preview and an export by construction, which is the property that test protects.

## Decision 4 - Post-crop effects are drawn in delivered-frame coordinates, and tiles agree

The vignette's centre and radius are the crop's, and grain is a function of the frame coordinate
alone (value noise over a hashed lattice, sized against the long edge). The streamed path strips both
from its per-tile recipe and applies them to each committed tile in output-raster coordinates, and
`a_streamed_render_with_every_lightroom_panel_equals_a_whole_one` asserts the two paths agree to the
byte.

## Decision 5 - Shaders are held to the reference

`creative.wgsl` declares `stage_calibration`, `stage_colour_grade`, `stage_post_crop_vignette` and
`stage_grain`, and shares `GRADE_TINT`, `GRADE_LUMA_STOPS`, `VIGNETTE_STOPS` and `GRAIN_STRENGTH`
with `aura_render::creative`, which `shader_parity` compares. The grain lattice is hashed on the host
because WGSL has no 64-bit integers; nothing executes a shader in this build (ADR-0029 section 4).

## Blast radius

`grep -rln 'Global {\|global\.\(parametric\|channel_curves\|colour_grade\|calibration\|effects\)' crates tests`
at the time of the change:

- `crates/aura-recipe/src/contract/recipe.rs` - the five blocks.
- `crates/aura-recipe/src/fixtures.rs` - the only two `Global { .. }` literals in the workspace.
- `crates/aura-recipe/src/schema.rs` - clamping, validation, `OPTIONAL_BLOCKS` in the merge.
- `crates/aura-recipe/src/xmp.rs` - Lightroom attribute names in and out.
- `crates/aura-render/src/{graph,cpu,tiles}.rs` - the four stages and the streamed path.

Every other crate reads `Global` by field and never constructs it, so the additive blocks compile
through untouched; `crates/aura-app` gains readers (edit profiles, the develop panel) after the lock.

## Consequences

- `contracts.lock` is re-locked for `crates/aura-recipe/src/contract/recipe.rs`.
- The edit profiles use real colour grading, grain and a post-crop vignette instead of approximations.
- Still missing from Lightroom parity, and recorded in `docs/lightroom-parity.md`: HDR and panorama
  merge, a generated AI mask set (the mask generators are placeholders - phase 18's C1), content-aware
  healing with a brush (phase 24 refuses unclassified removals), soft proofing, and a GPU backend.
