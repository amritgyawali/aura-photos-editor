# Retouch recovery and native portrait validation

Date: 2026-10-08. Source: `C:\Users\amrit\aura-eye-protection`, branch
`fix/eye-protection-and-retouch-coverage`, based on `a5c4fe0` with the existing
uncommitted eye/nose protection and coverage work plus the recovery fix.
The original `D:\aura photos editor` checkout has missing tracked source files
and was not the source built for this validation.

## Change and regression

After a failed photo/recipe reload, the workspace previously kept the old preview,
saved operation list and history. The new regression reproduced stale coverage
remaining visible. Reload now clears that state and blocks edits/history until
the preview loads successfully. Reload retouch and Back to Develop remain usable.

The export driver additionally hashes the imported copy, not just the source it
was copied from. `verify-retouch-recovery.py` injects one read-only preview failure
after Undo in an isolated test catalog, checks that stale pixels and edit controls
are unavailable, reloads, and Redoes to recover the original recipe hash.

## Completed code checks

- Baseline frontend: 78 files, 692 tests passed.
- After the recovery change: 26 focused workspace/coverage tests passed.
- TypeScript and production frontend build passed.
- Native application library: 120 tests passed, including feature protection and
  residual acne repair.
- IPC surface: 302 definitions, registrations and frontend calls agree.
- Rust formatting and patch whitespace checks passed.

The frontend build reports its existing large-chunk warning (about 621 kB before
gzip). This run does not establish cross-platform or release-installer validation.

## Native application evidence

Evidence directory: `output/continue-2026-10-08` (ignored by Git; contains portraits).

The desktop executable SHA-256 was
`73ef29ed8f0f4ada190764e36623b9c210618d159ecd5b5490f845d7d17aaf72`.
The native app completed import, acne-only retouch and verified 2048x3072 JPEG export
on the real portrait. It saved 81 operations (1 frequency heal, 80 patch repairs).
Retouch took 14.59 seconds; export took 18.07 seconds. Both source and imported
copy hashes stayed unchanged. These timings describe this local run only.

The recovery driver verified stale-preview clearing, disabled edits on load failure,
successful reload and exact recipe restoration after undo/redo. It supports both
WebView2 postMessage and custom-protocol fetch transports; the first incomplete
fault-injection attempts were test-driver issues and were rerun successfully.

Eight additional native workflow checks passed: all six navigation destinations,
two-photo import, one-click collection edit, independent manual exposure, Develop
undo/redo, before/after viewing without recipe changes, verified two-file export,
and all five Advanced panels opening. No page errors were recorded. External AI
providers and Instagram network retrieval were not exercised.

Original nose-exclusion result: 41,547 eye/nose preview pixels were unchanged.
Visual inspection found remaining circular repair boundaries on the lower cheek/chin
and untreated nose acne. The user then requested nose coverage. ADR-0108 supersedes
the old whole-nose exclusion for blemish tools; results above describe the earlier
build and do not establish the newer nose-inclusive behavior.

## Nose structure correction, 2026-10-09

The first nose-inclusive export failed visual review: frequency healing treated a
chromatic nose shadow as a blemish. An unsaved native draft disabling frequency
healing isolated the cause. Frequency healing now rejects candidates with
incompatible surrounding light, even when their redness passes the mark test.
Residual patch planning applies the same conservative lighting requirement.

Code checks passed: 694 frontend tests, 122 application-library tests, 176 renderer
library tests, nine skin-finish integration tests, and the real-photo renderer
check. TypeScript/production frontend build, strict Clippy for application and
renderer targets, Rust formatting, and the 302-command IPC consistency check passed.
The Windows GNU linker still emits the existing multiple-manifest resource warning;
the desktop build completes and launches.

Native evidence is in `output/nose-structure-2026-10-08` and the fresh repeat in
`output/nose-final-2026-10-09`. Both generated the same 81-operation acne-only recipe
(`66796fdcf26c3f59d27cfc85ef09887517a27df60b2badb224573eedc74f965a`)
and byte-identical verified 2048x3072 JPEGs. The source and imported-copy SHA-256
hashes stayed unchanged. Global adjustments stayed neutral.

The protected-detail native check measured zero channel changes in 25,442 eye
pixels, 1,340 nostril pixels, and a 2,496-pixel nose-shadow region. Of 9,864 nose-skin
pixels, 7,733 were selected and 2,847 changed. This distinguishes selection from
actual repair. The earlier failing preview changed 740 pixels in the shadow region.

Full-image and full-resolution nose/mouth/chin inspection shows the jagged nose
artifact removed, with original contour, nostrils, lip edges and lighting retained.
Some marks remain near protected features and uncertain shadow transitions; the
result is conservative blemish reduction, not complete acne removal or a guarantee
for arbitrary photos. Preview pixel equality does not imply JPEG byte equality
against the source, because export re-encodes the photograph.

