# ADR-0094: Healing all detected visible skin and confirmed residual spots

Date: 2026-10-10

The user requests spot healing comparable in appearance to Retouch4me Heal and
coverage of face, nose, forehead, cheeks, neck, hands, arms, body and legs. The
previous native results improved acne but left visible marks. This is a quality
and coverage requirement, not permission to flatten facial contours or to invent
an untested equivalence to a proprietary learned system.

Two regressions reproduce limitations in the old spot planner: a compact red
lesion on a smooth lighting gradient is rejected, and a confirmed residual lesion
retains a dark core because every residual patch is capped at 65% with 65% feather.
Fit a bounded robust local illumination plane to the surrounding ring instead of
rejecting all directional light. Keep sharp/curved shadow discontinuities excluded.
Fully heal the measured lesion core and feather the surrounding healthy skin.
Donors must still be clean, selected skin; feature masks remain in force.

Body ownership formerly dropped detached skin components beyond seven face sizes.
Assign every confidently segmented body component to the nearest detected person,
retaining partitioning for touching people. Blemish healing must also run when
surface finishing cannot find a clean sample. For body-only crops, run the bundled
skin segmenter without requiring a face and heal only its confident body-skin
class. Do not invent face geometry; competing face, hair, clothes, accessories and
background labels veto that body-only selection.

Existing manual operations, originals and saved recipe parameters are preserved.
New planner/segmentation version strings distinguish the new decisions. Every
healing operation remains inspectable, adjustable and reversible. Coverage still
depends on the bundled segmenter's evidence; neither universal skin detection nor
zero remaining defects may be claimed from the tests alone.

Validate disconnected skin and sample-free healing in native renderer tests, then
run fresh native face/body imports, verified full-resolution exports, original
hash checks, protected-feature checks and visual inspection. Add a body-only
photo regression. Hands and legs need real-photo validation before claims about
their appearance; synthetic region tests establish selection/processing behavior.

Native review of the first implementation still found nose and adjacent cheek
lesions masked by the broad wing exclusion. Keep broad frequency healing and
finishing on that conservative mask. Compact measured spot repairs instead use
a separate mask for eyes, actual nostril openings and underside crease, allowing
nearby wing/cheek skin to be repaired. Remove blanket glabella and smile-line
exclusions from this precise search; shape and ring-light checks still reject
creases and shadow edges. Complete repair rings and clean donors remain required.
Store separate masks so the precise selection cannot broaden the frequency-heal
or finishing operation. Raster tests must validate both selections at preview and
export sizes, and a native synthetic nose-side lesion must actually heal.

Wing inspection now distinguishes intended local blemish repair from broad
contour changes: broad selection on sampled wing cores must remain zero, and any
wing changes must lie in the saved compact-spot selections. Eyes and nostril
opening cores remain pixel-identical. The earlier damaged-region rectangles and
full-resolution contour inspection remain additional regression gates.

Use original luminance, grayscale at four times contrast and grayscale at one
quarter contrast as supporting measurements in both frequency healing and the
compact-spot search. Bound additional darkness evidence by original local
contrast and redness. Diagnostic clipping must not replace original lighting,
clean-donor or feature checks. A faint red pimple regression fails without this
analysis and passes with it. Preserve the existing exposure, pore, curved-shadow
and selection-boundary regression gates.

Expose the same three diagnostic views beside Original color in the retouch
workspace, using a labelled native select. Apply the selected display filter to
both halves of before/after comparison. Masks and saved coverage stay unfiltered.
These controls change review display only; native pixels, recipes and export
parameters remain unchanged. Inspect color output before accepting healing.

