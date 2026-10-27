# Twenty new photographs: reference colour matching assessment

Date: 2026-10-24. **Reference and target colours do not match perfectly in every case.** This assessment tests 20 additional real photographs separately against six edited, different reference photographs. It records 120 original palette tests, 120 palette retests after an acceptance fix, and 120 tests of the existing original + edited reference calibration workflow. No target is used as its own reference or training source.

## Result

| Workflow | Exact known edit | Close known edit | Close reference palette + tone | Meaningful reference colour comparisons improved | Known edit closer than baseline |
|---|---:|---:|---:|---:|---:|
| Palette before fix | 0/120 | 0/120 | 12/120 | 118/119 | 50/120 |
| Palette after fix | 0/120 | 0/120 | 12/120 | 119/119 | 50/120 |
| Paired calibration, separate pipeline | 0/120 | 73/120 | 0/120 | 95/119 | 115/120 |

**Exact** means identical RGB8 pixels to the expected edit on that target. **Close known edit** requires mean CIE76 error ≤3 and p95 ≤8, both measured on all target pixels. **Close reference palette + tone** requires colour-quantile distance ≤3, tone-quantile distance ≤3, and joint a/b distance ≤2. These are declared QA cutoffs, not universal perceptual thresholds or proof that every object has the right colour. The CSV answers both close-match questions separately for every pair. Reference-colour improvement measures resemblance to the separate reference; known-edit recovery measures the intended operation on the target. They answer different questions. Neither is an accuracy percentage.

The calibration workflow has an automatic correction stage. Its reference edits and target oracles were generated from that workflow’s native zero-strength baselines. The palette workflow starts from its original native baseline. These are tests of two complete pipelines with their own controlled expectations, **not an isolated, identical-input algorithm A/B**. Calibration results used the pre-fix executable; the changed palette acceptance helper is not called by that workflow.

A shared editing operation does not give unrelated scenes identical colour proportions. Therefore, calibration can closely reproduce the known edit while failing the direct reference-palette test. Conversely, palette resemblance can improve while the known edit is missed.

## Photos, references and method

The new sources include night portraits and city lights, a cafe, a beach at dusk, flowers in snow, two different waterfalls, desert dunes, elderly portraits, an already monochrome portrait, bright and dark food photographs, and studio portraits under varied lighting. All 26 photo IDs differ from the previous 11-photo assessment. Byte/pixel hashes and 611 pairwise perceptual screens cover the 26 new photos and comparisons against the previous set; no duplicate flags were found. The contact sheet was visually reviewed. This is duplicate screening, not a universal image-identity detector.

Official Pexels CDN source downloads were preserved at up to 1280 pixels. Inputs preserve framing and aspect, are oriented sRGB JPEG95 without chroma subsampling, and have a long edge of at most 960 pixels. “Full/native resolution” below means the complete imported assessment image, not a camera RAW or full original Pexels master. Published sources may already contain edits; the controlled oracle tests the **additional declared grade**.

Each reference has its own source photograph. Six deterministic encoded-sRGB edits are implemented explicitly in `scripts/test-color-match-twenty.py`: warm matte, cool cyan, luminance-dependent teal/orange split tone, desaturated lifted pastel, warm high contrast/saturation, and monochrome. They change colour/tone only. The unpaired matcher receives only the edited reference pixels, never the edit parameters, unedited reference, or target oracle. The calibration workflow receives an aligned original/edited reference pair, with both photographs separate from every target.

Each target is reset before every pair. Complete editing parameters and protection fields are checked; provenance source alone may change because Reset is a user action. Full original render pixels must match exactly. Each pair runs native fitting/application, full export-purpose rendering, independent pixel measurements and native full-preview parity. All 20 saved targets are then exported through native Delivery as verified lossless sRGB PNGs, once per reference.

Independent measurements use CIE Lab with a D65 white point: five Lab quantiles, colour-only a/b quantiles, 32 signed joint a/b projections of 2048 deterministic stratified samples, full-image target/oracle CIE76 mean and p95, and separate shadow/midtone/highlight residuals. A 0.1-unit colour-error floor marks near-neutral comparisons where a percentage would magnify numerical noise. In particular, the monochrome target already had neutral chroma; an apparent relative chroma improvement is not a meaningful colour achievement.