The fresh native manual-edit check passed: an unsaved brush draft did not alter the
recipe; apply saved one operation; per-operation coverage, opacity, zoom and split
comparison worked; removing that operation restored the exact recipe hash.

During validation, Export could become enabled before its preset finished loading.
The button now waits for the selected preset. A regression verifies the disabled
loading state and one successful callback after the preset arrives.

## Face and body small-mark cleanup, 2026-10-09

Validated source: `C:\Users\amrit\aura-eye-protection`, branch
`fix/eye-protection-and-retouch-coverage`. The original D: checkout was not restored
or overwritten. Installed desktop SHA-256:
`9ab28c11d41b287ff578327aab9396cf168f7c54dd20669e079705ea0df2d328`.
The isolated launcher and evidence are in `output/full-skin-2026-10-09`.

Acne only now chooses face and body skin, searches a smaller spot scale, and allows
220 residual face repairs. Body cleanup uses shadow-aware frequency healing and
excludes every detected face. Broad beauty finishing remains off in this preset.
This covers detected visible skin; these two portraits do not establish coverage
for all full-body poses or clothing/occlusion conditions.

Passed: 696 frontend tests, 123 application-library tests, 176 renderer-library
tests, 10 skin-finish integration tests, production frontend build, strict Clippy
for both Rust crates and all targets, formatting, and the desktop build. The new
regressions check a small red mark with unchanged surrounding pores and body-mask
exclusions around two faces at preview and export resolutions.

Native automatic runs imported, reset, retouched and exported both real portraits:
the 2048x3072 acne portrait used 222 operations (two frequency heals and 220 spot
repairs); `E:\13332.jpg` at 1333x2000 used 70 (two and 68). Export readback verified
both files, originals and imported copies retained their hashes, and global
adjustments remained neutral. These automatic results are stored in `real` and
`second`, separately from subsequent reviewed corrections.

On the first automatic result, 25,442 sampled eye pixels, 1,340 nostril pixels, and
the 2,496-pixel nose-shadow regression region were unchanged. Nose skin coverage
was 78.4%, with 3,450 changed pixels. Body coverage selected 88,192 pixels and
changed 21,483; no body coverage crossed a detected face. The second result kept
7,761 eye and 410 nostril pixels unchanged and selected 76,038 body pixels, of
which 8,343 changed. Its angled nose sample had only 48.1% coverage, so it failed
the first portrait's 60% coverage threshold. The measurement-only rerun certifies
feature protection, not sufficient nose coverage.

Visual review still found missed lesions after the automatic runs. Separate native
patch operations were added for reviewed forehead, cheek and chin spots. Three
nose patches were rejected after full-resolution review and replaced with one
using a cleaner source. The final first portrait has 13 reviewed operations in
addition to the automatic 222; the second has one in addition to 70. These manual
results must not be attributed to the automatic detector.

Accepted exports are `accepted-final/export/gallery/` (first portrait) and
`final-second/export/gallery/` (second portrait). Earlier `final-first` and
`accepted-first` exports are intermediate review evidence, not the accepted nose
result. Final recipe hashes are
`00e96b0d8b924c2d9b5602de9f0f21b3a55594c48b70e69fb31fce287cb6f4bf` and
`0a1f333d836a854d077609f3222a947d9297b167c9611df275dd7a4f875d7f0e`.
Both final exports passed readback verification without changing the saved recipes
or source hashes. Face and nose crops were inspected at full resolution.
The final native pixel checks again kept all sampled eye/nostril cores and the
first portrait's nose-shadow region unchanged; final nose-skin changed-pixel count
was 4,461 on that portrait. After a graceful desktop restart, both final recipes
retained their exact hashes and both 1066x1600 native previews were byte-identical
in RGB to their saved pre-restart previews (`restart.json`).

Native manual editing passed draft/save/remove, per-operation coverage, viewing
without recipe changes, and split comparison. Simulated preview failure cleared
stale images, disabled editing, and recovered with the exact undo/redo recipe hash.
All eight desktop workflow checks passed, including six navigation destinations,
two-photo import/edit, independent settings, history, comparison, direct verified
export without filename preview, and five Advanced panels. No page errors were
recorded. External AI providers and Instagram retrieval were not tested.

Quality limitation: remaining redness and marks near protected features/shadow
transitions are visible. The result improves blemishes while preserving structure;
it does not satisfy a universal zero-blemish or perfect-retouch claim. No geometric
reshaping or AI-generated image replacement was used.

## Pale-centered lesions and pore refinement, 2026-10-10

A new `Skin cleanup · refine pores` preset combines the existing face/body cleanup
with adjustable moderate smoothing, pore refinement and restrained redness
correction. The acne-only preset remains separate. Eye and nose-detail protection
remain enabled, with no eye/teeth enhancement.

