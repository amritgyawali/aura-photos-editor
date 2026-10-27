# Match a look from one finished photo or a reference folder

On the main **Match a look** screen, choose **Upload edited reference photo** for
one opaque JPEG, PNG or TIFF. **Choose reference photos** still accepts a folder
with at least eight distinct readable photos. Instagram exports retain their
existing folder workflow. References can be analysed before importing targets.

**Use original + edited pair** offers supervised colour calibration. Select the
unedited JPEG, PNG or TIFF first, then its edited version. They must have the same
native dimensions, orientation and framing. The original is rendered through the
same automatic baseline used for targets. A robust, regularised cubic RGB model
learns the additional global colour/tone operation; a 17 x 17 x 17 lookup executes
that model in both previews and exports. Targets reuse that operation instead of
being forced to reproduce the reference scene's object-colour proportions.

Calibration holds out entire spatial tiles for independent residual measurement,
balances colour-cell support, and downweights inconsistent pixels. Reframed or
poorly aligned pairs, excessive held-out error and neutral-ramp reversals are
refused before a reference is selected. These are qualification checks for a
global mapping, not proof of matching on every future image. A reference pair
with broad colour and tonal coverage provides better evidence than a narrow
palette. Local retouch, changed illumination, clipped detail and colours absent
from the pair remain limitations. Chosen profiles intentionally stack beneath
the learned transformation, and skin/highlight guards can reduce its strength.

You often don't need to pick the original yourself. When you apply one edited
photo to a collection that contains its camera file, AURA finds that file and
learns the edit from it. A file counts as the original when the edited photo's
name starts with its name (`989A2698-Edit.jpg` and `989A2698.CR3`) or when it
was taken in the same second. AURA then learns the edit from the two photos,
exactly as if you had chosen the pair, and applies that same edit to every
photo in the collection. Each result says which photo the look was learned
from. A reference folder works the same way: every edited photo whose original
is in the collection is used, up to eight, and they are learned together as one
edit. More of your own edited frames cover more of a wedding's colours, which
left about 11 % of the difference instead of 15 % in the same tests. A cropped or retouched export, or a different frame with the same name,
fails the pixel checks, and AURA estimates the look from the edited photo alone
instead. Measured on real photos, learning from the original leaves about 15 %
of the colour difference that the edited photo alone leaves at about 84 %
([ADR-0161](adr/ADR-0161-learn-a-reference-look-from-its-original-in-the-collection.md)).
On five wedding camera files with a Lightroom-style preset, it brought the
average difference from 3.56 to 1.47, where the edited photo alone reached
2.51. Highlights, Shadows and Clarity work locally, so a single learned colour
operation can only carry them part of the way to other frames. Because the
look is your own edit, it may change skin and highlights as much as your edit
did on its original, rather than being held to the stricter limits that
protect skin from an estimate. If a photo looked like the original but could
not be used, the result says why. So if you want the batch to match as closely
as possible, include the edited frame's original in the collection, export it
uncropped, and keep its file name.

The calibration is part of the saved recipe. Repeat application replaces it,
zero strength removes it exactly, and switching to an ordinary reference clears
the previous calibration. Protected recipe fields, Undo/Redo, cancellation and
transactional save checks retain their normal behavior.

Choose your target photos to import and edit, or use **Apply to this collection**
for an existing collection. Each target gets its own measured correction and fit.
Changing strength and applying again rebuilds the fit from that target's automatic
or chosen-profile baseline. Zero restores that baseline, including its retained
retouch and manual values; it does not remove all automatic correction.

The main workflow searches exposure, white balance, contrast, highlight/shadow
controls, individual tone-curve anchors, supported colour-band saturation and
luminance, and three-way colour casts. It uses the native decode for analysis,
including camera RAW, instead of treating the embedded camera JPEG as the export.
Reference orientation and supported ICC conversion are applied before measurement.

A single reference with very strong structure and aspect-ratio correspondence
can use a thumbnail pixel objective. Other photos and reference folders use
lighting-conditioned aggregate appearance with conservative edit regularisation.
Structure correlation is a heuristic, not proof of a shared camera original.
The thumbnail is never pasted into the target output.

The fit is measured at full look strength before the selected strength is applied.
Candidate recipes are merged with manual-field protection, validated, and rounded
to the recipe store's canonical precision before rendering. Repeating an identical
match therefore does not create a new edit merely from an unstored float tail.

Before saving, native-resolution renders check measured improvement, highlight
clipping and sampled detected-person skin guards. Reduced-fitting proposals can
be softened or discarded if they fail these checks. The recipe/history write
checks the original recipe hash transactionally, preserving newer edits. Stop
requests reach the native fitter between candidates; completed earlier batch
photos remain saved.

The search is bounded and costs more than the previous three-strength probe.
Up to two rounds of 160 parameter proposals and 36 cast proposals are explored,
with canonical recipe caching avoiding duplicate renders, followed by native
verification. The second round revisits tone and white balance with the first
round's split toning present, then refines the casts from their incumbent.
The calibrated-pair path fits once during analysis and checks the requested
strength through native renders without this per-target parameter search.

This approximates visible colour and tone. A lower appearance distance is not an
accuracy percentage and does not prove the same editing operation was recovered
between different scenes. Mixed lighting, clipping, strongly different subjects,
unsupported colour effects and a missing original reference limit what can be
inferred. Original/edited reference pairs would support stronger calibration.

The [main workflow validation report](reports/AURA-reference-first-native-2026-10-23.md)
separates this path from the experimental distribution-transfer matcher and its
remaining accuracy failures.
