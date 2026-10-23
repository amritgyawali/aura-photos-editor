# AURA reference colour matching — 2026-10-22

Follow-up: [actual exported-colour verification](AURA-exported-colour-verification-2026-10-22.md)
adds adjusted matching strengths, JPEG/PNG comparisons and a full-resolution RAW delivery.

AURA successfully imported two original-resolution Internet references, applied each to two supplied-photo previews, saved and exported the edits, and matched one native CR3 without modifying its source. The main **Match a look** page now exposes single-image matching beside the shared editor preview.

**Quality verdict: approximate appearance matching, not exact cloning of any grade.** In the controlled known-grade experiment, 15/18 matches moved closer to the known edit; 9/12 different-photo transfers improved and 3 became farther from that edit. None of the 18 was pixel exact. Strong split-toning and monochrome reconstruction remain limitations. Exact Evoto parity and perfection have not been demonstrated.

## Repair and scope

- `App.tsx`: bring the existing single-image reference controls and filmstrip into **Match a look**; keep folder/Instagram style learning available in a separate disclosure.
- `PhotoStudio.tsx`: reuse the mounted editor, preview and history; show reference tools in this workspace, close active drawing/picker modes on entry and preserve the previous editing panel for return.
- A regression verifies that switching workspace does not create another preview renderer or write an edit, and restores the previous editing panel.
- The native matching algorithm and its skin/highlight guards were retained. The Windows picker automation was corrected to send edit notifications and Enter, then verify the actual selected path; this was a test-helper fix, not an established app-picker defect.

## High-quality downloaded references

