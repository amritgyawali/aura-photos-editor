# Advanced reference palette matching: desktop verification

Date: 2026-10-24. The rebuilt desktop completed all four real-photo runs and the interactive smoke test. Reference palette and tone is substantially stronger on these photographs; universal exact colour recovery is not established.

## What changed

Match a look now offers two explicit matching goals. **Natural look** retains the existing conservative estimator for unrelated scenes. **Reference palette and tone** deliberately fits the reference's lightness, neutral-axis cast, chroma, shared hue sectors and tonal zones even when the photographs have different content. Quick and Precise retain their real 10,000/two-iteration and 40,000/four-iteration budgets.

Palette fitting checks its endpoint with actual native-resolution renders before offering it at 100%. It brackets and refines safe strengths within six candidate renders, keeps the best measured safe candidate, and reports the retained fitting factor. Each attempt reuses one native frame and one measured baseline. Applying strengths performs another current native check before saving; an extrapolated setting is not automatically approved because the fitted endpoint passed.

The palette transform also carries a new optional `skinFidelity` value. It bounds the joint tone and colour displacement of skin-like colours, rather than reducing the entire image solely to preserve a face. The smooth hue/chroma weighting is independent of lightness to avoid brightness reversals in a fixed-colour gradient. It is colour-based protection, not proof of perfect semantic segmentation. Older grades omit this field and retain their existing rendering.

The existing full-resolution skin, highlight and colour-distance limits remain unchanged. Exact-source duplicate rejection, immutable same-reference baselines, zero-strength restoration, stale/manual-edit protection, source preservation and Undo/Redo remain in place. The product guard rejects identical file contents, including renamed copies; it does not universally identify re-encoded, cropped or edited versions of the same frame. The assessment separately screens perceptual duplicates and uses different photographs. Palette mode requires current reference measurements instead of silently falling back to the legacy slider estimator.

Monochrome references retain full desaturation at the fitted endpoint, including skin, while safety refinement can reduce the other fitted components. Fractional user strengths can still blend colour back intentionally. The monochrome path uses the existing achromatic skin guard.

For fitting speed, quantile sorting now avoids scratch allocation and recognizes arrays already sorted for another rank query. Safety failures are rejected before sorting their colour descriptors. The live UI keeps serialized writes, cancellation and its existing debounce; collection Sync refits each target with the selected goal and quality.

Native verification also retains at most 64 in-process proofs. Both baseline and candidate are still rendered at full resolution. Their complete RGB data and dimensions are hashed with BLAKE3; reference measurements, named sample coordinates and monochrome intent form the context key. Only an exact pixel/context match can reuse a previous measured result. Changed native pixels or guard inputs force fresh checks. Baseline guard aggregates are shared without retaining photo buffers or duplicate coordinate lists. These proofs are never loaded from catalogue JSON or portable references, and are discarded with the application state.

