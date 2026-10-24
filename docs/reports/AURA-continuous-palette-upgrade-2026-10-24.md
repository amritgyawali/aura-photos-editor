# Match a look: continuous palette transport and shadow quality

Date: 2026-10-24. Engine: `reference-palette-transport-v6`.

This upgrade addresses unstable colour mapping and newly amplified steps in smooth shadows. It does not make unrelated reference and target photographs match perfectly. Reference-palette resemblance, recovery of a known editing operation, and spatial image quality are measured separately.

## Code and behaviour

- Quantile transport now collapses repeated input ranks into one output centroid. A flat source receives a continuous translation instead of splitting into the reference's extremes. Piecewise interpolation is continuous at both endpoint and interior knots.
- Chromatic projection maps bound interval gain to 0.25–2.0, integrate those increments, and choose a weighted least-squares intercept. The bound limits amplification of very small colour differences. Tone/chroma curve fitting retains its rank interpolation and existing separate regularisation.
- Each chromatic projection prepares its map once and uses binary search for lookups. Previously, each lookup rescanned coincident quantiles. The existing continuous palette LUT, gamut compression and native renderer remain part of the pipeline.
- Native candidate review also checks adjacent pixels from originally smooth, dark regions. Large newly amplified steps can trigger the existing bounded backtracking or explicit refusal. The saved endpoint still has to pass separate native total-distance and palette-distance nonregression, skin and highlight checks.
- Tone and palette readouts now show the actual saved strength's native measurements. The former diagnostics retained the fitted endpoint's measurements after a strength change. Zero-strength integration coverage verifies return to the original baseline; real UI checks also cover 75%.
- Live matching and collection Sync refit saved older palette engines. A refused operation clears the progress message and leaves the saved edit intact.

The UI's native OKLab-based distance components and the independent CIE Lab measurements below use different definitions. Neither is a percentage of accuracy.

The shadow check observes at most 4,096 stratified pixels and their right/down neighbours. Eligible source pairs have all RGB8 channels at most 96 and maximum channel difference at most 2. With at least 128 eligible pairs, no more than 0.5% may become steps of 12 or more. These are authored quality limits, not perceptual standards. Insufficient observations skip this extra rejection check; passing does not prove all image regions are smooth. The independent assessment counts **every** eligible horizontal and vertical pair across each complete imported image.

## Repeated-rank regression and performance

A flat source around 0.5 mapped to reference quantiles spanning 0.2–1.0 previously jumped approximately 0.800002 when the input moved from 0.499999 to 0.500001. It now changes by approximately 0.00000203. Tests cover coincident endpoint/interior ranks, a constant source, bounded near-flat gain, identity and an affine grade.

The optimized isolated lookup benchmark ran five repeats of 1,280,000 evaluations, including one prepared map per projection. Median lookup workload: **53.03 ms previous / 11.66 ms prepared (4.55×)**. This is not a 4.55× claim for complete image matching. Native decoding, fitting, rendering and backtracking have separate costs. [Raw local benchmark](../../.work-checks/quantile-bench-61d4b09d/results.json).

## Scope and interpretation

The desktop retest reuses the earlier assessment's 20 unique targets and six separately sourced, differently graded references. Every reference differs from every target. Photos cover portraits, studio lighting, night scenes, food, landscapes and an already monochrome source. Imported assessment images are oriented sRGB JPEGs with a long edge at most 960 pixels. Results do not establish RAW, HDR or full camera-master performance.

Each pair starts from pixel-verified original native renders. The matcher receives only the edited reference. Known target expectations apply the declared editing operation to that target's native baseline; those expectations are never supplied to fitting. Full preview, export-purpose render and native Delivery PNGs are compared. Source and input file hashes are checked. Independent analysis measures CIE Lab D65 quantiles, signed joint a/b distributions and complete-image known-edit CIE76 errors.

"Close known edit" requires mean CIE76 at most 3 and p95 at most 8. "Close reference palette and tone" requires colour-quantile distance at most 3, tone distance at most 3, and joint a/b distance at most 2. Exact means bitwise target/oracle pixel identity. These are declared QA thresholds. A shared grade cannot make different subjects have identical colour proportions, and an edited unrelated reference cannot uniquely identify its original white balance or editing history.

