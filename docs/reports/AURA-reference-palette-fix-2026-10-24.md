# Reference portrait to mountain: colour matching fix

Date: 2026-10-24. AURA was rebuilt and tested with the reference and target shown in the reported mismatch, plus different photographic scenes. The palette transfer is stronger on the reported pair; these tests do not establish universal exact colour recovery.

## Confirmed problem and changes

The existing catalogue already recorded the warm portrait reference as applied at Standard strength to the mountain/lake photograph. Its checked recipe hash matched its saved recipe hash. The blue result was therefore a transfer-quality problem. [The previous saved match](../../output/aura-reference-mismatch-fix-2026-10-24/prior-saved-match.json) records that state.

The previous hue-sector fit used overlap between hues. Missing warm colours and dominant blue colours were poorly represented by that overlap objective. The new palette engine, `reference-palette-transport-v4`, keeps the tone/cast estimator and adds deterministic sliced transport of the joint, signed OKLab a/b distribution. Quick uses 16 projection updates and Precise uses 32. Sixty-five quantiles per projection, bounded update steps and a continuous 33 × 33 chromatic displacement lattice avoid spatial resampling of the photograph. The palette residual leaves lightness unchanged; the existing tone curve controls brightness separately. Mean warmth/tint displacement is stored separately from the residual for independent controls.

The palette distance now includes eight signed projected distributions alongside the earlier tone/cast/chroma/hue terms. This fixes the missing-colour blind spot, but changes the native scoring objective. Native percentage improvements from the old and new engines are **not directly comparable**. The independent CIE Lab measurements below use the same calculation and original pixels in both versions.

Every fitted endpoint still passes native skin, clipping and nonregression checks. When full transport would require a large global reduction, the native search additionally checks the underlying tone/cast fit and keeps the closer safe endpoint. It shares one native frame/baseline and permits up to twelve candidate renders in that conditional search. No skin or clipping threshold was relaxed. Zero strength restores the preceding transform exactly, including its palette map.

Older saved reference measurements can obtain the missing joint evidence from their stored 32 × 32 thumbnail. Reuploading obtains fresh evidence from the reference image. Existing saved palette sessions are refitted by live matching and collection Sync. Natural-look grades without a palette field keep their rendering.

The workspace adds **Match reference to this photo**, which performs fresh analysis and native application for the selected reference in one click. It verifies photo/reference IDs and cancellation before saving. A selected-but-unapplied reference is labelled explicitly; a no-improvement endpoint is not described as a closer match. The separate analysis and fine-strength controls remain available.

