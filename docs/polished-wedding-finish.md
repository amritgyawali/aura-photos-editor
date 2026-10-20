# Polished wedding finish

New photographs use **Polished wedding** in Retouch > Automatic retouch. The
desktop and native automation read the same bundled
[`polishedWedding.json`](../ui/src/config/polishedWedding.json), including offline
automation. Existing saved choices and manual operations remain owned by the
photographer. Changing this default does not re-edit an existing collection.

This is an AURA implementation of a portrait workflow. It does not use Evoto's
proprietary code or weights and has not established identical Evoto output.
Evoto documents separate [blemish/under-eye controls](https://support.evoto.ai/portrait-retouching-module-blemish-removal/)
and [texture, tone and dodge/burn controls](https://support.evoto.ai/portrait-retouching-module-skin-retouching/).
Those are workflow references, not an algorithm specification or quality certificate.

## Processing strategy

1. Decode the original with AURA's native RAW path; measure light, colour and
   noise before applying a selected look. Skin-only retouch preserves global
   colour and crop. Every face is measured against its own skin.
2. Detect face landmarks and segmented face/body skin. Save selections with
   ordinary editable operations. Eyes, lashes, lips, nostril structure and
   facial hair retain the existing dedicated exclusions. Review the mask when
   a face is small, covered, oblique or partly outside the frame.
3. Repair supported compact spots before evening texture. This finish includes
   diffuse dark marks and freckles; the independent **Keep moles** control stays
   on. The mark classifier is a heuristic, not an identity or medical diagnosis.
   **Protect bindi and forehead pigment** is enabled: a rotation-aware forehead
   search finds strongly red/magenta decorations as separate connected regions.
   Protection follows distance from the actual pigment pixels, including holes
   and concave shapes, instead of excluding each region's bounding rectangle.
   Face-cropped guards use up to 512 cells on their long edge, so interpolation
   margins stay small around a bindi. The guard excludes pigment from
   every automatic face operation before residual repair is predicted. This was
   added after real-photo review caught a removed bindi. It is a conservative
   colour/position rule; other decorations, makeup and unusual marks still need
   visual review. Turning it off restores ordinary blemish treatment there.
4. Apply measured smoothing, tone, shine and under-eye correction. Under-eye
   correction follows that person's own cheek and keeps its existing brightness
   cap and eyelid guard. The preset reduces fine texture more than Natural;
   amounts still adapt to face size, noise, directional light and measured skin.
   Broad frequency healing also rejects candidates on a mixed light/shadow ring,
   even when local warmth looks like redness. Its surrounding-light tolerance is
   narrower; genuinely bright inflamed heads remain a separate accepted case.
   The renderer's versioned cache identity includes this lighting decision.
5. Treat exposed body skin separately, with its own sample. Match body to face
   is limited to 10% of that control rather than forcing identical complexion.
   Body healing remains capped at two passes even when face cleanup requests
   four. **Include dark body spots** is independent of facial dark-spot cleanup
   and off by default, protecting natural neck/body shadows. Visual review of
   the family sample exposed a patchy chin-shadow boundary. The previous
   planner propagated the face's pass count to the body, and a broad face-healing
   selection extended into the neck. Separate body repair limits and a retained
   jaw/neck boundary address both paths.
6. Allow explicit frequency separation and dodge/burn only with one sufficiently
   confident detected face of usable size, unless the photographer disables
   **Advanced techniques only on single-person portraits**. All detected faces
   count, including background faces; Main subject only does not bypass it.
   This is a face-detection eligibility rule, not proof that no other person is
   present. Group faces receive supported local donor repairs, ordinary skin
   smoothing, colour, shine and measured under-eye correction. Broad frequency
   healing is also restricted: without jaw landmarks its face mask can cross
   into a neck shadow. The lower face boundary still excludes the neck; forehead
   coverage follows the semantic skin mask within a limited expanded forehead
   region. Lateral protection retains the detected box as a backstop against
   jewellery spill, with a narrow fade and a finer guard grid to reduce untreated
   skin bands inside its edges.
   Surface finishing uses separate distances for the skin silhouette and facial
   features, retaining wider eye/brow/lip protection without eroding a wide band
   of skin around the face. Body correction uses separate selections.
   The eighteen-stage advanced workflow applies the same portrait restriction;
   background toning remains a scene adjustment.
7. Coordinate overlapping surface passes. The continuous frequency finish uses
   only the unused part of an 85% surface-strength budget after the primary skin
   smoothing pass. This is a bounded planning heuristic, not a measured claim of
   85% texture retention. It does not weaken spot healing or change manual work.
8. Render the saved recipe at full resolution. Inspect before/after face and
   body crops and actual changed areas. Export only after review; verify output
   dimensions, colour profile, bit depth and source integrity.

## Robust skin sampling

The sampled-skin renderer previously averaged every pixel near the selected
source. A few dark hairs, a blemish or a specular reflection could bias both
skin eligibility and colour correction. It now estimates the patch's median
log luminance and chroma, measures their median deviations, and uses bounded
quartic weights to reject outliers before an f64 weighted mean. The thresholds
scale with the source patch; there is no preferred skin colour. Nonfinite and
black samples are excluded. An ambiguous patch falls back to an actual
photographed pixel near the median luminance, rather than inventing a colour.
Both selection preview and final rendering use the same estimator.
Its version is included in the engine identity, so disk preview caches from the
older sampling algorithm cannot stand in for the new rendered result.

## Validation and reproducible review

Focused coverage includes single/group eligibility, main-subject handling,
manual-step preservation, shared surface budgeting, saved UI preferences,
outlier sampling at multiple exposures, HDR samples and invalid input. Existing
renderer tests check selected-region isolation, detail retention, colour and
exposure bounds, healing, preview/export paths and texture protection.

The real-photo integration harness uses isolated catalogs and original RAW
sources. It checks preserved global settings and crop, validated operations,
group technique restrictions, full-resolution visible changes and unchanged
source hashes. It writes matching-frame PPM review images and JSON evidence:

```powershell
$env:AURA_POLISHED_PHOTOS = 'C:\photos\portrait.CR3;C:\photos\group.CR3'
$env:AURA_POLISHED_OUT = 'C:\review\polished-wedding'
cargo test -p aura-app --test polished_wedding_photos -- --ignored --nocapture
```

Tests of execution and pixel boundaries cannot establish that every blemish was
removed or that a result is aesthetically better. Acceptance needs face/body
inspection at 100%, varied complexions and lighting, and a preference check.
The selected preset is a starting point that remains editable.

### Recorded checks: 17 October 2026

The final local build passed 767 UI tests, 229 application library tests and
213 renderer library tests. Four focused renderer integration suites passed
another 56 checks. Two application tests and two renderer tests intended for
manual execution remain ignored in their normal suites.

Three native RAW integration cases passed in isolated catalogs: a solo bride
(`989A2698`, 4480 × 6720), a family group (`989A2886`, 6720 × 4480) and an evening
group (`989A3876`, 6720 × 4480). All original source hashes, global adjustments
and crops were unchanged. Group recipes contained none of the six restricted
frequency-healing, frequency-separation or dodge/burn tools. Matching-frame
face crops and changed-pixel maps were inspected, including the bride's bindi
and the family photo's jaw/neck transitions. The previously affected neck
sample now matches the original pixel exactly.

The evening detector returned nine targets, including two printed portraits.
This remains a known detector limitation: the finish does not establish that
each detected face is a real person, and the review labels them as targets.
Three samples demonstrate these cases; they do not establish universal quality
or removal of every blemish on future photographs.

The rebuilt desktop started successfully, all 98 existing delivery recipes
matched their recorded hashes, and all 23 review-image/recipe links resolved.
The evidence is saved locally in
`.work-checks/polished-wedding-final/Review.html`, `results.json` and
`runtime-validation.json`. The app uses a normal local desktop build; this
check did not install an updated system-wide package or re-export the existing
delivery.

### Recorded checks: 18 October 2026

The contour and lighting revision passed 231 application library tests and 214
renderer library tests, with two manual tests ignored in each suite. Three native
CR3 integration cases passed again using the existing job's saved colour and crop.
The rebuilt desktop produced the same pixels as the isolated solo portrait probe.

All 98 selected photographs were rendered again at original resolution and reviewed
with face/body comparisons and actual changed-pixel overlays. The flagged portrait
was additionally inspected at the forehead mark, hairline and both face edges.
Its 218 strongly coloured bindi pixels changed by at most one 8-bit code value.
Changed-pixel coverage in the measured surrounding skin rose from 31.7% to 97.01%;
this measures coverage, not aesthetic quality. Printed-face false detections were
excluded from this delivery's face operations during review.

## Skin boundaries and spatial colour checks

The v32 planner stores skin masks at up to 512 cells per side (previously 256).
Competing hair, clothing, accessory and background probabilities now constrain
both facial and body skin. The body mask reapplies its semantic, person-region and
subject-colour checks after filtering, so refinement cannot fill rejected areas
back in. Darker skin is assessed against the subject's own measured samples.

Protected portrait and body masks request `refineContour`: at render resolution,
the renderer samples inside/outside colours at the external contour and can only
reduce authored coverage. Enclosed eye/tika holes cannot become outside references;
their original zero coverage stays zero. Ordinary edge refinement also preserves
authored zero pixels. Coarse connected-skin selection feathers inward, and only
connected selected pixels supply the skin-smoothing averages. The renderer's
cache identity includes the boundary algorithm version.
Above four megapixels, at most two masks per frame allocate refinement scratch
space concurrently; each mask still parallelizes its pixel work.

Automatic white balance retains its existing gray-edge, gray-pixel and bright-tone
checks and adds equal votes from up to sixteen spatial tiles. When at least three
tiles have usable bright references, at least 80% must agree with the proposed
illuminant within 0.12 log-chroma distance. Otherwise the global white balance is
left alone with a mixed-light explanation. Sparse references use the existing
conservative checks. This is a guard against harmful corrections, not a guarantee
that any scene has an objectively unique correct colour. Deliberate grading and
complex local lighting still benefit from a reference and visual review.

Validation for this revision passed 592 library tests (application, recipe,
renderer and vision; five manual tests ignored) and 45 renderer integration
tests. Three original-resolution CR3 trials preserved their source hashes and
saved global colour/crop. They isolate the skin changes; the new white-balance
guard was checked with uniform-cast, mixed-light and spatial-dominance fixtures.
The portrait's 218 strong bindi pixels changed by at most one 8-bit code value.
A second 30-megapixel portrait render confirmed identical pixels after limiting
concurrent mask refinement. The rebuilt desktop reopened with all 128 saved
catalog recipes unchanged.

The evening trial still detected two printed faces on a poster. This revision
does not solve real-person versus printed-face detection or establish perfect
selection on every photograph. Review evidence, face/body crops and trial
recipes are in `.work-checks/skin-boundary-v32/Review.html`. These trials are
separate from the existing completed batch delivery.

## Chin and neck anatomy protection (v33)

The reported `989A2698_AURA_FINAL_POLISHED.jpg` had an apparent skin flap below
the chin. Full-resolution ablation identified the lower-chin patch repairs as
the cause: they treated part of the chin shadow as blemishes. The v32 contour
changes alone did not fully remove this artifact.

Automatic portrait masks now preserve a coherent chin/neck brightness boundary
below the mouth. The trace uses the rotated eye/mouth frame, brightness ratios,
a continuity penalty and support across most of the jaw; a single dark spot
cannot establish an anatomical boundary. Uncertain traces use conservative
protection for healing near the estimated jaw. The same guard constrains body
operations so a neck tool cannot bypass the facial protection. Protected coverage
remains zero when masks are rendered at export resolution. Manual edits remain
available.

236 application tests passed, including curved shadows at several brightness
levels, rejection of an isolated dark spot and a rotated portrait. Two native
RAW trials passed with source hashes, saved colour and crop preserved. In the
reported 120-by-30-pixel chin-crease area, the corrected native render differs
from the no-retouch reference by at most one 8-bit code value, compared with 23
in the flagged finish. This is a local preservation check, not a universal
retouch-quality score.

The rebuilt desktop applied a recipe identical in all render fields to the
reviewed trial. Only this photograph's saved recipe changed; the other 127 saved
recipes were preserved. A full-resolution JPEG and 16-bit TIFF were exported
after review and independently decoded, checked for embedded ICC profiles and
hashed. Evidence and comparisons are in
`H:/editing to aura/AURA_Chin_Corrected_2026-10-18/Review.html`.

## Under-eye shading and blend protection (v34)

The reported `989A2699_AURA_FINAL_POLISHED.jpg` showed pale parallel bands below
the eye. Native ablation reproduced them with the under-eye operation alone;
disabling that operation removed them. The old renderer drove each pixel's low
band toward cheek luminance and chroma, flattening the socket's natural shading.
The automatic selection also used two parallel correction ribbons.

The renderer now measures one robust shadow reference and applies a shared,
bounded exposure gain to the original pixels. It corrects 35% of the measured
exposure deficit, capped at 0.32 stops before the amount/coverage blend. Chroma
uses a limited partial adjustment (at most 18% of the log ratio, with each channel
bounded to 0.06 log gain), followed by luminance normalization. Original texture,
colour variation and socket gradients survive. The correction field is smoothed
inward and cannot expand past authored zero coverage or excluded lashes. Already
bright skin is left alone, and the cheek remains an upper luminance bound.

New automatic selections follow a single connected curve beneath the lid, with
strong feathering, lower default strength and partial cast correction. The
existing eye, bindi and chin guards remain in place. Existing saved under-eye
edits also receive the bounded renderer; the render cache identity now includes
`socket-preserving-v1`, so stale previews cannot conceal the change.

Regression coverage includes a graded socket shadow, extreme cheek/eye brightness
differences at four exposure levels, lash/bright-skin exclusions, pore contrast,
partial cast retention and a single-peaked automatic correction profile. This
does not establish perfect dark-circle detection for every lighting condition.

Validation passed 237 application unit tests, 218 renderer unit tests and 43
renderer integration tests. Two original-resolution CR3 trials preserved source
hashes, saved colour and crop. Visual review at 100% confirmed that the reported
parallel bands were removed while socket shading remained. The previous chin
regression also passed: the protected 3,600-pixel crease differed by at most one
8-bit code value from its no-retouch reference. The rebuilt desktop reopened
with all 128 saved recipes unchanged. Native evidence is under
`.work-checks/under-eye-v34`.

The production recipe matched the reviewed trial in every render field. Only
`989A2699` changed; the other 127 saved recipes, including the previous chin fix,
were preserved. AURA exported a 4,301-by-6,451 JPEG and 16-bit TIFF after review.
Both passed native verification and independent decode, ICC and hash checks;
the actual exported JPEG was also inspected at 100%. The corrected copy,
comparisons, retouch audit and verification record are in
`H:/editing to aura/AURA_UnderEye_Corrected_2026-10-18/Review.html`.

## Coherent nose and jaw edge protection (v35)

`989A2812_AURA_FINAL_POLISHED.jpg` exposed another gap in anatomical protection.
Full-resolution ablation isolated nose-wing changes in frequency healing and
chin-boundary changes in broad finishing. Removing spot patches alone did not
remove these effects. The existing jaw trace reduced the chin error, but a
landmark ellipse did not cover the shadow above this person's nostril.

Automatic masks now share an additional measurement of coherent nasal and jaw
edges from the original photograph. A rotated grid uses 64 cells per eye
distance. Log-luminance gradients provide relative contrast rather than a fixed
brightness or complexion threshold. A local structure tensor tests orientation
coherence; connected weak-edge regions must contain strong evidence and have
sufficient spatial extent (0.18 eye distances around a nose wing, 0.45 along the
jaw). This rejects small isolated marks while retaining weaker connected parts
of a real contour. Measurements outside the photograph are excluded.

A distance field creates a protected core and a smooth transition, with a wider
core for healing. It contracts the existing masks and never selects new pixels.
Face repair, broad finishing and body/neck masks share this protection, so one
tool cannot bypass another tool's exclusion. The previous jaw fallback, eye,
under-eye and ceremonial-mark protections remain. New automatic recipes record
`measured-features-v35-coherent-anatomy-edges`; saved older recipes need to be
replanned to receive these additional mask exclusions.

240 application unit tests passed, including curved nasal/jaw contours at three
exposure levels, quarter-turn rotation and isolated small blemishes that must
remain repairable. Full-resolution native review of the reported photograph
showed that its checked 400-pixel nose-wing patch and 340-pixel chin-crease patch
were preserved exactly. The flagged finish changed these patches by up to 19 and
21 8-bit values, respectively. These are local preservation checks, not a
universal quality score or a claim of perfect detection for every photograph.

All three full-resolution CR3 trials (`989A2812`, `989A2698`, `989A2699`) passed
with original hashes, saved global colour and crop preserved. The sampled chin
curve across 260 columns in `989A2812` was also preserved exactly. The prior
3,600-pixel chin regression in `989A2698` remained unchanged, and visual review
confirmed that `989A2699` retained its corrected under-eye blend. The rebuilt
desktop reopened with all 128 saved recipes unchanged. Evidence and native
trial recipes are in `.work-checks/nose-chin-v35`.

The production recipe matched the reviewed trial in every render field. Only
`989A2812` changed; the other 127 saved recipes were preserved. AURA exported a
4,301-by-6,552 JPEG and 16-bit TIFF after review. Both passed native verification
and independent decode, ICC and hash checks. The actual exported nose/chin
crops were also inspected at 100%. The corrected copy, comparison crops,
retouch audit and verification are in
`H:/editing to aura/AURA_Nose_Chin_Corrected_2026-10-18/Review.html`.

## Expression contours and pigment fringes (v36)

Review of `989A2843_AURA_FINAL_POLISHED.jpg` identified pale parallel bands below
both eyes, a flattened nose-wing shadow, a pinched smile fold and a trimmed bindi
edge. Replanning with the bounded under-eye renderer and v35 anatomy masks
removed the eye bands and restored the nasal shadow. Further ablation showed
that broad finishing still hardened the smile crease.

The coherent-edge search now extends through bounded nose-to-mouth corridors
and a lip-border region. Only measured connected contours become exclusions;
the whole corridor is not masked out. Lip components require at least 0.28 eye
distances of spatial extent. Healing and broad finishing share these feathered
exclusions, including in body masks. The planner records
`measured-features-v36-expression-contours`.

Forehead protection also grows strong red/magenta seeds through weaker,
connected pigment pixels for at most two percent of an eye distance (rounded to
analysis cells, capped at six). This includes anti-aliased decoration edges;
weaker colour cannot create a new decoration or bridge distant marks through
ordinary skin. It retains the existing shape-based distance feather rather than
excluding a rectangular patch of forehead. Half an analysis cell of additional
core protection accounts for resampling uncertainty at full resolution.

243 application tests passed. New tests cover expression contours at several
exposures, preserved repair and finish exclusions at 256/1024 resolution in
both face and body masks, retained cheek coverage, and connected pigment fringes
with nearby skin left editable. Original-resolution RAW review, saved recipes
and ablations are under `.work-checks/portrait-2843-v36`; earlier diagnostic
renders are under `.work-checks/portrait-2843-v35`.

The final four original-resolution CR3 trials (`989A2843`, `989A2812`,
`989A2698`, `989A2699`) preserved their source hashes, saved colour and crop.
In the reported photograph, the checked 10,200-pixel eye-band patch,
5,500-pixel nose-wing patch and 149 red/magenta bindi samples matched the
no-retouch reference exactly. The mean RGB change in the 6,800-pixel smile-fold
area fell from 4.31 in the flagged finish to 0.09. Earlier measured chin and
nose regressions remained unchanged. These are local preservation checks,
not a universal retouch-quality score. The rebuilt desktop reopened with all
128 saved recipes unchanged.

The final desktop recipe matched the reviewed trial in every render field.
Only `989A2843` changed; the other 127 saved recipes were preserved. AURA
exported a 4,480-by-6,720 JPEG and 16-bit TIFF after review. Both passed native
verification and independent decode, ICC and hash checks. The exported eye
and expression crops were inspected at 100%. The corrected copy, comparisons,
retouch audit and verification are in
`H:/editing to aura/AURA_2843_Corrected_2026-10-18/Review.html`.

## Matching an Evoto finish

Use [Choose finished reference](paired-reference-matching.md) with a finished
export of the same photograph for bounded tone/colour fitting. It does not infer
Evoto's repair masks or recreate missing texture, expression or reshaping. To
evaluate closer matching, record Evoto's version and settings, align paired
originals/exports, compare each face and body region separately, inspect detail
bands and colour shifts, and validate on different photographs from the same
shoot. Report measured differences; do not label an approximate result exact.