The native first-photo run reached the old 220 spot limit and still showed missed
lesions. Raise deep-cleanup capacity to 900 repairs per face, requested by explicit
acne/cleanup presets (512 for deep/professional), and retain a bounded 1,024-operation
recipe limit shared with manual controls. Store the count as u16; existing integer
settings deserialize unchanged. Preserve existing manual edits and report remaining
measured candidates when the requested spot budget is reached. The existing total
budget check still reserves non-spot/manual operations before truncating proposals.
More repairs take more rendering time; actual desktop export timings are evidence.
A dense synthetic acne field must produce more than 220 stable proposals, while a
220 setting remains honored. Oversized recipe writes must fail without changing
the previous recipe. Manual duplicate/edit controls must remain usable above 256.

The broad orbital exclusion also hid upper-cheek acne from precise repairs. A new
native-rendered regression fails before correction. Give compact measured patches
a separate eye core (0.34 by 0.23 eye distances, 25% transition), keeping the broad
orbital mask unchanged for frequency healing and finishing. Protect actual eye,
inner-corner and opening cores at both preview and export resolution. Complete
repair rings and clean selected donors still apply; do not bypass shadow checks.

An angled-photo structure rectangle had a three-level RGB change at its edge,
from a compact repair centered outside that rectangle. Default checks still
require pixel identity. An explicit feather-only inspector may instead establish
that broad selection is zero there and every changed point is outside every
accepted repair core, solely in the feather of a neighboring compact repair.
Report that change, rather than claiming the entire rectangle is unchanged. The
eye and nostril-opening cores continue to require exact identity. Skin contour
windows may contain requested compact spot repairs and are assessed separately.

The smaller precise eye mask exposed two weak neutral texture repairs in the
first-photo nose-shadow rectangle (six encoded RGB levels maximum). Neutral
candidates now need a second original-
light ring, at the larger of three repair radii or 10% of eye distance, with 90%
ring support for a smooth illumination plane. The smaller lesion ring retains
75% support for neighboring acne. Strong red lesions retain their measured
local-light path. A neutral mark near a shadow edge fails the old gate and is
correctly rejected by the wider context check. The context may read excluded
pixels for lighting evidence; it never adds them to healing selection.

Healing a blemish or blackhead within a skin contour window necessarily changes
its skin pixels. Distinguish that from the earlier flattened nose: optionally
render the saved stack without compact patches, using reversible isolated-catalog
diagnostic writes, and require every window pixel to remain identical in that
broad-only render. Every changed pixel in the final render must lie in a saved
compact repair. Within the named window, bound twice-blurred absolute RGB change
relative to baseline luminance below 1%, with each box radius at least 4% of eye
distance (ceil, at both resolutions). Changes outside the named window are not
part of that specific regression; inspect the full image separately. This gate
rejects the earlier real damaged wing at 59.9% and a synthetic flattened patch.
Report changed-pixel counts and maximum changes; do not call the whole window
unchanged. Eye/opening cores remain exact. This metric is a contour/color check,
not proof of professional retouch quality or zero residual blemishes.

The remaining real-photo clusters exposed two distinct rejection causes. An
elongated inflamed component failed the circular shape/clearance checks, and
landmark-only nostril exclusions covered actual acne above the visible opening.
Use measured axis-aligned repair ellipses for components with positive redness
across the component (mean above 0.012 and peak above 0.025). Neutral marks keep
the stricter compact-shape path. Every measured lesion pixel must lie inside the
solid repair core, and the entire ellipse plus sampling margin must fit selected
skin. Lighting rings follow the ellipse; donor search still requires a complete
clean square conservatively sized by the larger axis. Existing native recipes
already support elliptical regions, so old saved repairs are unchanged.

For precise spot protection, measure a pair of bounded dark nasal cavities in
small rotated landmark windows, across three exposure-relative thresholds. Require
compact area, bounded extents and compatible placement on both sides; otherwise
retain the old conservative landmark exclusions. Uniform skin and unbounded
shadows must not reduce protection. Expand measured openings for their immediate
edge, feather and matte interpolation. Broad frequency healing/finishing keeps
its existing nose/wing guard, and the underside guard remains independent.
Validate real anatomical opening cores using independently inspected photo
rectangles; record their coordinates explicitly rather than calling an inaccurate
landmark guess an anatomical measurement. Eye and contour checks remain required.