The initial small pale-center test passed unchanged. Expanding the pale center
to 60% of the lesion radius reproduced a missed eight-pixel red lesion. Changing
the bright-pore rejection to read the component's maximum redness fixed that
regression. All 124 application-library tests then passed, including bright-pore
exclusion, long-edge exclusion, protected features and body-face mask tests.
The 14 settings UI tests, frontend production build, and strict application Clippy
checks passed.

The initial native skin-cleanup build exported both portraits with unchanged
originals, but close inspection exposed a flattened bright patch on the angled
portrait's outer nostril wing. This is a quality failure despite passing the
earlier eye/opening-core pixel checks. The earlier acne-only export also contains
that patch; those narrow checks did not establish preservation of the entire
nose. Disabling healing and broad finishing separately showed that both
contributed. Evidence is in `output/skin-refinement-2026-10-10/second/` and
`nostril-diagnosis/`.

The wing regression failed before the fix at 128 pixels. The feature masks now
exclude the wings from both healing and broad finishing, retaining bridge/tip
eligibility. All 124 application tests passed after that change; the protection
test now checks both tool families at 128 and 512 pixels. Native verification
also checks the wing cores and the observed damaged region explicitly.

The wing-only build (`2a075c36b7fe4177ccd33ac725b03fe9f01255a74e4ddb352ae0d95cbd4a9a5e`)
preserved the angled portrait's wing in its full-resolution export. Native checks
kept all 1,229 wing-core pixels and the 456-pixel damaged-region rectangle unchanged.
However, the first portrait's shadow regression rectangle changed by up to 31
channel levels. Read-only draft ablation traced this to frequency healing; disabling
that operation returned the rectangle to an exact match. This build is intermediate,
not the final accepted result (`output/skin-refinement-wing-2026-10-10/`).

A unit regression reproduced how masking a dark sector incorrectly allowed a
contour to pass the surrounding-light check. The renderer now reads all in-frame
neighbors for lighting context, keeping selected-only samples for donor/skin
evidence. All 177 renderer unit tests passed after this correction.

The final lighting analysis uses the photograph's actual luminance, not the
selection-weighted estimate (which is undefined inside fully excluded areas).
Validated executable SHA-256:
`e3af25972a55bc151158aa62afe316ccc9b2e6bce672702c30b091a7cc6d890c`.
Source/build: `C:\Users\amrit\aura-eye-protection`,
`fix/eye-protection-and-retouch-coverage`; isolated catalog and evidence:
`output/skin-refinement-verified-2026-10-10/`, WebView port 9357.

Automated checks passed: 697 UI tests, 124 application tests, 177 renderer tests,
10 skin-finishing integration tests, strict Clippy for the changed native crates,
frontend production build, formatting and diff checks. The final help-text change
also passed all 14 settings tests. Existing bundle-size and GNU manifest-linker
warnings remain; both builds completed.

Fresh native Skin cleanup runs exported 2048x3072 and 1333x2000 JPEGs with readback
verification, unchanged source/imported-copy hashes and neutral global edits.
The first automatic result has 226 operations (220 patches); the second has 65
(59 patches). Original SHA-256 values remain
`4ac9e7fa858996ca0a476867a061c54844de63340b005a88a954821479273f19` and
`577448e4d45a39cfc6ae254f30a23f70eeca9165a8d5091250282d1ab8810663`.

First-portrait native checks preserved 25,442 eye pixels, 1,340 nostril-opening
pixels, 4,014 wing-core pixels and the 2,496-pixel shadow regression region exactly.
Nose-skin coverage was 64.5%, with 3,015 changed pixels. Body coverage selected
88,192 pixels and changed 27,489, without crossing detected faces. The second
portrait preserved 7,761 eye pixels, 410 opening pixels, 1,229 wing pixels and the
456-pixel wing regression rectangle; body coverage selected 76,038 pixels and
changed 30,814. Its angled nose sample has only 31.7% coverage, so that run is a
feature-preservation check, not a claim of sufficient coverage everywhere.

Full-resolution nose, forehead, chin, neck and arm crops were inspected. The
flattened wing patch is gone, and the shadow-edge regression is fixed. Forehead,
cheek and chin blemishes are reduced, with visible skin texture retained. Marks
near the protected nose remain. Of ten restored reviewed patches, the nose patch
was rejected after its export showed a coarse texture transition; intermediate
`reviewed-export/` is review evidence, not the accepted final image. No universal
zero-blemish, pore-free or perfect-retouch claim is supported.

