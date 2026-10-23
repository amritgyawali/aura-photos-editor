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

In **Reference colour matching**, choose **Upload colour reference** for an edited
JPEG, PNG or TIFF. Choose the saved reference, select **Analyse reference match**,
adjust strengths and choose **Apply colour strengths**. A reference can depict a
different photograph. Similar subjects and lighting usually provide a more useful
comparison. AURA fits visible appearance; a finished JPEG does not uniquely reveal
the editor's original slider values.

The overall, tone, colour and warmth controls range from 0–200%. **Fine matching
controls** separately scale exposure, contrast, highlights, shadows, whites,
blacks, curves, temperature, tint, vibrance, saturation and hue within that fit.
They scale the fitted treatment rather than setting absolute camera values. The
native renderer still checks highlights and skin colour. A stronger request can
be refused or reduced if it fails those checks. A zero fitted component cannot be
amplified into a new correction; use Develop to author that correction directly.

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
