# Adjustable finishing and reference colour

## Reduce or increase an automatic edit

Open a photo, run automatic retouch and choose **Automatic finish** in the editor
panels, or open **Precision retouch**. Expand **Adjust automatic finish**.
Adjust **Overall retouch** or the individual categories, then choose
**Apply automatic strengths**. Only categories with saved automatic operations
appear. The controls cover blemishes, smoothing, skin tone, wrinkles, eyes, teeth,
shine, hair, clothing and background where automatic retouch produced those steps.

100% means the original automatic amount. 50% halves that amount. 0% removes the
effect while retaining its original amount for restoration. You can increase a
strength to 200%; each operation remains bounded by its tool's maximum amount.
Overall and category strengths multiply. Changing a slider prepares a change;
**Apply** saves it. **Restore original strengths**, followed by **Apply**, restores
the automatic amounts. **Discard strength changes** reloads the saved sliders.

Each application is one Undo/Redo step. The saved baseline travels with the recipe,
so strength changes remain absolute after reopening. Manual and disabled steps are
preserved. An individually edited step leaves the group controls and remains
editable in its existing saved-operation controls. Rerunning detection preserves
adjusted operations rather than applying a second copy. New unadjusted operations
can still be produced by the new detection; review the resulting stack.

These controls adjust saved native retouch operations. They do not rescale every
global develop setting, generate missing operations, reshape a person or rerun a
detector. You can still adjust exposure, white balance and other Develop controls
separately. Save pending retouch edits before changing automatic strengths.

## Match an edited reference photo

Open **Match a look**, select a target photo in the filmstrip and, in
**Reference colour matching**, choose **Upload colour reference** for an edited
JPEG, PNG or TIFF. Choose the saved reference, select **Analyse reference match**,
adjust strengths and choose **Apply colour strengths**. A reference can depict a
different photograph. Similar subjects and lighting usually provide a more useful
comparison. AURA fits visible appearance; a finished JPEG does not uniquely reveal
the editor's original slider values.

The [Internet-reference and known-grade test report](reports/AURA-reference-colour-match-2026-10-22.md)
includes original-resolution downloads, native RAW verification and before/after
comparisons. Matching approximates appearance; arbitrary exact grade cloning is
not established. Strong split tones and monochrome references may need substantial
manual finishing. JPEG, opaque PNG and TIFF are supported reference formats;
WebP and transparent references are refused.

The [exported-colour verification](reports/AURA-exported-colour-verification-2026-10-22.md)
checks the actual delivered JPEGs and PNGs, including a 4480 × 6720 CR3 export.
Selected strengths reduce the regional distance, while reference-grade and
calibrated-white-balance equality remain unverified.

The overall, tone, colour and warmth controls range from 0–200%. **Fine matching
controls** separately scale exposure, contrast, highlights, shadows, whites,
blacks, curves, temperature, tint, vibrance, saturation and hue within that fit.
They scale the fitted treatment rather than setting absolute camera values. The
native renderer still checks highlights and skin colour. A stronger request can
be refused or reduced if it fails those checks. A zero fitted component cannot be
amplified into a new correction; use Develop to author that correction directly.

The adaptive matcher can explore corrections that were absent from its initial
guess, including independent curve anchors, colour bands and tone-dependent casts.
It uses image-structure evidence to limit large extra corrections between unrelated
scenes. Analysis checks its proposed treatment on the native decode and can reduce
that proposal; Apply checks the exact strengths you choose again. An accepted edit
does not certify an exact reference match.

### When the reference's original is in your collection

If the edited reference you upload was made from a photo in the collection you are matching,
AURA finds that photo and learns your edit from the two, pixel for pixel, instead of estimating
it from another scene. It counts as the original when the edited file's name starts with the
photo's name (`989A2698-Edit.jpg` and `989A2698.CR3`) or the two were taken in the same second,
and the pixels confirm it. The panel then says **Learned from your own edit of 989A2698.CR3**,
and every photo you match receives that same colour and tone operation. Amount scales it up to
100%; Tone, Colour, Warmth and the fine controls are greyed out, because a learned edit has no
separate parts. Your own edit may change skin and highlights as much as it did on its original,
and smooth skies, walls and floors are still checked for new bands. A cropped, rotated or locally
retouched export cannot be lined up with its original; the panel then names the photo it found,
says why it could not use it, and estimates the look from the edited photo alone. On the
known-look test set the learned edit left an error of 0.58 where the edited photo alone left
3.70 ([ADR-0181](adr/ADR-0181-match-a-look-assessment-fixes.md)).

### Smooth skies, walls and floors