Accepted first export: `accepted-export/export/gallery/professional-retouch-verification_0001.jpg`,
235 saved operations (226 automatic plus nine reviewed patches), recipe
`ab1a83c96ff916f1085dc5f818a8b738d92d615005acb60744499c5a88148ebb`.
Second export: `second/export/gallery/second-portrait-verification_0001.jpg`,
65 automatic operations, recipe
`f0438682ba01e70c32756ed468fa6928561e2f5b9cbfb8dc03ee9e75e591fdfd`.
The accepted export passed readback without altering its recipe or original.
Final preview checks kept both observed nose-regression rectangles unchanged.

Native manual-edit verification passed draft/save/remove, per-operation coverage,
view-only controls and split comparison. Preview-failure injection cleared stale
images, blocked editing until reload, and restored the exact recipe through
undo/redo. All eight desktop workflow checks passed with no page errors: six
workspaces, two-photo import, one-click editing, independent exposure changes,
history, before/after comparison, direct verified export and Advanced panels.
External AI providers and Instagram retrieval were not exercised.

After a graceful desktop restart, both accepted recipes retained their hashes
and both 1066x1600 native previews were byte-identical in RGB to their pre-restart
versions. The protected nose-regression rectangles also remained unchanged
(`restart-verification.json`). The tested app is left open in the isolated catalog.

## Full visible-skin healing and residual repair, 2026-10-10

The user requests healing all visible skin, including nose, forehead, cheeks,
neck, hands, body and legs, with spot-healing quality comparable to Retouch4me.
ADR-0110 records the local algorithm changes and the required evidence.

Two baseline regressions failed: a red spot on a smooth illumination gradient
produced no repair, and the residual patch left a red-channel value of 0.5196
against healthy surrounding skin at 0.5882. The new robust ring-light check
accepts smooth gradients and rejects sharp/curved shadow edges. Confirmed residual
patches now repair their entire lesion core at full strength, with 25% feather
on surrounding skin. The gradient and rendered-residual regressions pass.

Detached body-skin components were previously dropped beyond seven face sizes;
all confident body components now retain an owner. Body blemish healing also
continues when a clean sample cannot be found for surface finishing. A separate
body-only path runs the bundled segmenter without requiring face landmarks;
competing face, clothing, hair, accessory and background classes veto selection.
Synthetic disconnected neck, hand and leg regions were selected and their marks
were actually healed, while pixels outside those regions remained unchanged.

All 129 application tests, 60 vision tests, 14 retouch-settings tests, strict
Clippy, and the frontend production build passed. Before/after test evidence:
`output/spot-healing-2026-10-10/`. The previous desktop binary was also tested on
`body-only-source.png`, a read-only derivative crop of `E:\13332.jpg`: its recipe
had zero detected faces and zero healing operations, with the message "Portrait
retouch was skipped." This reproduces the body-only failure in the native app.

Grayscale, four-times contrast and quarter-contrast analysis now support the
spot detector without grading the output photograph. The retouch workspace also
offers these as view-only controls for both sides of before/after comparison;
selection masks and saved coverage remain unfiltered. A faint red pimple test
failed before this analysis and passes afterward.

The 220-spot cap left valid repairs out of the real-photo result. Explicit acne/
cleanup presets now request up to 900 spots, deep/professional presets 512, and
the recipe limit is a bounded 1,024 operations. Existing saved integer settings
remain readable. Dense synthetic acne produces more than 220 stable proposals;
the 220 setting still returns the same first proposals and reports remaining
candidates. Oversized writes preserve the previous recipe. UI manual duplication
remains available above 256 operations.

Broad orbital/wing masks had excluded upper-cheek and nearby nose skin from small
repairs. Precise patches now use separate eye/opening cores; broad healing and
finishing retain their original exclusions. The upper-cheek native-render test
fails with the broad mask and passes with the precise one, without changing its
sampled eye/opening pixels. Native review also exposed weak neutral repairs near
the nose-shadow contour; a wider original-light ring now rejects those candidates.
The shadow-neighbor regression failed before this additional context gate.

Latest automated checks: 135 application tests (one optional real-photo inspector
ignored), 67 recipe tests, 177 renderer tests, 60 vision tests and 20 skin/patch
integration tests pass. The full UI suite passed 699 tests; the last preset/UI
changes also passed all 40 affected workspace/settings tests. Strict Clippy,
formatting, frontend production build and the desktop custom-protocol build are
checked separately. Existing Vite bundle-size and GNU manifest-linker warnings
remain; neither prevented a runnable build.

Final tested source: `C:\Users\amrit\aura-eye-protection`, branch
`fix/eye-protection-and-retouch-coverage`; planner `measured-features-v10`, skin
segmenter `mediapipe-selfie-multiclass-256-aura-v4`. Desktop executable SHA-256:
`03fbe4a9a4c9e4f0735f1e5917af12cf07191969ee0ed4e69f54d65a63155dc5`.
Evidence and isolated catalog: `output/spot-healing-2026-10-10/`, port 9359.