The transport is informed by the signed distribution-transfer approach described by [Pitié, Kokaram and Dahyot](https://www.cs.jhu.edu/~misha/ReadingSeminar/Papers/Pitie07.pdf), with AURA-specific bounded fitting, lattice evaluation and native safety checks.

## Reported photograph: independent comparison

The portrait reference (Pexels 19403517) and mountain target (23531520) are different photographs. Original native renders in the old and new fresh-import assessments were byte-identical.

| Measurement | Previous engine | New engine |
| --- | ---: | ---: |
| Independent full Lab distribution improvement over original | 26.22% | 82.01% |
| Independent colour-only a/b distribution improvement over original | -1.16% | 80.26% |
| Colour-only quantile error (lower is closer) | 35.782 | 6.981 |

The new rendered mountain/lake carries the portrait's warm olive/gold palette instead of retaining the previous strong blue cast. The original composition and dimensions are preserved. [Local visual comparison and raw measurements](../../output/aura-reference-mismatch-fix-2026-10-24/index.html) includes the old and new results and the actual existing-catalogue workspace.

The same selected reference and mountain photograph were also matched through the real one-click UI in the existing catalogue. Its colour-only error fell from 35.782 to 7.115. The final native report and screenshot are saved locally; the rebuilt app was left open for review.

## Broader real-photo verification

The same eleven unique downloaded photographs were reused, with three different references and ten different targets per reference. Byte/pixel hashes, all 55 pairwise duplicate screens and source credits are in [the original dataset assessment](AURA-unique-color-match-2026-10-24.md). Each assessment uses an isolated catalogue; reference and target are different photos.

| Reference / Precise mode | Standard accepted | Strong statistical matches | Median independent Lab improvement | Median analysis | Median Apply |
| --- | ---: | ---: | ---: | ---: | ---: |
| portrait | 10/10 | 5 | 30.21% | 6606 ms | 119 ms |
| wedding | 10/10 | 5 | 19.51% | 6066 ms | 121 ms |
| cool | 10/10 | 4 | 18.59% | 7202 ms | 117 ms |

The existing strong-match rule still requires at least 20% native improvement and 10% independent Lab improvement. Its native term uses the new objective, so this classification is not an old/new accuracy benchmark. Partial matches remain, especially where skin protection constrains a different portrait's palette. More saturated or stronger settings still require native safety checks. These are unpaired photographs without a known grading-operation ground truth.

The colour-only a/b comparison uses the same independent CIE Lab quantiles for every old/new pair, excluding lightness. Its raw record is [colour-only-comparison.json](../../output/aura-reference-mismatch-fix-2026-10-24/colour-only-comparison.json).

| Reference | Old median colour-only improvement | New median colour-only improvement | Targets improved or equal |
| --- | ---: | ---: | ---: |
| Portrait | 9.05% | 20.42% | 10/10 |
| Wedding | 20.47% | 23.40% | 7/10 |
| Cool | 19.25% | 18.12% | 8/10 |

Thus 25 of 30 reference/target pairs improved or tied on this independent colour-only measure. The cool-reference median was slightly lower, and five pairs were worse. This is not a universal improvement guarantee.

Portrait-reference per-target independent Lab improvement:

| Target | Previous engine | New engine |
| --- | ---: | ---: |
| 34923319 | 18.85% | 18.90% |
| 30267960 | 10.78% | 10.78% |
| 36646537 | 8.51% | 11.46% |
| 32747251 | 13.32% | 12.06% |
| 33271173 | 16.83% | 16.83% |
| 31705625 | 33.86% | 45.80% |
| 30482418 | 7.00% | 42.54% |
| 20479992 | 41.50% | 77.12% |
| 23531520 | 26.22% | 82.01% |
| 12655016 | 14.49% | 41.52% |

Completed final galleries:

- portrait: [run-b3468711](../../output/aura-unique-color-match-2026-10-24/run-b3468711/index.html).
- wedding: [run-161e1676](../../output/aura-unique-color-match-2026-10-24/run-161e1676/index.html).
- cool: [run-2f54a343](../../output/aura-unique-color-match-2026-10-24/run-2f54a343/index.html).

All successful preview/export-purpose pairs passed parity checks. The portrait-reference Delivery run wrote, read-back verified and decoded ten actual lossless PNGs; each exactly matched its export-purpose render. Original source/input hashes, renamed-duplicate refusal, native cancellation, zero/repeated strengths and page-error checks passed. The interactive desktop test passed one-click matching, changed-reference application, Quick/Precise switching, live strengths, presets and collection Sync. Its raw record is [reference-workspace-35e196bc](../../.work-checks/reference-workspace-35e196bc/results.json).

One preliminary automated window closed before completion; that attempt was not counted as a completed assessment. The final cases were rerun sequentially. One test-harness call initially omitted the required `input` wrapper; the harness was corrected before the completed GUI run.

The interactive run recorded 700 visible frame intervals: median 16.7 ms, 95th percentile 16.8 ms and maximum 141.3 ms. Occasional stalls remain; this is not a zero-jank guarantee. Every successful native preview/export comparison in the final runs had zero mean RGB difference. All final desktop processes were still running before their deliberate test cleanup.

## Build, tests and limits

- Final desktop SHA-256: `dc40b00902d59d7a851a07c04d01954686a9f373196d361c33c86b1502299b83`. Bundled frontend: `index-Bflujye7.js`.
- UI: all 119 files / 1,125 tests passed; TypeScript and production build passed.
- Native reference workflow: ten passed; the optional RAW fixture test remains ignored. All ten renderer/reference-grade tests and five recipe/reference-grade tests passed, including lattice continuity and malformed/nonfinite-map rejection.
- Native colour unit suite: 73 passed, one existing Natural-look batch accuracy gate failed with unchanged errors (1.319 → 0.868; 1.281 → 1.791). Its threshold remains enabled. The preceding report also records a separate Natural-look photographic integration accuracy failure; universal grading accuracy is not established.
- Formatting, strict Clippy and Python syntax checks passed. The commit hook additionally enforces banned patterns, strict workspace Clippy and the locked contracts.
- Observations use a Windows debug desktop and bounded JPEG inputs. They do not establish megapixel/RAW throughput or correctness for every camera/profile. The existing nonfatal GNU manifest warning remains.
- Whole-image distribution matching can recolour scene objects, including sky/water; a similar palette is not proof of recovery of an unknown edit. Skin protection can deliberately limit a match. The original files remain unchanged.

## Reproduce

```powershell
python scripts/test-unique-color-match.py --intent palette --quality precise --reference-photo-id 19403517 --verify-delivery
python scripts/test-unique-color-match.py --intent palette --quality precise
python scripts/test-unique-color-match.py --intent palette --quality precise --reference-photo-id 23531520
python scripts/test-reference-workspace-gui.py output/aura-unique-color-match-2026-10-24/dataset/reference.jpg output/aura-unique-color-match-2026-10-24/dataset/targets/09-23531520.jpg output/aura-unique-color-match-2026-10-24/dataset/targets/10-12655016.jpg --second-reference output/aura-unique-color-match-2026-10-24/dataset/targets/01-19403517.jpg --intent palette
```

Run the dataset preparation script first if the credited local fixtures are absent. Photos, test catalogues, PNGs and browser profiles remain local artifacts; source code, tests and this report are committed.
