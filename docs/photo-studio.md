# Photo Studio

## Everyday portrait editing

Import your photographs and open **Auto edit**. Select a thumbnail in the filmstrip;
Left/Right arrows switch photographs, and Home/End select the first/last photo.
The editor opens in **Essentials**, with one-click **Auto enhance photo**, presets,
exposure, contrast, highlights, shadows, temperature, vibrance and crop. Switch to
**Advanced** for the complete Develop controls without changing the photograph's edits.

The RGB histogram measures the edited preview, sampling at most 100,000 pixels.
Its near-black percentage counts pixels with all channels at or below 2; near-white
counts any channel at or above 253. These display-space measurements help review
clipping; they are not RAW sensor measurements or a portrait quality score.

The photograph remains visible while edits save and render. Controls stay locked
until the new recipe, history and preview arrive. Empty numeric fields revert on blur;
Enter commits a value, Escape restores the stored value, and Enter followed by blur
creates only one edit. Failed saves are shown next to a retry action.

### Test with five real portraits

`scripts/test-portrait-studio.py` exercises the running native application through
its debug WebView, without substituting mocked IPC or generated pixel fixtures.
It imports five JPEGs into a new collection, checks deterministic auto enhancement,
manual-value protection, undo/redo and reset, exports five full-size PNGs, compares
their decoded pixels with the full renderer, and checks SHA-256 hashes of the originals.
It also checks the histogram, comparison, Essentials/Advanced views and filmstrip.

With `playwright` and `pillow` installed, launch a debug desktop with the process-local
environment variable `WEBVIEW2_ADDITIONAL_BROWSER_ARGUMENTS=--remote-debugging-port=9223`,
then run:

```powershell
python scripts/test-portrait-studio.py --photos PATH_TO_FIVE_JPEGS --output PATH_TO_RESULTS
```

Use a dedicated test folder. Results include `results.json`, before/after PNGs,
`before-after.jpg`, two editor screenshots and the verified `exports` directory.
This is functional validation on five portraits, not a general aesthetic quality benchmark.

## The start screen: look, reference, photos

AURA opens on **Start**, three steps on one page:

1. **Pick an edit profile** - sixteen researched looks plus profiles learned from professional
   RAW before-and-afters, each previewed by the export renderer. Choose one, set its strength and
   compare before and after on the sample scene, or on your own photo once one is imported.
2. **Match a photographer's Instagram** (optional) - described below. When you use both, the
   reference is fitted on top of the profile.
3. **Choose photos** or **Choose a folder**. The import starts and, when it finishes, every photo
   is edited with your profile (and reference) automatically.

How profiles adapt to each photo, and how the learned ones were measured, is in
[edit-profiles.md](edit-profiles.md).

## Start with an Instagram reference

The second step on the start screen is **Instagram style matching**. Paste a
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
image. Normally exposed midtones are preserved, and global darkening is capped
at a quarter stop: a bright background is not proof that a face is overexposed.
Uniform black and white images are left neutral. PNG photos now import,
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
