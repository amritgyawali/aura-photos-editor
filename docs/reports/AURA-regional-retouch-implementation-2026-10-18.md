# AURA: under-eye reference reliability and regional contour verification

Date: 2026-10-18. Continuation of `AURA-retouch-implementation-2026-10-18.md` and the two Evoto audit reports. This is independently implemented AURA code. It does not contain Evoto code or models, and does not establish identical output.

## Problems addressed

The previous contour check counted edges over an entire face and the adjacent neck. A small area of damaged lower-lid detail could be outnumbered by intact edges elsewhere. Its face-level result also did not tell the photographer which feature needed inspection.

The under-eye renderer already limited exposure lift, preserved socket gradients, avoided copying a flat cheek colour, and blended correction inward. However, its cheek-reference estimator rejected samples mainly by luminance. Similarly bright background or clothing could influence the reference. Its final minimum accepted weight was only one effective sample. Such a weak reference should not control an under-eye adjustment.

These changes address those specific shortcomings. They do not replace the coarse segmentation model with dense facial parsing.

## Renderer changes

Code: `crates/aura-render/src/retouch_undereye.rs`.

The reference disk still uses this person's nearby cheek, with the 40th percentile of luminance anchoring the lower tones. The existing weighting excludes dark detail and sheen. New checks add:

1. Finite, nonnegative RGB and positive luminance, with valid samples covering at least 80% of the in-frame disk pixels.
2. At least 12 usable lower-tone samples before colour statistics are computed.
3. Component-wise median chromaticity of those samples. Normalizing RGB by its channel sum makes this comparison independent of a uniform exposure scale.
4. Smoothly decreasing contribution for chromaticity distance from that consensus: full weight through 0.025, zero at 0.07.
5. A final consensus of at least 12 effective samples, at least 20% of valid disk samples, and at least 65% of the luminance-eligible mass. An ambiguous split between incompatible colours or a tiny surviving reference is rejected.
6. A compatibility check between the resulting reference and the measured shadow. Correction confidence stays full through chromaticity distance 0.20 and decreases smoothly to zero at 0.40. These broad pilot limits allow strongly red/purple shadows while rejecting extreme disagreement. They are not a learned skin-colour classifier.

The confidence scales correction strength; it does not replace the original pixels with the reference. Existing bounds remain: maximum exposure lift 0.32 stops, partial chroma adjustment, preserved socket gradients and pores, inward blending, zero-selection protection, and no correction without a reliable reference. The renderer version changes to `socket-preserving-v2-reference-consensus`, which is included in the render-cache identity.

A regression test initially exposed over-rejection of a deep-red crease. The compatibility interval was corrected and the original crease test retained. The test was not weakened. Reference ambiguity, minority colour contamination, insufficient finite support, exposure scaling, sheen, pores, lashes, and socket gradients are covered by renderer tests.

## Regional measurement algorithm

Code: `crates/aura-app/src/advanced_retouch/quality.rs` and `quality/regions.rs`.

Each face now has five additional inspection zones: image-left eye/under-eye, image-right eye/under-eye, nose, mouth, and chin/adjacent neck. Eye centres define an axis; its perpendicular points toward the mouth. The zones therefore rotate with the landmarks instead of remaining horizontal rectangles. Their sizes scale with interocular distance. Nose/mouth ordering and face confidence are checked before generating measurements.

These are approximate ellipses for inspection, not anatomical segmentation masks. They are never used to paint pixels. Image-left/right identifies screen position, not the person's anatomical left/right. Chin/neck placement is inferred from the mouth and eye geometry and can be imperfect for strong pose or occlusion.

The existing RGB-difference measurement is accumulated independently in every zone containing a sampled pair's midpoint. Each records sampled pairs, new seam pairs, source-edge pairs, weakened-edge pairs, measurement availability, and review status. Constant additive colour offsets do not introduce pair differences by themselves.

The existing per-pair criteria remain:

- New seam: original normalized mean absolute RGB difference below 0.025, final difference above 0.06, and change in the RGB difference above 0.04.
- Strong source edge: original difference above 0.05. It counts as weakened when the final difference falls below 45% of the original and difference departure exceeds 0.03.

Local measurements require at least 64 sampled pairs. A region requests review for at least `max(8, ceil(interocular_distance * 0.05))` new seam pairs, or at least 12 source-edge pairs with at least 8 weakened and a weakened fraction above 25%. These are pilot review thresholds, not calibrated defect probabilities. The original whole-face measurement remains in addition to the local checks.

A synthetic regression creates a damaged under-eye stripe and many intact edges elsewhere. The global weakened-edge fraction remains below its review threshold, while the affected eye region requests review. Additional tests cover rotated geometry, missing/duplicate regions, invalid confidence, unchanged images, and rejection of a proposed correction that worsens a local region.

## Workflow and interface integration

The one-click portrait preflight is now `portrait-boundary-preflight-v2-regions`. A complete report requires all five local measurements for every detected face, alongside face-level measurements and finite clipping values. Missing regional coverage is incomplete; a regional warning requests review and withholds that photo from automatic delivery. The existing recipe-hash check at export remains in force.

Advanced retouch reports are now `auto-advanced-v3-regional-qc`. A proposed quality-control correction must preserve regional measurements and avoid worsening regional seam/edge counts, in addition to the previous texture, colour, clipping and whole-face checks. A whole-face improvement cannot compensate for a newly damaged feature.