Native Skin cleanup exported the first portrait at 2048x3072 with 414 operations
(408 compact repairs), and the second at 1333x2000 with 199 operations (193 compact
repairs). Their recipes are respectively
`073df633ead240fec37eb7698b811d3cb696cde3c79e5ca27c6ee1e5466042d2` and
`3a35b8125c7c2cf994a0d188aa37a01ef93e575fcbe85dab8e004aae3633ea93`.
Both exports passed readback with neutral global edits. Body-only Acne only
exported 1333x1050 with zero faces and one frequency-heal operation; recipe
`ae092867723209015e6d422a82788ba4037583da6f455af107517631314b924b`.
Body-only inspection selected 114,417 pixels and changed 17,340 of them; every
unselected pixel remained identical. `actual-source-integrity.json` resolves
the actual catalog source paths and verifies all three original SHA-256 values,
in addition to the export harness's source/copy checks.

Full-resolution native before/after rendering preserved 93,916 eye-core pixels
and 4,941 nostril-opening pixels in the first portrait exactly; the second
preserved 12,140 eye-core and 645 opening pixels. Broad-only native ablation left
every pixel of both previous nose-damage windows unchanged. Requested compact
repairs affected 251 of the first window's 9,353 pixels, maximum seven RGB levels;
its scoped coarse-change maximum is 0.9212%, below the 1% gate. The second window's
720 pixels remained entirely identical. The first skin window is not described
as pixel-identical: the compact repairs remove small measured skin marks there.
The same coarse gate rejects the earlier real damaged second-photo wing at 59.9%,
and rejects a synthetic flattened patch. Its window-specific measurement excludes
repairs outside that window; full-image visual inspection remains a separate gate.
All temporary native diagnostic writes restored the exact saved recipe afterward.

Preview nose-skin selection was 68.1% on the first sample, with 3,634 changed pixels;
the second angled sample was 33.5%, with 61 changes. These landmark samples are not
proof of complete nose coverage. Body selections chose 88,192 and 76,038 pixels,
with 27,948 and 31,870 changes respectively and no overlap inside detected faces.

Native manual brush save/remove passed with the larger stack and restored its
recipe. Preview-error injection cleared stale images, blocked edits and recovered
through reload/undo/redo. Grayscale/high/low/color controls passed in the desktop,
including both comparison halves and unfiltered saved coverage; view changes left
native RGB and recipes unchanged. After normal window closure and restart (process
10112 to 12892), all three recipe hashes and rendered RGB hashes matched exactly.
The tested app is left open on the first portrait in Original color.

Final JPEGs: `validated-first/export/gallery/professional-retouch-verification_0001.jpg`,
`validated-second/export/gallery/second-portrait-verification_0001.jpg`, and
`validated-body/export/gallery/body-only-verification_0001.jpg`. Whole images and
full-resolution nose, forehead, cheek, chin, neck and arm crops were inspected.
The second photo's flattened wing defect is absent. The first photo still shows
clustered nose/cheek blemishes and some uneven texture transitions, so this is
improved detection/coverage with verified protected features, not a professional
quality pass or a zero-blemish result. Pores remain visible. No 100% perfection or
Retouch4me-equivalence claim is supported. Real legs were unavailable; disconnected
hand/leg tests establish processing behavior, not universal real-photo accuracy.

## Cluster refinement and curved lighting - 2026-10-10

Validated `C:\Users\amrit\aura-eye-protection`, branch
`fix/eye-protection-and-retouch-coverage`, planner `measured-features-v16`.
The tested custom-protocol executable SHA256 is
`3af36cb91ce80e67be4f2ccf560d4d0a7f7ccac6f002e03629ce35bddefb4b55`.
Evidence is in `output/skin-clusters-2026-10-10/validation-report.json`.

Actual native input capture/replay confirmed that selected elongated red marks
could fail circular clearance or affine lighting checks. Implemented measured
elliptical repairs, conservative paired nostril measurements, additional
red-supported score tiers, versioned clean-ring and curved RGB fitting, healthy
surrounding-skin donor references and one protected residual pass. Keep successful
affine fits unchanged; exclude previous repair footprints from residual centers.
Temporary input-capture code was removed after diagnosis. Saved old recipes and
rendered RGB remained identical across the updated renderer.

Current checks pass: 142 app library tests, 67 recipe library tests, 177 render
library tests, 12 patch-heal and 10 skin-finish integrations, and all 27 affected
workspace tests. Strict Clippy, formatting, diff checks, frontend production build
and the native build pass. The existing linker manifest and Vite chunk-size
warnings remain. Curved-light and contaminated-ring negative controls distinguish
incorrect flat/tinted repairs from reconstructed light. Manual strength adjustment
retains the saved repair flags.

