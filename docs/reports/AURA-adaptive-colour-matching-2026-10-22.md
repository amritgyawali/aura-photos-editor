# AURA adaptive colour matcher — 2026-10-22

The native matching algorithm now searches signed corrections, separate curve anchors and colour bands, and tone-dependent casts. It handles newly imported achromatic references with a reversible grayscale contribution. Native-resolution validation checks the proposed treatment before analysis is saved; Apply rechecks the exact strengths. This is a working algorithm change, with the real desktop and exports tested.

**A universal 100% colour/tone match is not established.** In the repeated 18-case known-grade experiment, 7/18 results were closer than the old matcher, 11/18 were unchanged within 0.01 mean CIE76, 15/18 were closer than the ungraded baseline, and 0/18 were pixel exact. Every row below is retained, including regressions. A correct implementation can still produce an imperfect photographic match; subject matter, lighting, clipping and protected edits constrain what a reference can tell it.

## What changed

- Additive pattern search explores both directions, including parameters initially zero, within the existing style bounds. It adjusts ten scalar controls, five curve anchors, and saturation/luminance for sufficiently supported colour bands. At most 160 refinement candidates are scored; duplicate canonical recipes reuse scores. Hue-band rotations remain conservative.
- A further 36 bounded probes fit independent shadow/midtone/highlight casts through the existing native grade wheels. Offsets use Cartesian hue/chroma coordinates to avoid the 0°/359° seam. Existing wheel luminance, global grade, blending and balance are retained, and manually protected fields pass through the same merge rules.
- A reference verified as achromatic from its imported pixels can remove chroma continuously through native saturation. Amount/Colour/Saturation strengths control that contribution, and zero restores the baseline. The achromatic skin guard checks every supported person, limits all seven tone landmarks and allows movement toward neutral while rejecting new casts and added chroma. Normal colour references keep the existing skin-colour guard. A selective-colour reference is not classified as monochrome merely because its 90th-percentile chroma is low.
- Before publishing a fit, render the native source and backtrack the fitted treatment through 100/75/50/25/0% if necessary to retain skin/highlights and avoid increasing native appearance distance. Apply remains explicit and can refuse unsafe custom strengths. The allowed native worsening tolerance was reduced from 0.25 to 0.0001 descriptor units.
- Keep old saved sessions readable. New imports/captures store bounded 32 × 32 RGB structure samples for gradient correlation; these are analysis data and are not copied into target pixels. Private reference metadata is upgraded when its source is reimported. Descriptor-only portable libraries retain their format and use conservative defaults until source pixels are supplied again.

## Regression found and corrected

The first unregularized prototype passed its functional tests while making some different-scene matches visibly washed out. For example, the warm known-grade error on target 1 rose from 7.903 to 21.933 even though its internal descriptor improved. That build was not qualified for release. Its complete results and binary hash are retained.

The revised search penalizes extra edit effort relative to the established guarded incumbent. Reference/target gradient correlation ≥0.95 permits a lighter 0.25 penalty; unrelated/unknown content uses 4.0. These are authored conservative policy values, not calibrated confidence probabilities or a guarantee. This reduces the opportunity to chase scene differences with large tone or cast changes. The final known-grade table below measures whether it helped.

## Controlled photographic results

The same three supplied-photo proxies and six known native AURA grades were used, with unchanged input hashes and matching baseline measurements. Compare against a known grade applied directly to each target. Mean aligned CIE76 is a difference measure, not a perfection percentage. The source-photo case tests reconstruction; the other two test transfer. No pixel comparison is assigned to unrelated Internet/reference scenes.

| Grade / target | Baseline error | Old matcher | Unregularized prototype | Revised matcher | Versus old |
|---|---:|---:|---:|---:|---|
| warm / 0.png | 10.189 | 6.048 | 5.714 | 5.955 | closer |
| warm / 1.png | 9.094 | 7.903 | 21.933 | 7.903 | unchanged |
| warm / 2.png | 8.392 | 5.795 | 3.966 | 5.795 | unchanged |
| cool / 0.png | 11.988 | 7.615 | 7.204 | 7.615 | unchanged |
| cool / 1.png | 10.742 | 5.567 | 17.342 | 5.567 | unchanged |
| cool / 2.png | 10.010 | 6.798 | 7.265 | 6.798 | unchanged |
| matte / 0.png | 5.976 | 2.813 | 2.315 | 2.685 | closer |
| matte / 1.png | 8.076 | 2.823 | 16.847 | 2.823 | unchanged |
| matte / 2.png | 7.370 | 5.658 | 6.775 | 5.658 | unchanged |
| split-tone / 0.png | 2.447 | 2.294 | 1.721 | 0.848 | closer |
| split-tone / 1.png | 3.268 | 5.629 | 17.985 | 5.629 | unchanged |
| split-tone / 2.png | 2.978 | 4.026 | 4.713 | 4.026 | unchanged |
| punchy / 0.png | 7.754 | 5.019 | 4.316 | 4.629 | closer |
| punchy / 1.png | 10.468 | 11.782 | 25.576 | 11.782 | unchanged |
| punchy / 2.png | 8.320 | 5.635 | 8.375 | 5.635 | unchanged |
| monochrome / 0.png | 30.516 | 26.933 | 0.792 | 0.964 | closer |
| monochrome / 1.png | 39.746 | 24.945 | 9.480 | 6.763 | closer |
| monochrome / 2.png | 31.130 | 27.486 | 6.604 | 1.458 | closer |

