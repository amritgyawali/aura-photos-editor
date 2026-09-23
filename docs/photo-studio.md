# Photo Studio

## Start with an Instagram reference

The first section on the home screen is **Instagram style matching**. Paste a
public photographer profile and choose **Analyze Instagram style**, or use
**Use saved reference photos** for a folder of at least 8 distinct JPEG/PNG images.
Reference analysis works before a target collection exists. It shows the measured
palette, tonal character, color lean, actual photo count and skipped files.

Instagram retrieval uses Python + Instaloader (`python -m pip install instaloader`).
It makes anonymous requests; it does not read browser cookies or credentials.
Choose 60, 240, or all accessible photos up to 2,000. Retrieval also stops at
512 MB or ten minutes. Private profiles, login requirements and rate limits are
reported; the app does not claim full-profile coverage when only a sample arrived.
Instagram can block even public profiles. Saved references remain available offline.

When a reference is ready, **Choose your photos** creates a collection if needed,
imports the selection, and automatically fits the style to each image. **Apply to
this collection** applies a changed strength to existing photos. The reference
selection persists between launches. Clear it to use local auto enhancement only
on future imports; clearing does not revert edits already saved.

The fitter measures tone landmarks, color temperature, tints and eight color bands.
It uses lighting-matched reference groups when sufficiently populated, compares
bounded candidates through the real renderer, and keeps the closest measured
appearance. It protects manual settings and restarts from a local baseline to
avoid compounding edits. This is a statistical approximation, not recovery of the
photographer's original settings, lighting, lens, retouching or exact white balance.

## Import, review and export

Create a collection, then choose individual photos or a folder in **Photos**. When import finishes,
AURA automatically switches to **Auto edit**, corrects each photograph, verifies a
rendered preview. No second click is
required. Stopping import prevents this automatic hand-off.

**Auto edit all photos** repeats local preparation without requiring AI models.
The optional **Advanced wedding workflow & analysis** retains the longer pipeline
and its readiness checks. A **Render final output**
button opens export, where you choose an output folder and preset. The first export
uses imported photo IDs, so it does not depend on an earlier export or culling run.

**Review automatic preparation** shows the result for each photo. **Review every
step** shows each advanced stage's outcome, reasons, item counts, duration and
attempts. Each photo also has a persistent edit history; open **Review every edit
on this photo** and use Undo/Redo to inspect the saved changes.

Select a photograph in the filmstrip to see its actual rendered edit. **Auto enhance
photo** applies conservative local exposure, highlight, shadow and contrast corrections
without downloading models or configuring a provider. This is pixel analysis, not a
trained vision model. Inspect intentionally dark or bright scenes before export.

Enhancement measures each photo in linear light, limits exposure increases to
protect bright highlights, and scales shadow and highlight adjustments to the
image. Uniform black and white images are left neutral. PNG photos now import,
preview, edit and export alongside JPEG and supported camera RAW files. PNG
processing uses 8-bit sRGB, reduces 16-bit input, and composites transparency on
white; embedded non-sRGB profiles are not converted. HEIC/HEIF and unsupported
RAW compression still require conversion before editing.

Use **Compare** and the divider to inspect original and edited previews. Fine-tune
the numeric controls, then undo, redo or reset. Manual values stay protected from
automatic edits. The renderer reads imported pixels, preserves the original file,
and reports an error when it cannot decode a requested resolution.

## Advanced lighting-bucket profiles

1. Run the **Advanced wedding workflow & analysis** to prepare tone and color estimates.
2. Open **Instagram style → Advanced lighting-bucket look profiles** and choose a folder of saved reference photographs or an
   Instagram data export. The existing engine requires at least 8 references and
   recommends 24 or more. A profile URL is an optional source label; it does not
   download the account's images.
3. Press **Match and apply look**. AURA measures the references, selects the result,
   and runs tone followed by color grading to save edits to your photographs.
4. Adjust **Look strength**, then press **Apply strength** once. Moving the slider
   alone does not launch expensive editing jobs.
5. Inspect your photos in **Auto edit**, then use **Export**.

Stop preserves edits already completed. An interrupted application can leave a
partially edited collection; use **Apply again** to finish it. The match report is
the reference engine's sampled measurement, not a guarantee of identical results
for every subject, camera, light or strength setting. Advanced learned models keep
the availability and quality limitations described in the project README.

**Advanced** contains quality review, gallery consistency, albums, AI provider,
performance and storage settings.