## Concrete bug found and fixed

Target 17 (Pexels 32972578, dim studio) with the teal/orange fruit reference exposed an acceptance error. The native combined distance improved **11.517659 → 7.942121**, while native colour-palette distance worsened **5.125721 → 5.451486**. Independent colour quantile error also worsened **33.0114 → 36.6042**; joint a/b distance worsened **20.4799 → 21.2036**. A large tone improvement concealed a colour regression.

Native acceptance and the in-process proof cache now require **both total and palette distances to be non-regressing**, using the existing 0.0001 numerical tolerance. Natural mode has a zero palette term, preserving its previous policy. Fitting can backtrack or reject a proposal; a rejected proposal cannot replace the saved edit. Skin/highlight thresholds and distance weights were not changed. Palette diagnostics are versioned `reference-palette-transport-v5`; live matching and Sync refit saved v3/v4 palette sessions. A regression test uses the measured failing distances and verifies that the cache cannot certify them.

The retest of that pair produced colour quantile error **33.0114 → 29.4111**, and joint colour distance **20.4799 → 17.9587**. Its known-edit error remains **10.94 mean / 20.23 p95 CIE76**. This corrects acceptance policy; it does not establish perfect grade recovery. [Actual previous/retested/expected pixels](../../output/aura-twenty-color-match-2026-10-24/regression-fix-comparison.jpg) also show residual contouring in the dark backdrop. The colour guard does not solve that spatial quality problem.

## Results by reference grade

| Grade | Palette median joint-colour improvement after fix | Palette median known-edit CIE76 | Calibrated close known edit | Calibrated median known-edit CIE76 |
|---|---:|---:|---:|---:|
| warm-matte | +28.0% | 9.56 | 14/20 | 0.81 |
| cool-cyan | +51.0% | 14.26 | 11/20 | 1.98 |
| teal-orange | +8.8% | 7.91 | 7/20 | 3.27 |
| pastel | +67.3% | 12.78 | 13/20 | 0.96 |
| warm-punch | +8.5% | 14.27 | 8/20 | 4.10 |
| monochrome | +100.0% | 11.14 | 20/20 | 0.32 |

Calibration reference validation is separate from these new target photographs. Its native error uses **OKLab ×100**, not the CIE76 units above. Low error on held-out tiles of one reference does not prove that unseen scenes are covered.

| Training reference | Occupied coarse colour cells /64 | Native reference validation mean / p95 |
|---|---:|---:|
| warm-matte | 19 | 0.182 / 0.383 |
| cool-cyan | 23 | 0.229 / 0.454 |
| teal-orange | 36 | 0.318 / 0.734 |
| pastel | 15 | 0.191 / 0.576 |
| warm-punch | 20 | 0.305 / 0.799 |
| monochrome | 17 | 0.190 / 0.463 |

Reference source credits (each is separate from all 20 targets):