Touching deep cores require additional score tiers (4.5, 7, 12 and 20) restricted to
positive local redness. A regression fails the original tiers and the first two
additional tiers because the inflated contrast response still joins deep cores.
Real residual traces also show neighboring inflammation contaminating the light ring. For strongly red
components, exclude only positively identified neighboring red samples, retaining
at least ten samples and two in every quadrant. Require 90% of those original-light
samples to agree with the same plane within 4%; never fill missing/protected ring
samples. Neutral marks retain their original shape/context checks.

The renderer's original robust fit can give all samples full weight when about a
third of its ring contains neighboring acne. Add a default-off, omitted-when-false
`cleanRingFit` recipe flag. New confirmed inflamed repairs request joint RGB
least-trimmed fitting over the healthy majority, retaining real unscaled donor
texture. Old repairs keep the original five-pass fit exactly. New repairs use
eight bounded passes with the lowest 60% joint residual support; channels are
never selected independently. Test contaminated-ring tone reconstruction against
the legacy negative control, untouched pixels outside the repair, serialization,
replay, manual strength adjustment and real saved-recipe compatibility.

Twelve measured first-photo defects still lacked a donor. Donor cost previously
matched the blemish-darkened and reddened center, so clean healthy skin could fail
the unchanged 0.45 cost limit. Match the accepted lighting plane's center and the
clean surrounding ring's redness instead. Keep all source-square clearance,
disjointness, contamination and fallback-scale checks unchanged. A negative control
with the wounded center rejects the donor; the measured healthy reference accepts
it under exactly the same cleanliness constraints. This changes new plans only;
saved donor coordinates and existing repairs remain unchanged.

Replay of an exact native planning input confirmed selected nose lesions rejected
by the affine illumination gate. New inflamed repairs may request default-off
`curvedHeal`: a smooth quadratic fit over three original-light rings, with at
least 58 healthy samples, 16 per ring and 12 per quadrant. Every context sample
must still be selected skin. Bound curvature/gradient and require 90% original
light agreement within 4%; hard shadow steps remain rejected. The corresponding
native renderer fits joint RGB curved illumination over the same three radii and
transfers unscaled real donor texture. Old affine fits and existing recipes keep
their original rendering. A curved-surface regression rejects the flattened affine
negative control and verifies reconstructed illumination, replay and untouched
pixels outside the repair. Independent eye/opening and contour checks still apply.

Explicit deep cleanup performs at most one further residual pass on the native
rendered, protected first-pass repairs, within the existing per-face spot budget.
Guard temporary copies for prediction, then guard saved edits once, avoiding a
double matte multiplication. Additional repairs require measured inflammation and
cannot be centered in any previous repair footprint, including its blend. They retain ordinary native
operations and unique stable IDs. No recursive cleanup, unbounded work or automatic
overwrite of manual edits is introduced. Temporary exact-input QA capture code
was removed after replay; the opt-in ignored inspector can read the local captured
frame for future regression diagnosis.

Full-resolution review caught a faint chin repair ring when curved fitting also
replaced supported affine fitting. Retain the existing affine fit whenever it
passes; request curved reconstruction only when the flat gate fails and measured
curvature exceeds 1.5%. Exclude the complete previous footprint from second-pass
repair centers so borrowed pores and blended edges are not treated as new acne.