Native retouch-only exports completed at original dimensions, with zero global
exposure change and verified delivery manifests:

- First: 2048x3072, 470 operations including 464 spot repairs; cleanup 19.82 seconds,
  export 42.12 seconds. Recipe hash
  `980e6463a253c1e13184b44773fd86885a736e562f747afd3de2ad63f52397cf`.
- Second: 1333x2000, 203 operations including 197 spot repairs; cleanup 16.94
  seconds, export 15.81 seconds. Recipe hash
  `c4c937c6f10434c7f860468927ac7c9161f8b1ccda8df9d6d0a9e278cce981c9`.
- Body-only: 1333x1050, no detected face, one frequency-heal operation; cleanup
  11.78 seconds, export 8.55 seconds. Recipe hash
  `677ba07cf138b4d641b9fbe6caec7b13322b038bf7dd9e58b00284933ea09b5b`.

Full-resolution first-photo eye cores (93,916 pixels) and independently inspected
opening rectangles (2,490 pixels) have zero RGB changes. The named shadow window
contains 152 compact-repair changes, at most seven RGB levels; broad-only rendering
is identical there, and coarse absolute RGB change is 0.5815%, below 1%. Second
eye cores (12,140 pixels), opening cores (645 pixels) and its entire 720-pixel
wing regression rectangle have zero changes. These checks certify the named
regions, not every possible anatomical detail or professional quality.

Body-only coverage selects 114,417 pixels, changes 17,340 selected pixels and leaves
every unselected pixel unchanged. All three actual catalog source hashes still
match their recorded originals. The second external E-drive path was unavailable;
validation used the unchanged original catalog input copy.

Native manual save/remove, coverage controls, preview failure/reload/undo/redo and
all grayscale comparison views pass. Views leave recipes/native pixels unchanged
and keep coverage unfiltered. An initial window-close attempt did not exit and
is excluded from restart evidence. Actual normal Alt+F4 closure and restart
(process 1016 to 18864) preserved every recipe and rendered RGB hash. AURA is left
open on the first portrait in Original color.

Reviewed whole images plus nose, cheek, chin, forehead, neck and arm details.
The second portrait retains natural nose shading without the earlier flattened
wing. The heavy-acne portrait improves but still has clustered nose/cheek marks
and uneven texture transitions at detail zoom. `quality_target_met` remains false:
this is not 100% blemish removal, complete pore elimination or a Retouch4me
equivalence result. Real legs remain untested. Final JPEGs are under
`reviewed-first/export/gallery`, `reviewed-second/export/gallery` and
`reviewed-body/export/gallery` in the evidence directory.

## Confined redness repair and complete collection workflow - 2026-10-11

Validated the recovered `C:\Users\amrit\aura-eye-protection` checkout on
`fix/eye-protection-and-retouch-coverage`, base commit `a5c4fe0`, planner
`sample-consensus-v3+measured-features-v19`. The D-drive checkout still has missing
source files; it is not the source/build validated here. Native executable SHA256:
`54d3985dd433435a15025d99eb0e1afc5b1c6dd4b6361bc4eff1b6ac17932b3b`.
Evidence: `output/skin-clusters-2026-10-10/validation-report-v19.json` and
`output/evoto-workflow-2026-10-11/validation-report.json`.

The new planner can confine pigment correction to a strongly supported inflamed
lesion when the original light cannot safely support texture replacement. The
native color operation retains original luminance and detail; it does not replace
the whole nose. Keep complete skin clearance, healthy distributed ring support,
clean donors and precise feature guards. Supported texture repairs retain
priority. Unsupported candidates remain explicit skips. The captured native
fixture retains its required nose-context and eight-donor chin repairs: 473
texture repairs plus one localized pigment correction. A renderer regression
checks preserved brightness across a shadow step and untouched surroundings.

Reopening saved cleanup preferences previously truncated a 900-spot budget to
220 in the interface. Restore the same 900 bound as the backend. Fresh checks:
147 app-library tests, the opt-in exact native-photo regression, 37 affected
interface tests, strict Clippy, formatting, frontend build, native build and diff
checks pass. The existing linker-manifest and frontend chunk-size warnings remain.

Fresh retouch-only native exports all preserve originals and original dimensions:

- Heavy-acne portrait: 2048x3072, 480 operations: 473 texture repairs, one pigment
  correction and six broad face/body cleanup operations. Retouch 23.03 seconds;
  verified export 45.63 seconds.
- Second portrait: 1333x2000, 204 operations including 198 texture repairs.
  Retouch 20.91 seconds; verified export 18.09 seconds.
- Body-only crop: 1333x1050, no face, one saved body-skin frequency heal.
  Retouch 12.00 seconds; verified export 9.56 seconds. Coverage selects 114,417
  pixels, changes 17,340 selected pixels and leaves every unselected pixel intact.