- warm-matte: [Senior man with flat cap, Pexels 13822354](https://www.pexels.com/photo/13822354/) — Jonathan.
- cool-cyan: [Green forest stream, Pexels 12779583](https://www.pexels.com/photo/12779583/) — Lukas Feldmann.
- teal-orange: [Assorted fruit bowls, Pexels 1652312](https://www.pexels.com/photo/1652312/) — Jane T D..
- pastel: [Smiling woman in white top, Pexels 33202132](https://www.pexels.com/photo/33202132/) — Gabriela Brasiliano.
- warm-punch: [Cafe with striped awning, Pexels 34704802](https://www.pexels.com/photo/34704802/) — syd.trgt.
- monochrome: [Brunette studio portrait, Pexels 23891642](https://www.pexels.com/photo/23891642/) — Andrea Musto.

## Every target separately: palette retest

Cells are mean target/oracle CIE76, lower is closer. A low mean alone does not pass the close criterion; p95 must also pass. See the CSV for both values, each reference/target ID, direct reference-palette distances, exact/close flags and per-pair diagnosis.

| Target / photo ID | Warm matte | Cool cyan | Teal/orange | Pastel | Warm punch | Mono |
|---|---:|---:|---:|---:|---:|---:|
| 01: Woman on beach at night (33380315) | 5.99 | 6.57 | 6.48 | 7.64 | 9.15 | 8.90 |
| 02: Illuminated coastal city (16646551) | 11.87 | 15.48 | 12.06 | 17.78 | 5.25 | 22.37 |
| 03: Rustic cafe and red flowers (18688314) | 23.67 | 12.89 | 23.65 | 12.99 | 18.26 | 32.13 |
| 04: Man on beach at night (15069828) | 3.31 | 21.63 | 7.56 | 15.54 | 11.23 | 32.79 |
| 05: Urban beach at dusk (30154480) | 11.02 | 11.60 | 13.55 | 5.85 | 19.43 | 7.60 |
| 06: Red flowers in snow (29209958) | 21.16 | 27.76 | 12.68 | 19.40 | 17.49 | 19.57 |
| 07: Mossy forest waterfall (11008002) | 26.64 | 18.68 | 31.60 | 19.75 | 33.51 | 26.89 |
| 08: Autumn forest waterfall (3455934) | 16.33 | 18.46 | 19.42 | 17.18 | 20.46 | 29.60 |
| 09: Desert dunes and blue sky (26651573) | 13.17 | 17.85 | 27.28 | 11.02 | 19.83 | 11.98 |
| 10: Senior man outdoors (37164686) | 7.35 | 7.00 | 4.86 | 5.90 | 7.30 | 9.18 |
| 11: Bearded man in dramatic shadows (7808457) | 8.47 | 6.51 | 3.81 | 5.00 | 5.30 | 8.92 |
| 12: Thoughtful grey-haired senior (7322073) | 8.40 | 13.03 | 3.81 | 8.08 | 8.89 | 3.75 |
| 13: Monochrome senior portrait (17000433) | 4.85 | 10.43 | 8.26 | 8.42 | 11.91 | 6.58 |
| 14: Bright fruit bowl (37461742) | 21.38 | 18.49 | 14.51 | 17.07 | 27.30 | 9.04 |
| 15: Figs and grapes in dark setting (20699589) | 10.19 | 16.71 | 5.39 | 10.43 | 11.48 | 33.59 |
| 16: Woman in white corset in studio (36701322) | 3.78 | 10.18 | 3.48 | 6.59 | 10.46 | 9.05 |
| 17: Dim studio fashion portrait (32972578) | 14.68 | 23.25 | 10.94 | 22.42 | 29.94 | 26.28 |
| 18: Dramatic red studio portrait (20788937) | 5.76 | 11.76 | 6.77 | 18.55 | 16.63 | 13.82 |
| 19: Studio portrait with natural hair (20756240) | 5.84 | 7.14 | 5.32 | 12.59 | 9.42 | 9.74 |
| 20: Woman in pastel pink dress (31714366) | 8.93 | 22.41 | 5.07 | 12.98 | 23.91 | 10.31 |

## Every target separately: calibrated grade

Cells are mean target/oracle CIE76, lower is closer. A low mean alone does not pass the close criterion; p95 must also pass. See the CSV for both values, each reference/target ID, direct reference-palette distances, exact/close flags and per-pair diagnosis.

| Target / photo ID | Warm matte | Cool cyan | Teal/orange | Pastel | Warm punch | Mono |
|---|---:|---:|---:|---:|---:|---:|
| 01: Woman on beach at night (33380315) | 7.09 | 6.68 | 3.86 | 8.63 | 8.05 | 0.49 |
| 02: Illuminated coastal city (16646551) | 0.44 | 0.57 | 3.92 | 0.45 | 1.55 | 0.15 |
| 03: Rustic cafe and red flowers (18688314) | 0.53 | 0.73 | 3.42 | 0.60 | 2.94 | 0.20 |
| 04: Man on beach at night (15069828) | 0.90 | 1.31 | 2.82 | 1.14 | 2.73 | 0.85 |
| 05: Urban beach at dusk (30154480) | 0.49 | 0.75 | 4.18 | 0.44 | 1.35 | 0.14 |
| 06: Red flowers in snow (29209958) | 1.19 | 3.39 | 4.47 | 0.63 | 4.27 | 0.50 |
| 07: Mossy forest waterfall (11008002) | 0.75 | 0.61 | 2.64 | 0.97 | 1.30 | 0.42 |
| 08: Autumn forest waterfall (3455934) | 0.53 | 0.59 | 3.86 | 0.69 | 3.40 | 0.23 |
| 09: Desert dunes and blue sky (26651573) | 0.58 | 0.98 | 3.13 | 0.62 | 4.66 | 0.21 |
| 10: Senior man outdoors (37164686) | 8.42 | 6.07 | 2.88 | 7.67 | 5.66 | 0.31 |
| 11: Bearded man in dramatic shadows (7808457) | 8.03 | 6.41 | 3.07 | 6.42 | 4.39 | 0.15 |
| 12: Thoughtful grey-haired senior (7322073) | 7.68 | 9.12 | 3.81 | 6.22 | 7.89 | 0.12 |
| 13: Monochrome senior portrait (17000433) | 5.04 | 4.05 | 2.24 | 0.68 | 1.86 | 0.43 |
| 14: Bright fruit bowl (37461742) | 0.74 | 1.02 | 5.19 | 0.54 | 6.51 | 0.34 |
| 15: Figs and grapes in dark setting (20699589) | 0.68 | 1.11 | 3.92 | 0.95 | 3.92 | 0.54 |
| 16: Woman in white corset in studio (36701322) | 0.37 | 8.23 | 3.02 | 9.10 | 11.39 | 0.16 |
| 17: Dim studio fashion portrait (32972578) | 0.88 | 1.36 | 1.25 | 1.04 | 0.95 | 0.87 |
| 18: Dramatic red studio portrait (20788937) | 6.68 | 17.12 | 2.49 | 23.54 | 18.95 | 0.39 |
| 19: Studio portrait with natural hair (20756240) | 2.37 | 9.37 | 2.29 | 14.11 | 11.25 | 0.79 |
| 20: Woman in pastel pink dress (31714366) | 0.61 | 2.61 | 3.40 | 0.52 | 2.04 | 0.20 |

## What does not match and how to improve it

| Finding | Evidence and consequence | Fix / next improvement |
|---|---|---|
| Scene colours are confused with the edit | Flowers, fruit, walls and water change the reference histogram; forcing that histogram onto another scene can improve palette distance while moving away from the intended grade. | Use the existing original + edited reference workflow when both are available. For unpaired matching, compare compatible measured regions and retain uncertainty instead of labelling histogram similarity exact. |
| Different scene brightness changes the inferred tone curve | Dark/night subjects can be lifted toward a bright reference; monochrome outputs can be fully desaturated but still miss brightness, black point and contrast. | Separate scene exposure normalization from style tone; fit tone using corresponding neutral/subject regions or paired edit evidence. Keep the Natural option available when scene brightness should remain. |
| Split tones and hue changes remain imperfect | The oracle tests shadows, midtones and highlights separately. The unpaired chromatic lattice is a 2D a/b mapping, so its displacement does not independently model every lightness-dependent colour operation. | Extend unpaired residual fitting to a bounded lightness-conditioned/3D mapping, with held-out colour and gradient checks. The existing paired calibrated model already provides a different route for global RGB mappings. |
| Protection attenuates requested grades | Native backtrack factors and calibrated saved amounts are recorded per pair. Some desired casts/contrast edits cannot be retained at full strength under the current skin/clipping limits. | Improve semantic subject masks and separate creative grading from unintended damage in the acceptance objective. Do not silently remove current checks or claim a limited endpoint is a full-strength clone. |
| Small subjects lack measured face/body regions | Targets 17 and 20 visibly contain people, but their native matched-region lists contain only whole image/background in these tests. Colour-based protection is not equivalent to reliable subject segmentation. | Improve multiscale face/person detection and mask confidence at native resolution; allow explicit corresponding-region anchors when automatic evidence is missing. Recheck skin colour on small subjects separately. |
| Dark-backdrop contouring remains | Visual review of target 17 shows colour bands/patches after a strong unrelated-scene palette transfer, including the corrected output. Aggregate colour metrics do not capture this defect. | Add native flat-region/gradient quality checks; isolate amplification of source JPEG quantization, tone/chroma slopes, gamut compression and final quantization before changing the renderer. Existing gamut mapping already compresses chroma; the precise source of this example’s contouring is not established here. |
| One training photo has incomplete colour coverage | Calibration can fit its reference well but extrapolate poorly to colours/luminances absent from that reference. Native calibration reports occupied colour cells and held-out error. | Learn from multiple aligned original/edited examples with varied illumination and colour coverage; evaluate on distinct held-out photographs and report out-of-coverage cases. This remains additional work. |
| Unknown physical white balance and clipped/local edits cannot be uniquely recovered | These are rendered JPEG/PNG images without camera/lighting ground truth; multiple edit recipes can yield the same reference. | Offer explicit neutral/skin region anchors and colour-managed camera baselines. Test RAW, ICC/wide-gamut, HDR and local edits separately before extending claims. |

The implemented change fixes the observed colour-regression acceptance bug. The architectural improvements above are recommendations, not claims of completed algorithm changes. An edited image of an unrelated scene alone does not uniquely identify its original colours, illumination and editing operation. Consequently, a universal 100% exact-transfer guarantee would be unsupported.

For the current app: use **Use original + edited pair** when both versions of the reference are available, keeping all targets separate from that training photograph. Use **Reference palette and tone** when intentionally transferring scene colour proportions; choose **Natural look** when preserving the target scene’s tonal balance matters. Inspect the result and use the independent tone, colour and warmth strengths to address residuals. A red/green or blue/yellow residual in the CSV describes measured colour bias; it does not uniquely identify a physical white-balance error.

## Verification and reproducibility

All 20 original baseline renders were byte-identical across the before/after assessments. Independent retesting changed 1 of 120 output images; 119 outputs were byte-identical to the previous engine. Changed pairs: [(17, 'teal-orange')]. The final retest accepted **zero native colour-palette regressions** under the declared tolerance.

Code validation: four native proof/cache tests passed, including the measured regression; native colour-match integration tests passed (10 passed, one optional RAW test skipped because its external fixtures were not provided); the focused UI suite passed 39 tests, including v4 live-session and Sync migrations. TypeScript/Vite production build and desktop build succeeded. Existing frontend chunk-size and GNU linker resource-manifest warnings remain. Python assessment scripts compiled successfully.

Real UI follow-up: [results](../../.work-checks/reference-workspace-c79f81c4/results.json) and [workspace](../../.work-checks/reference-workspace-c79f81c4/workspace.png). Quick one-click matching, live 75% strength, saved preset, Precise refitting, collection Sync and original-file checks passed on two new targets with bounded 640-pixel GUI fixtures. Reference-switch outcome: **refused without changing the saved edit or prior session**. This is separate from the 360 full-imported-image pair assessments.

The warm-to-cool reference switch was not a successful match: `Native UI refusal: No reference palette fit preserved this photo's native skin tones, highlights and measured colour distance. Try Natural look or a different reference. No edit was saved.`. The saved recipe hash and previous session ID were verified unchanged, and the UI continued to label the new reference unapplied. Do not interpret this refusal-preservation check as successful colour transfer.

GUI frame observations: 808 visible samples, median gap 17.900000000000546 ms, p95 18.09999999999991 ms, maximum 53.899999999999636 ms. These measurements include occasional stalls and are not a guarantee of smoothness on other workloads.

Preliminary GUI attempt retained: [reference-workspace-6faf08ca](../../.work-checks/reference-workspace-6faf08ca/results.json): `TargetClosedError: Page.evaluate: Target page, context or browser has been closed`; desktop exit before cleanup `1`. A window/connection closed unexpectedly in one attempt; its cause was not established. Another attempt stayed open but the old harness timed out waiting for success despite the explicit native refusal. The harness now detects alerts promptly and records refusals separately.

Preliminary GUI attempt retained: [reference-workspace-9519dd11](../../.work-checks/reference-workspace-9519dd11/results.json): `AssertionError: Timed out waiting for saved colour match`; desktop exit before cleanup `None`. A window/connection closed unexpectedly in one attempt; its cause was not established. Another attempt stayed open but the old harness timed out waiting for success despite the explicit native refusal. The harness now detects alerts promptly and records refusals separately.

- **before**: [local gallery](../../output/aura-twenty-color-match-2026-10-24/run-3cea7f47/index.html), [raw native results](../../output/aura-twenty-color-match-2026-10-24/run-3cea7f47/results.json), [independent analysis](../../output/aura-twenty-color-match-2026-10-24/run-3cea7f47/independent-analysis.json). Executable SHA256 `dc40b00902d59d7a851a07c04d01954686a9f373196d361c33c86b1502299b83`. Checks: all 120 pairs, distinct references/targets, original baseline before each pair, native preview/export pixel parity, 120 verified Delivery PNGs, original source/input hashes unchanged and no JavaScript page errors. Desktop exit before controlled cleanup: `None`.
  Observed median analysis 4116.906899987953 ms; median Apply 110.0 ms. Debug-build observations with other local verification work; not a controlled throughput benchmark.
- **after**: [local gallery](../../output/aura-twenty-color-match-2026-10-24/run-0a31f1ef/index.html), [raw native results](../../output/aura-twenty-color-match-2026-10-24/run-0a31f1ef/results.json), [independent analysis](../../output/aura-twenty-color-match-2026-10-24/run-0a31f1ef/independent-analysis.json). Executable SHA256 `9cc79f6f0bfec73377d86abc384f5aadc1111a13f95f24e32bb0eca234cc45ba`. Checks: all 120 pairs, distinct references/targets, original baseline before each pair, native preview/export pixel parity, 120 verified Delivery PNGs, original source/input hashes unchanged and no JavaScript page errors. Desktop exit before controlled cleanup: `None`.
  Observed median analysis 4178.786300006323 ms; median Apply 117.8 ms. Debug-build observations with other local verification work; not a controlled throughput benchmark.
- **paired**: [local gallery](../../output/aura-twenty-color-match-2026-10-24/run-cfd34598/index.html), [raw native results](../../output/aura-twenty-color-match-2026-10-24/run-cfd34598/results.json), [independent analysis](../../output/aura-twenty-color-match-2026-10-24/run-cfd34598/independent-analysis.json). Executable SHA256 `dc40b00902d59d7a851a07c04d01954686a9f373196d361c33c86b1502299b83`. Checks: all 120 pairs, distinct references/targets, original baseline before each pair, native preview/export pixel parity, 120 verified Delivery PNGs, original source/input hashes unchanged and no JavaScript page errors. Desktop exit before controlled cleanup: `None`.
  Observed median analysis not applicable (one shared calibration per reference) ms; median Apply 1573.6 ms. Debug-build observations with other local verification work; not a controlled throughput benchmark.

The original palette run paused after its first reference because the harness incorrectly required Reset provenance hashes to remain identical. It resumed only after verifying original pixel equality and all parameter/protection fields; the earlier failure is retained as `previousRunFailure`. No failed result was silently replaced by a pass.

[All 360 pair measurements and diagnoses](AURA-twenty-image-colour-pairs-2026-10-24.csv). [Dataset credits](../../output/aura-twenty-color-match-2026-10-24/dataset/ATTRIBUTION.md), [duplicate evidence](../../output/aura-twenty-color-match-2026-10-24/dataset/manifest.json), [source contact sheet](../../output/aura-twenty-color-match-2026-10-24/dataset/contact-sheet.jpg). Pexels permits downloading and modifying these photos under its [licence](https://www.pexels.com/license/); individual photographers and source pages are recorded. Downloaded photographs, catalogues and generated galleries stay in ignored local output; scripts, this report and numerical CSV are tracked.

Reproduce from the repository root (Pillow, numpy and Playwright required):

```powershell
python scripts/prepare-color-match-twenty.py
# Inspect dataset/contact-sheet.jpg; record honest visual review in manifest.json.
python scripts/test-color-match-twenty.py --intent palette
python scripts/test-color-match-twenty.py --paired-calibration
python scripts/analyse-color-match-twenty.py <completed-run-directory>
python scripts/report-color-match-twenty.py --before <before-run> --after <after-run> --paired <paired-run>
```

Known pre-existing Natural-mode accuracy failures are recorded in the preceding [palette-fix report](AURA-reference-palette-fix-2026-10-24.md). They are not cleared by this assessment. This assessment does not establish proprietary Evoto algorithm equivalence or correctness for all photographs.