## Desktop results and tradeoffs

| Measurement | Previous v5 | Updated v6 |
|---|---:|---:|
| Exact known edit | 0/120 | 0/120 |
| Close known edit | 0/120 | 0/120 |
| Close reference palette and tone | 12/120 | 12/120 |
| Meaningful independent colour comparisons improved over original | 119/119 | 118/119 |
| Known edit closer than original | 50/120 | 55/120 |
| Full-frame large shadow steps, pooled count | 462,836 | 52,962 |

All 120 updated cases saved native-checked endpoints, with zero native total/palette regressions under the 0.0001 numerical tolerance. Every original-baseline reset, preview/export comparison, source/input hash check and native Delivery verification passed. There were no JavaScript page errors; AURA remained running before controlled cleanup. Both versions used identical original baseline and reference pixels. 119 of 120 output images changed.

Compared with v5, independent reference-colour distance became closer in 32 pairs, farther in 57, and within 0.001 in the remaining 31. Known-edit error became closer in 58, farther in 39, and within 0.001 in 23. This is a spatial-quality/stability improvement with mixed colour-fidelity changes, not uniform palette improvement.

The pooled large-shadow-step count fell by 88.6%. Across individual pairs, 43 had fewer such steps, nine had more, and 68 were unchanged. Eight full-frame fractions exceeded 0.5%, despite passing the bounded native sample; a sampled threshold is not a full-frame guarantee. All 120 images had at least 128 eligible pairs in the independent full-image measurement.

Target 17 (32972578), teal/orange: the full-frame step fraction fell **2.4236% -> 0.1001%**, across 1,049,937 original eligible neighbour pairs. Known-edit mean/p95 CIE76 changed **10.94 / 20.23 -> 5.90 / 11.45**. However, reference colour distance worsened relative to v5, **29.41 -> 31.42**, and tone distance **55.70 -> 60.16**. Both still improved over the unedited target's distances of 33.01 and 62.73. The fitted endpoint was limited to 12.5% of its unconstrained proposal. [Actual original/reference/v5/v6/expected pixels](../../output/aura-twenty-color-match-2026-10-24/run-a4149173/shadow-regression-comparison.jpg).

Target 17 with warm punch is a remaining independent colour regression: CIE colour distance **54.2588 -> 54.9657** relative to its unedited original, about 1.3% worse, while native OKLab measures passed. Its shadow-step rate improved relative to v5, **7.4553% -> 0.3066%**, and known-edit error **29.94 -> 8.84**, but it is still a weak colour match. Different colour spaces and distribution summaries do not certify each other. The already monochrome source is excluded from meaningful chroma-percentage comparisons when its initial error is below the declared 0.1-unit floor.

| Reference grade | Median joint-colour improvement over original | Median known-edit CIE76 |
|---|---:|---:|
| warm-matte | +27.5% | 8.64 |
| cool-cyan | +49.5% | 13.03 |
| teal-orange | +6.9% | 6.69 |
| pastel | +63.7% | 10.47 |
| warm-punch | +8.5% | 11.67 |
| monochrome | +100.0% | 10.02 |

[Every pair: numerical CSV](AURA-continuous-palette-pairs-2026-10-24.csv). It includes separate reference and known-edit outcomes, source IDs, shadow counts, native-check results and previous/new measurements. Exact known-edit reproduction was absent in every case. [Gallery](../../output/aura-twenty-color-match-2026-10-24/run-a4149173/index.html), [native results](../../output/aura-twenty-color-match-2026-10-24/run-a4149173/results.json), [independent diagnoses](../../output/aura-twenty-color-match-2026-10-24/run-a4149173/pair-measurements.md), [comparison evidence](../../output/aura-twenty-color-match-2026-10-24/run-a4149173/upgrade-comparison.json). Photo credits and duplicate screening are linked from the [earlier 20-photo assessment](AURA-twenty-image-colour-assessment-2026-10-24.md).

## Verification and latency

Native integration: **10 passed, one optional RAW fixture test skipped**. Colour-match units: **80 passed, one existing Natural batch accuracy failure** described below. Four quantile tests passed again after the lint corrections. Focused UI tests: **43 passed**. TypeScript/Vite production build, desktop build, Python script compilation and strict aura-app Clippy passed. Existing frontend bundle-size and GNU resource-manifest linker warnings remain.