The collection workflow follows the public separation of blemish removal,
skin-tone work, texture retention, per-photo review and batch delivery described
by [Evoto portrait retouching](https://www.evoto.ai/features/portrait-retouching)
and [Evoto batch editing](https://www.evoto.ai/features/batch-edits), checked
2026-10-11. No Evoto software, private model or cloud photo upload is involved.
AURA already measures each photograph's own light, color and scene; this run
exercises that path with independently saved face/body cleanup preferences.

All 11 native collection checks pass: import, workspace navigation, per-photo
cleanup, collection editing, reopening the 900-spot controls, independent manual
exposure, repeated collection editing that preserves it, undo/redo, comparison,
verified export/readback of every photo and advanced panels. Final exposures are
0.30 EV (deliberate manual-protection test), 0.102011 EV and 0 EV respectively;
operation counts are 480, 204 and 1. All three JPEGs decode at original dimensions
and all actual catalog source hashes remain unchanged. No page errors occurred.
Exports are in `output/evoto-workflow-2026-10-11/export/gallery`.

Full-resolution checks compare retouch against an otherwise identically graded
baseline, so deliberate global grading is not confused with feature damage.
For both cleanup-only and complete edits, first-photo eye cores (93,916 pixels)
and both independently inspected anatomical opening rectangles (2,490 pixels)
remain RGB-identical. Changes in its named nose-shadow window are confined to
saved compact repairs; broad-only rendering is identical. Maximum coarse
absolute color change is 0.6291% for cleanup-only and 0.6622% for full editing,
under the unchanged 1% limit. Second-photo eye cores (12,140 pixels), opening
cores (645 pixels) and the 720-pixel nose-wing regression rectangle are unchanged
in both paths. Full-edit body coverage changes 19,231 of 114,602 selected pixels
and preserves all unselected pixels. These are checks of named regions, not
certification of every anatomical pixel.

Native manual save/remove, coverage inspection, failed-preview clearing,
edit-blocking/retry, undo/redo and grayscale/high/low contrast views pass. Filters
leave saved recipes/native pixels unchanged and coverage unfiltered. A keyboard
close attempt did not exit and is excluded from restart evidence. A normal
WM_CLOSE event closed the owned window; actual processes changed from 7436 to
18800. All six saved recipes and preview RGB hashes match exactly after restart:
three cleanup-only tests plus every photo in the full-edit collection. Previously
saved v18 recipes also rendered identically after upgrading the executable.

Whole exports and full-resolution nose, forehead, cheek, chin, neck and arm crops
were visually reviewed. The second nose-wing regression remains absent. The
heavy-acne portrait is substantially cleaner but still has small nose, forehead,
cheek and chin marks, and visible pores. `quality_target_met` remains false.
No 100% removal, professional-quality equivalence, Evoto-equivalence or
Retouch4me-equivalence claim is supported. Real legs and varied group portraits
were not exercised. The tested app is left open on the full-edit collection's
first photo in original color, with split comparison and coverage off.


## Current studio integration and collection cleanup - 2026-10-11

The working checkout is `C:\Users\amrit\aura-eye-protection`, branch
`fix/eye-protection-and-retouch-coverage`. Snapshot `9793c6e` was committed,
pushed and published as [PR #52](https://github.com/amritgyawali/aura-photos-editor/pull/52)
before continuing implementation. The subsequent integration merges main
`4276486f7c3c1ed51b1f6897f9635f56a58f7f2c`, including its current native
advanced-retouch, blemish-brush, local-mask and full-quality preview workspace.

Collection cleanup synchronizes settings, with fresh independent native analysis
for each photo and an explicit stop-after-current-photo control (ADR-0111).
The automatic planner remains frequency healing followed by checked local repairs;
manual Acne Clear remains available. Current planner identity is
`sample-consensus-v6+measured-features-v20`. Legacy saved operations retain their
rendering semantics. Measured under-eye corrections always protect lids and lashes,
even with optional broad detail guards switched off.

The initial integrated native build (`4cf71387c6b9499d4f83c195de435645d724acd3e264840606426f34a4a20c0e`)
launched and passed six-workspace navigation, import, individual cleanup, initial
collection editing and manual-exposure preservation. Its repeat-edit test did not
complete: the app reported a fatal 8 MiB allocation failure while development
tools were also running. This run is a failed stability result, not export or
quality validation. Inspection found strong references in the latest-preview-pair
list outside the checkpoint cache budget. ADR-0112 bounds these separately and
reduces finished-preview retention without changing resolution or pixels. A new
executable and native retest are required for acceptance.

Feature scope and remaining commercial-workflow gaps are documented in
[Evoto-style workflows](evoto-style-workflows.md). This integration does not
establish 100% removal, complete commercial-tool parity or Evoto-equivalent quality.

### Final retest after the memory fix

The native runtime executable SHA-256 is
`20bf4544c174c97ea2f7b3b1b470aeb1e02260229dd36321c7ed22865b1a321d`.
Only a Rust documentation backtick correction followed this build; no runtime
code changed. The desktop was launched in the isolated catalog and WebView profile
under `output/spot-healing-2026-10-10`. Trial status permitted exports without an
activation bypass. Final evidence is `output/evoto-workflow-2026-10-11/final-validation.json`.

- All 443 affected Rust library tests and 732 interface tests passed. Strict
  workspace/all-target Clippy, formatting, banned-pattern checking, 83 locked
  contracts, signed model/card checks (26 manifests, 58 files), frontend build
  and native desktop build passed locally. The captured real-photo spot regression
  passed with its absolute input path, including healthy-context and multiple
  clean-texture-donor checks.
- `verify-desktop-workflows.py --skin-cleanup` passed all eleven checks in
  **Desktop workflows 022011 verification**: navigation, three-photo import,
  individual cleanup, collection editing, restored settings, manual exposure,
  repeat editing, undo/redo, comparison, verified export, and advanced panels.
  Export readback dimensions were 2048×3072, 1333×2000 and 1333×1050, and all three
  source copies retained their original hashes.
- `verify-collection-retouch.py` exercised the new cleanup-sync button. Stopping
  after the current photo saved one and left the other two recipes unchanged.
  A complete run retouched all three, with zero failures/skips and each original
  grade preserved. Saved counts were 480, 204 and 1 operations, each with planner
  identity `sample-consensus-v6+measured-features-v20`.
- `verify-full-retouch-integrity.py` passed on both collection portraits at full
  resolution against otherwise identically graded baselines. First-portrait eye
  regions (93,916 pixels) and independently inspected nostril openings (2,490
  pixels) had zero RGB change. Its named shadow region changed only within saved
  compact repairs: 162 of 9,353 pixels, maximum channel delta 12, maximum coarse
  relative change 0.6622%, and no broad-operation change. Second-portrait eyes
  (12,140 pixels), nostril cores (645 pixels) and the prior nose-wing regression
  region (720 pixels) all had zero RGB change. Diagnostic recipe changes were
  reversed and the exact hashes restored.
- `verify-body-only-retouch.py` confirmed 114,602 selected pixels, 20,384 changed
  selected pixels, no required face detection, and zero changes to unselected
  pixels. This exercises the visible neck and arm skin in the supplied crop;
  it is not a real-leg or varied-body dataset.
- `verify-advanced-retouch-ui.py` ran all eighteen reported stages on the second
  portrait and exported a verified 1333×2000 JPEG. Internal checks passed:
  texture retention 1.014211, skin shift 0.001655, mirror balance 1.062878 and no
  measured clipping. These averaged checks do not establish commercial-quality
  equivalence. The exported full photo and face detail were inspected visually.
- Manual brush save/removal, operation coverage, read-only zoom/opacity, split
  comparison, all four diagnostic views and failed-preview recovery passed.
  Grayscale used `grayscale(1)`, high contrast additionally `contrast(4)`, and low
  contrast `contrast(0.25)` on both comparison sides; coverage stayed unfiltered,
  and recipes and native pixel hashes remained unchanged.
- The dedicated blemish brush saved three actual UI dabs as one Acne Clear
  operation. `verify-blemish-brush-integrity.py` measured 2,143 changed pixels,
  maximum RGB channel delta 26, and zero changes outside the painted footprints.
  Undo/redo restored its exact recipe. Its full-resolution JPEG export was verified
  and the original hash retained. The inspected dabs still leave some target marks.
- A normal WM_CLOSE restart changed owned process 18780 to 22256.
  `verify-retouch-restart.py --verify` matched all six saved recipes and their
  full-resolution RGB hashes across the restart, including the new three-photo
  collection, the manual blemish brush and the advanced portrait result. The harness
  now waits for the debug endpoint and native bridge during startup.

During the retest the sampled native peak working set was about 1.3 GiB and peak
private memory about 1.9 GiB. The initial allocation failure remains recorded above;
the successful retest ran without a compiler alongside it. These measurements are
not a universal memory/stability guarantee. Optional model/provider flows and the
complete cross-platform CI matrix were not run in this local native audit.

The final visual verdict remains **quality_target_met = false**. Whole-photo and
face-detail review shows substantial cleanup with preserved tested eye/nose
structure, but residual nose, forehead, cheek and chin marks and visible pores
remain in the heavy-acne portrait. Even the additional brush dabs did not remove
every target mark. Dedicated arbitrary hair-color selection, background replacement
with transparency/contact shadows, face/body reshaping and the entire commercial
tool catalogue remain gaps; general sliders are not represented as those features.