Every match is checked on the full-resolution result for new bands and colour patches in areas
that were smooth before, at any brightness: a sky or a white backdrop as well as a shadow. If the
requested strength would band or blotch them, AURA uses the strongest strength that does not,
says so, or refuses and keeps your edit.

### Tone from a reference of another scene

A reference of a different scene mixes two things: what was in front of the camera
and what the editor did. Natural matching now estimates the editor's part of the
**tone** by comparing the reference with ordinary finished photographs (the
measured lightness landmarks of 37 varied photos ship with AURA; no photo does).
A reference whose blacks sit well above any ordinary photo's reads as a matte lift;
a reference with an ordinary tone range leaves your photo's tone alone. Your photo
is compared the same way, so one that is flatter than a finished photo has its
range brought back before the look is added. The panel says in one sentence what
it concluded, and **Tone match** scales it like any other tone.

This is a cautious estimate, not recovery of the editor's curve. A night scene and a
moody edit can look alike in one photo, and in that case the change is partial. On
the known-look test set it reduced the error against the true look on most photos
and made fewer photos worse than the earlier matcher; see
[ADR-0180](adr/ADR-0180-scene-prior-tone-for-match-a-look.md) for the numbers.
Natural analysis also now finds a strength that passes the full-resolution skin,
highlight and smoothness checks before offering it, as Palette analysis already did,
so the match offered at 100% is one Apply accepts.

Newly imported genuinely monochrome references can remove colour continuously.
Amount, Colour and Saturation match scale this contribution, and zero restores the
baseline. The monochrome guard preserves each person's tone and rejects new casts;
ordinary colour references retain the colour-preservation guard. A selective-colour
reference is kept as a colour reference. Reimport an older reference's source file
to supply fresh structure and monochrome evidence; portable libraries keep their
descriptor-only format.

To compare the updated matcher with an existing match, first choose **Remove
matched treatment**, upload the reference again, then choose **Analyse reference
match** and **Apply colour strengths**. Saved sessions retain their earlier fit
until you analyse again. Removing the old contribution first avoids treating it
as part of the next analysis baseline.

See the [adaptive matcher report](reports/AURA-adaptive-colour-matching-2026-10-22.md)
for code changes, retained prototype regressions and measured comparisons.

Matching starts from the saved analysis baseline to avoid compounding. Later
manual edits require a fresh analysis. **Remove matched treatment** restores the
matching contribution to zero. Undo/Redo also preserves the matching baseline.
Reference facial retouching and geometry are not copied onto the target.

## Reuse references

**Upload several colour references** imports a group of files; valid files are
retained if another file fails. Expand **Use this edited photo as a reference** to
capture the current saved edit, including its retouch and crop, without exporting
a temporary photo.

Expand **Reference library** to browse thumbnails, search, filter a group or show
favorites. Edit the selected reference's name/group, mark a favorite or choose
**Remember matching strengths for this reference**. Those strengths become the
starting settings when that reference is selected later. Existing saved matching
sessions keep their own settings.

**Remove reference from library** removes its reusable descriptor. It keeps the
source file, previous photo edits and their history. The collection workflow
measures each target separately and reports individual outcomes. Stop cancels
remaining work; completed changes remain saved. Originals remain read-only.

## Match regions independently

Choose the **Reference matching** panel and a saved reference. Save an initial
photo edit if the photo has no saved recipe yet. Expand **Match skin, hair,
clothing and background separately**, then select **Analyse regional colour
match**. AURA measures shared face skin, body skin, hair, clothing and background
regions and fits actual local colour corrections.

Each usable region has independent **Amount**, **Tone**, **Colour** and **White
balance** strengths from 0 to 200 percent. Adjust the drafts, then choose **Apply
regional colour strengths**. **Set all regional amounts to zero**, followed by
**Apply**, restores the saved analysis baseline, including existing manual masks.
Undo/Redo records each application. Switching editor panels keeps the drafts.

Missing or unsuitable regions have explicit reasons. A detected region with no
useful fitted correction shows disabled effect controls. Multiple people share
the class strength; AURA does not pair individual people with reference subjects.
Local regions adjust tone, saturation, temperature and tint. Whole-photo curves,
hue and vibrance remain in global matching. A full-resolution check can refuse a
requested correction. Later saved edits require a new analysis.

## Current limits

These are independently implemented AURA workflows. Evoto's proprietary
Quick/Control algorithms, trained networks and photographic equivalence remain
unestablished. Group finishing strengths apply across eligible automatic steps;
individual saved-step controls provide finer editing. See the
[feature and qualification report](reports/AURA-Evoto-workflows-2026-10-20.md).