The real native UI test passed 11 workflow checks: Quick one-click apply, correct native component readouts, live 75% strengths, component readouts following those strengths, preset persistence, Precise refitting, collection Sync, refusal preserving recipe/session, clearing refused progress and source integrity. The warm-to-cool reference switch was **refused**, not successfully transferred. [GUI evidence](../../.work-checks/reference-workspace-fe47ebb2/results.json), [screenshot](../../.work-checks/reference-workspace-fe47ebb2/workspace.png). Tone difference changed from 32.49 -> 23.29 at 100% to 32.49 -> 25.35 at 75%; palette difference changed from 2.94 -> 2.57 to 2.94 -> 2.69. The readouts matched the actual saved native values. There were no page errors or unexpected desktop exit in this test.

UI observations: 1,050 visible frame samples, median gap 16.7 ms, p95 16.8 ms, maximum 187.6 ms. Occasional stalls remain. Overall observed median analysis was **4,499 ms**, versus **4,179 ms** in v5; median Apply was **115.4 ms**, versus **117.8 ms**. These development-build runs included other local activity and are not controlled throughput benchmarks. The faster isolated quantile lookup does not establish faster end-to-end fitting; native review/backtracking still dominates some photographs.

Final tested executable SHA256: `2b49702b4ec5ce363c761b112d3cae91fcd0a6307c95ea28941dfd27c3d0ebd1`. The complete v6 run is `run-a4149173`; comparison v5 is `run-0a31f1ef`. Intermediate CDF-only previews and an intentionally stopped preliminary assessment are excluded from these final 120-pair results.

## Remaining limitations and useful next work

Distribution matching can move one object's colour toward another object's dominant colour. Small faces, different lighting, missing hues, already edited inputs and colours outside the reference's coverage remain difficult. The new shadow guard reduces one measurable artifact; it does not repair every existing gradient, remove all noise or prove semantic colour correctness.

For faithful recovery of an editing operation, AURA's existing aligned original + edited reference calibration provides additional information. A future improvement should validate reference coverage, use multiple calibrated references covering skin, neutrals and saturated colours, and fit luminance-dependent corrections with spatial residual checks. That work needs independent unseen-scene accuracy evidence; the current unpaired mapper cannot infer missing evidence with certainty.

The existing Natural-mode test `a_batch_matched_to_one_reference_converges_on_one_look` remains unresolved: errors 1.319 → 0.868 and 1.281 → 1.791 do not satisfy its required recovery. Its assertion was preserved. This upgrade must not be reported as clearing all colour accuracy failures or reproducing a proprietary application's algorithm.


## Reopen check and reproduction

The assessment catalogue was reopened and a switch from its saved monochrome look to warm matte was refused. The saved monochrome pixels were verified unchanged. An explicit Reset to original was then performed on that disposable test target, followed by a fresh warm-matte match. Its complete rendered pixels matched the earlier warm-matte assessment exactly. AURA remains open on that result. [Reopen evidence](../../output/aura-twenty-color-match-2026-10-24/run-a4149173/review-result.json), [open workspace](../../output/aura-twenty-color-match-2026-10-24/run-a4149173/native-final-workspace.png).

Different-reference analysis currently retains the earlier look as its crossfade baseline. A guarded transition from an existing treatment can therefore be refused even when matching a fresh original succeeds. A clear user choice to replace the previous treatment from its original matching baseline, while preserving subsequent manual edits, would improve this workflow. No production or personal catalogue was reset during this assessment.

Required pre-commit checks enforce formatting, banned-pattern scanning, strict workspace Clippy and the 83 locked IPC contracts. Photographs, executable builds and generated galleries remain local ignored artifacts; code, this report and the 120-pair numerical comparison are tracked.

Reproduce with the existing reviewed dataset, Pillow, numpy and Playwright installed:

```powershell
python scripts/bench-color-quantiles.py
python scripts/test-color-match-twenty.py --port 9390
python scripts/analyse-color-match-twenty.py <completed-v6-run>
python scripts/compare-color-match-runs.py <completed-v5-run> <completed-v6-run>
# Faster regression-only native assessment:
python scripts/test-color-match-twenty.py --target-indices 17 --styles teal-orange --port 9390
```