An exact native residual regression still rejected an inflamed nose lesion because
its three-ring support contained only four healthy samples in one quadrant. New
plans may search successively out to three target radii for healthy selected-skin
lighting samples. Keep the original complete target-ellipse containment and feature
guards. Stop at the smallest context supporting the unchanged 58-sample,
12-per-quadrant, 90%-within-4% illumination test and curvature limits. Skip protected
or inflamed context samples rather than treating them as healthy skin. Deduplicate
actual pixel positions. Store the accepted positions as `healSamples`, in target
ellipse coordinates, so native export uses that measured support instead of a
contaminated circular boundary. Old recipes omit this default-empty field and
retain their original fit. Bound persisted support to 58..224 finite points at
1..3.1 target radii, requiring an explicit texture-heal donor and curved fit.
Invalid saves preserve the existing recipe. Test contaminated neighboring lesions,
hard shadow steps, empty selection, replay, manual strength changes and untouched
pixels outside a repair.

Search farther within the same selected skin for a full-size clean donor before
falling back to a smaller reflected source. Additional reaches 32, 48 and 64 retain
the same entire-source cleanliness and matching-cost limits. This changes new
donor choices only; previously saved edits replay unchanged. A regression places
the only full-size clean source beyond the old 24-radius search limit.

Full-resolution review still found mirrored pore repetition in a large chin
repair whose only clean donor was 35% of the target size. New small-donor plans
may save `textureSources`: three to eight separately checked, spatially separated
clean patches from the same skin selection, using the original contamination,
clearance and matching limits. Transfer their real high-frequency detail through
deterministic overlapping patches with scale-preserving rotations. Smooth overlap
weights and energy normalization retain fine texture without a regular reflection
grid; the target's saved affine/curved lighting remains independent. Store donor
coordinates explicitly; empty lists preserve all previous single-donor rendering.
Tests compare against a mirrored-pattern negative control, retain texture energy,
check continuous seams and exposure scaling, and verify serialization/replay and
pixels outside the target. Validate list bounds, finite normalized coordinates,
an explicit PatchHeal source, texture healing and a smaller donor footprint.

Manual strength adjustments preserve measured lighting and donor references.
Changing the donor or donor size clears the extra automatic donors; changing the
repair footprint or tool clears both measured-light positions and extra donors.
Clearing the source also clears measured lighting. This prevents manually moved
or resized operations from reusing stale support for automatic healing.

The first multi-donor replay still found no alternatives for that chin repair:
32 polar directions stepped over clean forehead/cheek patches. Supplement that
search with at most 4,096 grid positions over the existing skin crop, scaled by
the donor footprint. Keep each complete source clean and outside the target;
retain the same matching-cost limit and separated-source requirement. An off-ray
regression and the captured native chin fixture require multiple actual clean
donors rather than merely testing that the optional recipe field exists.

## Residual pigment and per-photo workflow - 2026-10-11

Some red lesions touch lighting that neither an affine nor a curved fit can
safely reconstruct. A bounded fallback can reduce their pigment without replacing
their photographed luminance or high-frequency detail. Require strong redness
through the measured component, a complete selected-skin ring, at least ten
healthy ring samples with two in every quadrant, and a cleaner reference color.
Keep texture repairs ahead of pigment-only proposals; the fallback must not
suppress a supported texture repair. Use the same independently clean donor
search, with a conservative six-analysis-pixel minimum source clearance for the
native color sampler. Save an ordinary SkinUniformity operation confined to that
lesion's ellipse with a soft feather and the precise eye/nostril matte. Do not
apply a whole-nose color replacement or loosen structure validation. Pigment
correction does not claim to remove unsupported roughness or all remaining marks.

Evoto's public portrait and batch documentation separates blemish removal,
texture retention, skin-tone adjustment, review, presets and export:
https://www.evoto.ai/features/portrait-retouching and
https://www.evoto.ai/features/batch-edits (checked 2026-10-11). These guide the
workflow, not access to Evoto's private models. AURA's existing collection edit
measures each photograph's own light, color and scene and evaluates its saved
retouch preferences. Preserve originals, manual changes and reversible history.
The interface must restore the same 900-spot deep-cleanup bound as the backend,
rather than truncating a saved photo's budget to 220. Verify available face/body
photos through native collection editing and full-resolution export. Do not
infer universal accuracy or competitor equivalence from these fixtures.