Statistical colour transfer is a known image-processing approach; see [Reinhard et al., Color Transfer between Images](https://home.cis.rit.edu/~cnspci/references/dip/color_transfer/reinhard2001.pdf) and [Pitie's colour-distribution research](https://francois.pitie.net/research/colour-transfer/). This implementation extends AURA's existing renderer-based quantile fit; it does not claim to implement either paper exactly or reproduce Evoto's proprietary algorithm.

## Review in AURA

Open Match a look, choose a different uploaded reference, select **Matching goal → Reference palette and tone**, select Quick or Precise, and analyse. Review the side-by-side reference, the measured distance and any fitted-strength limit before applying. Live adjustments and Sync use the selected goal. Switching goals requires fresh analysis instead of applying an incompatible saved fit.

## Measurement protocol

Reuse the previous assessment's one reference and ten distinct Pexels targets. All eleven input byte/pixel hashes differ; all 55 pairwise duplicate checks and the visual review remain available in the dataset manifest. All photos and credits are recorded in [the preceding assessment](AURA-unique-color-match-2026-10-24.md). These targets are different photographs, not before/after versions of the reference.

The desktop harness uses an isolated catalogue and profile, native IPC, full export-purpose rendering and full native previews. It verifies source hashes, renamed-duplicate refusal without losing the preceding edit/session, native cancellation, zero and repeated strengths, preview/export agreement and page errors. The warm Precise run additionally writes, reads back and verifies ten real lossless sRGB PNG files through Delivery. It records per-call elapsed time and browser animation-frame gaps. The interactive smoke test uses separate 640-pixel copies of three different photographs. Timings are local observations of this Windows debug build on inputs bounded to 960 pixels, not a production, RAW or megapixel throughput guarantee.

The unchanged strong-match rule requires at least 20% native-distance improvement **and** 10% independent CIE Lab distribution improvement. Different subjects and object proportions affect both statistics. There is no known editing operation for these downloaded photographs, so even a strong score cannot establish exact colour cloning or correct recovery of an unknown edit.

## Final desktop results

| Goal / reference | Standard applied | Strong statistical matches | Refused | Extreme accepted |
| --- | ---: | ---: | ---: | ---: |
| Natural Precise / warm | 6/10 | 0 | 4 | 2/10 |
| Palette Quick / warm | 10/10 | 8 | 0 | 0/10 |
| Palette Precise / warm | 10/10 | 7 | 0 | 0/10 |
| Palette Precise / cool reference | 10/10 | 5 | 0 | 2/10 |

| Warm-reference mode | Median analysis | Median Standard apply | Median native improvement | Median independent Lab improvement |
| --- | ---: | ---: | ---: | ---: |
| Palette Quick / warm | 2936 ms | 217 ms | 28.64% | 27.61% |
| Palette Precise / warm | 4555 ms | 123 ms | 22.83% | 21.21% |

| Target | Precise native improvement | Precise independent Lab improvement | Outcome |
| --- | ---: | ---: | --- |
| 19403517 / Outdoor woman | 24.32% | 23.41% | strong statistical match |
| 30267960 / Woman in nature | 20.19% | 19.01% | strong statistical match |
| 36646537 / Woman in a garden | 5.52% | 5.62% | partial statistical match |
| 32747251 / Outdoor woman | 7.53% | 4.85% | partial statistical match |
| 33271173 / Indoor man | 21.35% | 15.06% | strong statistical match |
| 31705625 / Warm indoor man | 34.85% | 37.86% | strong statistical match |
| 30482418 / Formal man | 32.06% | 37.57% | strong statistical match |
| 20479992 / Wedding ceremony | 46.53% | 46.05% | strong statistical match |
| 23531520 / Mountain lake | 63.65% | 78.30% | strong statistical match |
| 12655016 / Night city | 15.21% | 13.75% | partial statistical match |

Raw completed runs:

- Palette Quick / warm: [output/aura-unique-color-match-2026-10-24/run-3fc941ea](../../output/aura-unique-color-match-2026-10-24/run-3fc941ea/index.html).
- Palette Precise / warm: [output/aura-unique-color-match-2026-10-24/run-6ac1f50f](../../output/aura-unique-color-match-2026-10-24/run-6ac1f50f/index.html).
- Palette Precise / cool reference: [output/aura-unique-color-match-2026-10-24/run-7f910b2c](../../output/aura-unique-color-match-2026-10-24/run-7f910b2c/index.html).
- Natural Precise / warm: [output/aura-unique-color-match-2026-10-24/run-d4993e50](../../output/aura-unique-color-match-2026-10-24/run-d4993e50/index.html).

The three palette runs made 30 Standard attempts across 20 different reference/target pairs using eleven unique photographs. All 30 were accepted at their native-safe fitted endpoints. This is successful application on this dataset, not 100% photographic accuracy. Quick and Precise used different samples and regional analyses and selected different constrained endpoints; a larger fitting budget did not guarantee the strongest statistical match on every target. The strong-match thresholds were not changed.

The 200% requests were still refused for both warm-reference palette runs. The galleries explicitly label those images as the saved Standard edit being kept. The safety checks were not weakened to make Extreme succeed.

### Speed and responsiveness

| Warm-reference mode | Median Apply before verification reuse | Final median Apply | Observed ratio |
| --- | ---: | ---: | ---: |
| Quick | 1036 ms | 217 ms | 4.8× |
| Precise | 847 ms | 123 ms | 6.9× |

These are Standard Apply calls immediately after analysis, when an identical full-pixel verification result is available in the same process. Every palette target reused that result. Reopening the application empties this cache; changed pixels or context require fresh measurement. Fitting itself remains more expensive than applying a verified fit. Local timing varies with hardware, background load and the selected transform; these single-machine observations are not a standardized production benchmark.

All 120 before/Standard/Extreme PNG comparison artifacts across the four runs were byte-identical to their counterparts before the verification-cache optimization. [Pixel identity evidence](../../output/aura-advanced-palette-verification-2026-10-24/pixel-identity.json) records each source and final run. Thus the measured speed change did not alter their output colours.

The actual desktop smoke test passed Quick/Precise switching, live 75% strengths, preset persistence, collection Sync and original-file preservation with three different photographs. It recorded 1017 visible browser-frame intervals: median 16.7 ms, 95th percentile 16.8 ms and largest 116.8 ms. Occasional stalls remain; these browser animation-frame measurements are not proof of zero jank or GPU throughput. The [workspace screenshot](../../.work-checks/reference-workspace-82440c95/workspace.png) was visually reviewed together with the final comparison sheet. Portrait proportions were preserved and the target/reference photographs were different.

Native cancellation completed in 59–138 ms in these runs and preserved both the edit and preceding analysis session. All successful full preview/export-render pairs were pixel-identical. The warm Precise Delivery job wrote and read-back verified ten actual PNGs, sealed its manifest, and recorded zero render failures or corrupt files. Decoding every delivered PNG produced an exact pixel match to its export-purpose render. All 22 source/input file hashes remained unchanged in each run, and no page errors were recorded.

### Build and local review

All four runs used one rebuilt desktop executable, SHA-256 `ee2a88fdcc32d10b5de3ba6526801207f13557a111a6638b7d43f2d55e8d6c26`, on Windows-10-10.0.19045-SP0 with 12 logical processors. The current bundled frontend asset, `index-W9as24fA.js`, was confirmed inside the executable. The existing GNU linker warning about duplicate non-default manifests was non-fatal; the app launched and completed these checks.

Open the [combined local review](../../output/aura-advanced-palette-verification-2026-10-24/index.html) for all comparison galleries and raw summaries. Generated images, catalogues and profiles are local artifacts rather than committed repository files.


## Verification and remaining failures

- Full UI suite: 119 files, 1,121 tests passed. TypeScript and production build passed.
- Native colour unit suite: 71 passed, one failed. The existing Natural-look batch known-grade accuracy test remains enabled with its original threshold. It is not a palette-mode ground-truth success.
- Native reference workflow integration: ten passed, one optional RAW test ignored. These include the new palette endpoint, atomic first saves, fractional refits, stale/manual edits, source duplicates, history and source preservation.
- Existing native grading integration: two safety/stale-measurement tests passed; the known-grade photographic accuracy gate failed. Split-tone target error was 1.32 → 0.89 and warm matte was 4.17 → 3.35, insufficient for its unchanged accuracy requirements. Together with the batch failure above, this means the native accuracy suite is not fully green.
- Recipe/reference-renderer checks: four recipe tests and nine renderer tests passed, including the skin displacement and monotone-gradient regressions. All three native verification-cache tests passed, covering changed pixels/dimensions, reference/region/monochrome context, session serialization, malformed non-finite inputs and bounded retention.
- Optional RAW verification requires a real local RAW/reference fixture and remains outside the downloaded JPEG experiment. No such RAW fixture was found in the workspace.
- Strict app/renderer/recipe Clippy, final formatting and all 83 locked contract checks passed. The required commit hook additionally checks banned patterns and strict workspace Clippy.

## Reproduce

```powershell
python scripts/prepare-unique-color-match-dataset.py
python scripts/test-unique-color-match.py --intent palette --quality quick
python scripts/test-unique-color-match.py --intent palette --quality precise --verify-delivery
python scripts/test-unique-color-match.py --intent palette --quality precise --reference-photo-id 23531520
python scripts/test-unique-color-match.py --intent natural --quality precise
python scripts/test-reference-workspace-gui.py output/aura-unique-color-match-2026-10-24/dataset/reference.jpg output/aura-unique-color-match-2026-10-24/dataset/targets/01-19403517.jpg output/aura-unique-color-match-2026-10-24/dataset/targets/02-30267960.jpg --intent palette
```

Pillow, numpy, Playwright and a built bundled AURA desktop are required. Downloaded photos, generated comparison PNGs, test catalogues and browser profiles remain local; scripts and this report are reviewable source changes.