[Full controlled gallery](../../output/aura-adaptive-reference-final-2026-10-22/index.html), [case CSV](AURA-adaptive-colour-matching-2026-10-22-cases.csv) and [machine report](AURA-adaptive-colour-matching-2026-10-22.json). Functional tests separately verify analysis does not edit history, matching is reversible, zero restores exact pixels and unsupported inputs are refused.

## Actual desktop and export checks

15 desktop steps passed with both downloaded high-resolution references, four transfers, exact Undo/Redo and zero removal, reload, stale-analysis refusal and collection matching that preserves manual exposure. All four main and twelve fine controls were exercised in 20 boundary cases: 16 saved and 4 explicitly refused; refusals kept the saved recipe/history unchanged.

| Internet reference / target | Native appearance distance | Delivered JPEG mean CIE76 versus its render |
|---|---:|---:|
| vibrant-teal-orange.jpg / target-0.jpg | 22.082 → 20.837 | 1.081 |
| vibrant-teal-orange.jpg / target-1.jpg | 24.574 → 23.449 | 1.152 |
| warm-cinematic.jpg / target-0.jpg | 22.031 → 18.176 | 1.133 |
| warm-cinematic.jpg / target-1.jpg | 20.377 → 18.869 | 1.136 |

The JPEGs were decoded with ICC conversion into sRGB and compared with the same target's saved export render. All four met the declared test bounds (mean CIE76 <2, p95 <5). Internal descriptor decrease is not proof of equal colour, tone, white balance or lighting across different subjects.

The original `H:\AURA EDIT\989A2865.CR3` was also matched and delivered at **4480 × 6720**. Native skin/highlight checks passed at amount 1.00; appearance distance 19.837 → 18.584. PNG matches every export-render pixel. JPEG mean CIE76 1.8394, p95 3.6656 on a declared 4-pixel sampling grid. Both recipe sidecars equal the delivered recipe; source/reference hashes stayed unchanged. Sidecars remain AURA canonical JSON with an `.xmp` suffix, without established Adobe/Evoto interoperability.

[Open the exported comparisons](../../output/aura-adaptive-match-2026-10-22/index.html) or the [full-resolution matched JPEG](../../output/aura-adaptive-match-2026-10-22/989A2865-adaptive-full.jpg). Native PNG/evidence remain in `F:\AURA-verification\aura-adaptive-raw-2026-10-22`.

## Verification and limits

- Application library: 412 passed, 0 failed, 2 existing opt-in cases ignored. Reference integration: 5 passed, 0 failed, 1 opt-in RAW test ignored; the real RAW was exercised separately above.
- Frontend: 1052 tests passed in 111 files; final TypeScript/production UI build and desktop build passed. The existing non-default Windows manifest linker warning remains.
- Qualified desktop SHA-256: `a40f9cbc953155ad6c43ba3202a8986359c1f0fc50e92955575c1a43e68dcc0a`. Exact commands, exit codes, source hashes and fitter diagnostics are in the machine report. Historical reports retain their own binary/test totals.
- New tests cover zero and wrong-sign seeds, independent curve/band/wheel fits, non-finite probes, evaluation budgets, cancellation, achromatic person guards, selective-colour detection, structure evidence and protected saturation. Earlier compile attempts caught a test Vec API mistake and a missing float type annotation; corrected attempts are retained rather than counted as passes.
- Distribution matching has a long research history; see [Reinhard et al.](https://home.cis.rit.edu/~cnspci/references/dip/color_transfer/reinhard2001.pdf) and [Pitié et al.](https://www.cs.jhu.edu/~misha/ReadingSeminar/Papers/Pitie07.pdf). This implementation uses AURA's bounded parametric renderer and measured acceptance checks. It does not recover an arbitrary hidden LUT, original camera WB or scene illumination from a finished photograph.
- JPEG/opaque PNG/TIFF reference limits remain. Guard refusals, conservative no-change fits and imperfect matches remain possible. Exact Evoto equivalence and universal photographic correctness remain unverified.