Saved reports without regional measurements remain readable, but no longer display a complete QC pass. The UI checks integer/nonnegative counts, count relationships, explicit status booleans, unique region coverage and face indices. It also refuses a pass if a nested region requests review even when the enclosing face flag is false.

The advanced retouch panel now includes a facial-region table with the face number, area, result, new seam count and weakened/source edge counts. It explains the measurement's limits. The stage notes and one-click preflight reports also identify specific regions.

## Validation and artifacts

Validation completed. The native test uses isolated catalogs, checks source hashes before/after, and preserves saved global colour and crop. It writes review artifacts without replacing originals or prior deliveries.

| Check | Result |
|---|---|
| App library | 252 passed, 2 intentionally ignored. |
| Renderer library | 224 passed, 2 intentionally ignored. |
| One-click delivery integration | 1 passed, including failed-edit withholding and cancellation. |
| Sampled skin renderer integration | 11 passed. |
| Advanced retouch UI | 4 passed, including incomplete/duplicate/missing regions and a nested review warning. |
| Native RAW integration | 1 passed across four originals; 233.59 seconds including full and screen renders. This was concurrent with a desktop build, not an isolated benchmark. |
| Focused Clippy | Exit 0 for `cargo clippy --locked --no-deps -p aura-app -p aura-render --lib -j 2`. Existing function-length, jaw-clamp and panic-documentation warnings remain. |
| Frontend production build | Passed TypeScript and Vite. Existing bundle-size warning remains. |
| Desktop build | Completed successfully; existing MinGW multiple-manifest linker warning remains. |
| Python review helper and whitespace | Syntax check and `git diff --check` passed. |

The combined Rust command was `cargo test --locked -p aura-app -p aura-render --lib --test one_click --test sampled_skin -j 2`: 488 passed, zero failed. Together with the four UI tests and the native integration test, 493 tests passed in these final runs. The earlier failed crease run is retained in the logs; it is superseded by the successful corrected run. Dependency-wide Clippy was not rerun; the previous report records unrelated RAW lint failures, so this is not a claim that the entire workspace is lint-clean.

Logs: `.work-checks/region-reference-final-tests.log`, `region-reference-final-clippy.log`, `region-reference-native.log`, and `region-reference-desktop-build.log`. UI tests and build completed in the tool session. Native artifacts are in `.work-checks/region-reference-native`.

| Original | Native dimensions | QC dimensions | Native / QC new seam pairs | Regions measured per scale | Local review warnings |
|---|---|---|---|---|---|
| 989A2698 | 4480 x 6720 | 1066 x 1600 | 5 / 0 | 5 | None |
| 989A2699 | 4480 x 6720 | 1066 x 1600 | 13 / 5 | 5 | None |
| 989A2812 | 4480 x 6720 | 1066 x 1600 | 22 / 0 | 5 | None |
| 989A2843 | 4480 x 6720 | 1066 x 1600 | 17 / 0 | 5 | None |

All four saved reports have complete regional measurements at both scales. The seam counts in this table are whole-face counts; individual region counts and weakened-edge counts are in each `*-regional-quality.json` and the review page. A nonzero count below a pilot limit is not a claim that those pixels are defects or that the whole image is perfect.

`BoundaryReview.html` includes 20 native-scale comparison crops and both measurement scales. It compares the matched global-colour baseline, the previous corrected AURA render and this renderer. Both 2699 eyes, the 2812 nose/face, the 2698 chin/neck and the 2843 chin/neck were visually inspected. No obvious new detached skin patch appeared in those crops. Strong smoothing, shine and natural under-eye shading are still visible; this does not certify all fine details.

The new and previous corrected AURA renders are almost identical in these crops: the largest per-crop mean absolute byte change is approximately 0.000017 on a 0-255 scale. This supports regression stability on the four examples, **not a measurable visual improvement over that already corrected version**. Rejection of contaminated references is demonstrated by the dedicated regression cases, not by asserting that these four references were contaminated. The previous comparison is AURA output, not Evoto output.

The updated desktop executable at `target/desktop-launch/debug/aura-desktop.exe` was launched. A read-only smoke check confirmed the AURA page, native IPC bridge and idle workers; evidence is `desktop-smoke.json`. No production catalog edit or new wedding delivery was initiated in this continuation. Originals and existing exports remain preserved.

## Remaining limitations

- Coherent contamination can still resemble skin. Reference consensus is not semantic recognition; real skin under mixed light can also be conservatively rejected.
- Eye/feature zones derive from five landmarks, not dense parsing. Extreme pose, occlusion, tiny faces or missing geometry can leave incomplete checks.
- Edge metrics do not classify blemishes, dark-circle pigment, makeup, pores, normal wrinkles or anatomy. Legitimate cleanup can trigger a review, and some visible defects can evade these thresholds.
- The automatic production gate uses screen-resolution analysis. Native test renders provide additional evidence only for the tested originals. They do not certify all future exports.
- White balance, RAW colour transforms and the existing global-colour algorithm are unchanged in this continuation. No universal "perfect colour" claim is made.
- Manual export retains its existing behaviour. The automatic one-click preflight is the delivery path changed here.
- Rights-verified dense parsing, representative calibration data, trained temporary/permanent-mark separation and controlled Evoto reference comparisons remain needed for stronger claims.