| Reference/source | Photographer | Original dimensions | Download bytes |
|---|---|---:|---:|
| [warm-cinematic](https://www.pexels.com/photo/cinematic-portrait-of-woman-in-warm-lighting-36672685/) | Elis S | 4000 × 6000 | 2,543,073 |
| [vibrant-teal-orange](https://www.pexels.com/photo/dramatic-portrait-of-a-woman-in-vibrant-fashion-29653811/) | BULE | 3778 × 4723 | 1,887,966 |

Downloaded from the original Pexels image URLs, verified as JPEGs with original dimensions and no upscaling. Credits, URLs, [Pexels licence](https://www.pexels.com/license/) and SHA-256 values are retained in [the machine report](AURA-reference-colour-match-2026-10-22.json) and `output/aura-internet-reference-2026-10-22/download-manifest.json`. These are finished reference looks; their original adjustment recipes are unknown. The warm reference includes strong orange scene lighting.

## Actual desktop workflow

15/15 recorded GUI steps passed, with no JavaScript page errors. The GUI targets are 1024-pixel proxies made from the supplied `989A2698.CR3` and `989A2865.CR3` before previews; this is distinct from the native RAW check below. Import preserved each reference's original dimensions; analysis internally samples the reference. Four verified JPEGs were exported through the app's Export page, with delivery manifests and an independent image decode. Analysis does not change history; matching retains geometry, retouch and masks; Undo/Redo restores exact recipes and amount zero restores exact baseline pixels. Reload retains recipes and strengths.

- PASS: Create collection, import targets and save initial corrections.
- PASS: Single-image matching is directly available in Match a look.
- PASS: Cancel native upload without importing another file.
- PASS: Upload original 4000 by 6000 reference through the native Windows picker.
- PASS: Upload several references through the native Windows picker.
- PASS: Match vibrant-teal-orange onto target-0.jpg, Undo/Redo and remove/restore.
- PASS: Match vibrant-teal-orange onto target-1.jpg, Undo/Redo and remove/restore.
- PASS: Verified JPEG export: vibrant-teal-orange.
- PASS: Match warm-cinematic onto target-0.jpg, Undo/Redo and remove/restore.
- PASS: Match warm-cinematic onto target-1.jpg, Undo/Redo and remove/restore.
- PASS: Verified JPEG export: warm-cinematic.
- PASS: Saved recipes, references and matching strengths survive UI reload.
- PASS: All four main strengths and twelve fine controls save or explicitly refuse unsafe combinations.
- PASS: A later manual edit invalidates the previous analysis.
- PASS: Collection matching measures both photos and preserves a manual exposure.

20 independent control checks covered all four main strengths at 0%/200% and all twelve fine strengths at 0%, with the other strengths reset to defaults. 16 saved and 4 were explicitly refused by quality guards; each refusal left recipe and history unchanged. Acceptance proves persistence and validation, not that every fitted component has a nonzero visual effect.

| Reference | Target | Internal appearance distance | Accepted amount |
|---|---|---:|---:|
| vibrant-teal-orange.jpg | target-0.jpg | 22.082 → 20.837 | 1.00 |
| vibrant-teal-orange.jpg | target-1.jpg | 24.574 → 23.449 | 1.00 |
| warm-cinematic.jpg | target-0.jpg | 22.031 → 18.176 | 1.00 |
| warm-cinematic.jpg | target-1.jpg | 20.377 → 18.869 | 1.00 |

Internal distance measures the app's regional appearance descriptor; it is **not a pixel error, quality percentage or proof of equal colour**. Different scenes have different subjects, materials and lighting. No pixel-similarity score is assigned to these unrelated Internet and target images.

Offline gallery: [`output/aura-internet-reference-2026-10-22/index.html`](../../output/aura-internet-reference-2026-10-22/index.html). Original downloads, before/after PNGs, verified JPEG deliveries, recipes, screenshots and a saved two-photo review catalogue remain there. Primary run: `output/aura-internet-reference-2026-10-22/ui-qualified`. `catalog/qualified.sqlite` was backed up before control and manual-edit stress checks, retaining the four-match workflow's final warm look for review.

## Native RAW check

Explicitly ran the opt-in test `native_raw_reference_match_does_not_touch_originals` with `H:\AURA EDIT\989A2865.CR3` and the downloaded 4000 × 6000 warm reference: **1 passed, 0 failed**. Amount 0.50; native-resolution validation true; internal descriptor 20.367283 → 19.539862. Source SHA-256 before and after: `b0c804647a3b066772ad6fb8f056b0c73894d513a3b7265658ca33a66a28e5ae`. The reference hash also matches the download manifest. This validates this RAW/reference pair, not every camera format or every possible look.

## Controlled known-grade quality experiment

Six edits were generated with AURA's own renderer: warm WB/tint, cool WB/contrast, matte curve/desaturation, teal/orange split tone, punchy contrast/saturation, monochrome. Each reference was uploaded and matched to its source and two different supplied-photo proxies. For each target the oracle is the same known adjustment applied directly to that target. Aligned mean CIE76 is a reproducible difference measure, not a perfection score. An adaptive matcher may choose a different edit; this experiment tests the specific request to clone a known treatment. Warm/cool fixtures intentionally exceed conservative fitting limits.

| Grade | Target | Relationship | Before mean CIE76 | Matched mean CIE76 | Result |
|---|---|---|---:|---:|---|
| warm | 0.png | same source | 10.189 | 6.048 | closer |
| warm | 1.png | new photo | 9.094 | 7.903 | closer |
| warm | 2.png | new photo | 8.392 | 5.795 | closer |
| cool | 0.png | same source | 11.988 | 7.615 | closer |
| cool | 1.png | new photo | 10.742 | 5.567 | closer |
| cool | 2.png | new photo | 10.010 | 6.798 | closer |
| matte | 0.png | same source | 5.976 | 2.813 | closer |
| matte | 1.png | new photo | 8.076 | 2.823 | closer |
| matte | 2.png | new photo | 7.370 | 5.658 | closer |
| split-tone | 0.png | same source | 2.447 | 2.294 | closer |
| split-tone | 1.png | new photo | 3.268 | 5.629 | farther |
| split-tone | 2.png | new photo | 2.978 | 4.026 | farther |
| punchy | 0.png | same source | 7.754 | 5.019 | closer |
| punchy | 1.png | new photo | 10.468 | 11.782 | farther |
| punchy | 2.png | new photo | 8.320 | 5.635 | closer |
| monochrome | 0.png | same source | 30.516 | 26.933 | closer |
| monochrome | 1.png | new photo | 39.746 | 24.945 | closer |
| monochrome | 2.png | new photo | 31.130 | 27.486 | closer |

All 18 completed functional checks at default amount 1.0, including read-only analysis, native validation, exact Undo/Redo and exact zero removal. Quality and functionality are reported separately. The split-tone cases on both new photos and the punchy case on target 1 were farther from the known edit; monochrome remained strongly different despite some error reduction. See [all cases as CSV](AURA-reference-colour-match-2026-10-22-cases.csv) and the [controlled gallery](../../output/aura-reference-grades-retry-2026-10-22/index.html).

## Input support and limits

JPEG, opaque PNG and TIFF imports were exercised. An embedded sRGB ICC profile was converted successfully. WebP, translucent PNG and corrupt JPEG were refused with explicit errors and no reference added. This is **not support for any image format**. The decoder also specifies bounds (256 MB, 64 megapixels, 16000 pixels per side) and ICC/colour-space checks; those bounds were inspected, not all exercised in this run.

The fitter estimates bounded tone, HSL and warmth/tint adjustments. Its conservative style delta limits include approximately ±800 K temperature, ±0.67 EV exposure and bounded colour/tone adjustments. It does not recover an arbitrary original 3D LUT, a complete editing recipe or exact original camera white balance from a finished image. It cannot reconstruct scene lighting from another photograph. Skin/highlight safety may refuse a strength combination. Matching remains useful for a starting look, with visual review and manual finishing required for stronger grades.

## Validation and provenance

- Frontend: **1052 tests passed in 111 files**, including the new workspace/preview regression.
- Frontend production build and native desktop build passed. Existing linker warning about multiple non-default manifests remains.
- Current native reference integration suite: **5 passed, 0 failed, 1 opt-in RAW test skipped** in that invocation. The RAW test was explicitly run separately above and passed. These tests cover regional strengths/masks, capture/library metadata/reopen, different dimensions/manual edits, framing and finishing history.
- Current tested desktop SHA-256: `d77e7351c4e736681ab3f1afb79a343588afd0a94a55f031f405b4b723b0f2c5`.
- The controlled matrix ran before the UI entrypoint repair on desktop `e6d5618879b90b6b181805bc143c15c469e8ffa0711fcda5f0f7e7620be29d9e`; native fitting/backend code did not change between the matrix and current build.
- Logs and command metadata: `.work-checks/aura-internet-reference-2026-10-22/`; controlled matrix: `.work-checks/aura-reference-grades-2026-10-22/`.
- Retained excluded attempts: initial matrix assumed independent strengths must always pass guards; Windows picker helper initially selected the previous file; immutable Tauri-invoke instrumentation did not observe calls; expanded helper queried collapsed fine controls using visible-only locators and omitted reselecting the collection after UI reload. These attempts were corrected and rerun, and are not counted as product passes or product defects. Further excluded attempts, if present, remain in the same logs.
- The earlier [comprehensive audit](AURA-comprehensive-test-2026-10-22.md) retains its historical test totals. This supplemental run does not relabel that audit as a current full native-suite rerun.
