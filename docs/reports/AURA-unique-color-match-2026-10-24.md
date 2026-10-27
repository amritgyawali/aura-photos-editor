# Colour matching on ten different photographs

The genuine cross-photo test does **not** establish reliable colour cloning. With one reference and ten different targets, Standard 100% produced six small statistical changes and refused four applications. None met the declared strong-match rule. This result must not be described as perfect matching or verified Evoto parity.

## Photographs and duplicate checks

Downloaded eleven independent photographs: one reference and ten targets. No target is an original/edited version of the reference. Inputs have a maximum edge of 960 pixels, with downloaded source copies bounded to 1280 pixels. No image was generated, cropped or given a synthetic matching grade for this assessment. Photos are used under the [Pexels licence](https://www.pexels.com/license/); each photographer is credited below and in the local dataset.

Reference: [Pexels 34923319, Joelle Khaled](https://www.pexels.com/photo/34923319/), a close wedding portrait.

All eleven input byte hashes and normalized pixel hashes differ. All 55 pairwise comparisons were checked: minimum dHash separation 22/64 bits, minimum pHash separation 20/64 bits, maximum block SSIM 0.197308. There were no exact duplicate or near-duplicate flags. Visual inspection of the labelled contact sheet confirmed four different outdoor women, three different indoor men, a separate wedding ceremony, a mountain lake and a night street.

The application now rejects a reference matching any full-file BLAKE3 source hash associated with the target, including a renamed identical copy, in both Prepare and Apply. This is an exact-source guard; it does not claim universal recognition of every crop, re-encoding or heavily edited version. The benchmark separately screens perceptual similarity and requires visual review. The same-frame calibration tool remains separate.

## Method

The actual bundled AURA desktop app imported ten distinct photo IDs into a disposable catalogue. Each fresh target was fitted independently against the same reference in Precise Mode. Standard used amount/tone/colour/white-balance strengths of 100%. Extreme then used 200% overall strength against the same immutable baseline; it did not stack another correction onto Standard.

Each attempt used the native full-resolution application guard, an actual full-resolution export-purpose render and the full native preview renderer. All successful preview/export pairs were pixel-identical. All 22 downloaded-source and test-input files retained their byte hashes. Refusals preserved the saved recipe. The renamed-reference negative test preserved both the photo edit and its earlier analysis session. No page errors occurred.

The independent palette score converts sRGB samples to CIE Lab D65 and compares five channel-distribution quantiles. Positive percentages mean closer to the reference; negative percentages mean farther away. Different scene content affects this score. It is not a per-pixel ground-truth comparison or proof that an unknown editing operation was recovered.

Before evaluation, the script defined a strong statistical match as at least **20% native-distance improvement and 10% independent palette-distance improvement**. These are declared review thresholds, not calibrated probabilities of photographic correctness.

## Standard 100% results

| Target / photographer | Native distance improvement | Independent Lab improvement | Outcome |
| --- | ---: | ---: | --- |
| [19403517 · terence b · outdoor woman](https://www.pexels.com/photo/19403517/) | 0.53% | −2.34% | Small change; independent palette score worsened |
| [30267960 · César O'neill · nature portrait](https://www.pexels.com/photo/30267960/) | — | — | Native skin/highlight check refused; no edit saved |
| [36646537 · abubakar mamman · garden portrait](https://www.pexels.com/photo/36646537/) | — | — | Native skin/highlight check refused; no edit saved |
| [32747251 · Zulfugar Karimov · outdoor woman](https://www.pexels.com/photo/32747251/) | 1.94% | −1.57% | Small change; independent palette score worsened |
| [33271173 · Anil Sharma · indoor man](https://www.pexels.com/photo/33271173/) | — | — | Native skin/highlight check refused; no edit saved |
| [31705625 · Wolrider YURTSEVEN · warm indoor portrait](https://www.pexels.com/photo/31705625/) | — | — | Native skin/highlight check refused; no edit saved |
| [30482418 · claytons gallary · formal portrait](https://www.pexels.com/photo/30482418/) | 2.40% | 1.34% | Small statistical improvement |
| [20479992 · Alexander Mass · wedding ceremony](https://www.pexels.com/photo/20479992/) | 2.05% | 0.13% | Small statistical improvement |
| [23531520 · Willian Justen de Vasconcellos · mountain lake](https://www.pexels.com/photo/23531520/) | 6.02% | 9.75% | Largest Standard improvement; below strong-match rule |
| [12655016 · Erik Mclean · night street](https://www.pexels.com/photo/12655016/) | 0.93% | 2.30% | Small statistical improvement |

Extreme 200% was accepted on two targets and refused on eight. The wedding ceremony improved 3.80% by the native score while its independent palette score worsened 1.62%. The mountain lake improved 9.95% natively and 19.15% independently. Neither met both strong-match thresholds. Rejected Extreme images in the comparison sheet show the previously saved edit and are labelled accordingly.

The skin/highlight limits and accuracy thresholds were not weakened to make this assessment pass. Ten photos and one reference are a limited sample, but the observed small changes and refusals are sufficient to reject a claim of reliable universal colour cloning.

## Why the match remains weak

All ten real targets took the existing unrelated-scene estimator path: structural correlations were approximately −0.097 to 0.036, content similarity was zero and actual transfer iterations were one. The Precise budget is four iterations, but that path deliberately retains the target's tone curve, applies one shared-colour update and shrinks the estimate toward identity by at least 50%. This explains the small total-distance changes; the fixes to fractional strengths do not remove this estimator limitation.

Preparation also reviews skin on a fitting image of at most 512 pixels. Apply uses the full-resolution image and different, stricter highlight/skin checks. A preview fit is consequently provisional, not a promise that Standard or Extreme will be accepted. `fullResolutionChecked` is the relevant acceptance flag. The refusal message combines skin and clipping conditions, so this assessment does not attribute a specific branch without evidence; the warm indoor man's session had no skin regions, making clipping the cause in that case.

Reliable recovery of an editing operation would require additional evidence, such as held-out known grading recipes or original/edited reference pairs with different target scenes. Reference palette similarity alone cannot prove that recovery. AURA's separate paired-calibration workflow remains available; it was not substituted for single-reference matching in this test.

## Fixes and verification

- Fresh imports now analyse without creating an edit/history step; first Apply saves atomically and supports Undo/Redo.
- Identical strengths do not create stale sessions or duplicate Undo steps.
- Same-reference fractional refits and Quick/Precise changes retain the original baseline. The 75% compounding regression was reproduced before the fix and passes afterward for both engines.
- Preview JPEGs preserve portrait/landscape shape and remain bounded. Reference cards prefer the larger preview.
- Provisional distribution fitting now reports `nativeCandidateChecked=false`; successful full-resolution Apply sets it true. This diagnostic correction changes no colour transform, guard or threshold.
- Native reference integration: nine passed, one optional local-RAW review ignored. Manual-write race unit: one passed. Preview geometry/storage units: two passed. Strict aura-app Clippy passed.
- UI: the full 1,119-test suite passed during implementation; the final changed panel/collection tests passed again, 31/31. TypeScript and production build passed. A real desktop smoke test with three genuinely different dataset photos passed live strengths, Quick/Precise fitting, preset persistence, collection refits, source preservation and page-error checks.
- The final desktop binary was checked to contain the current built frontend asset, `index-CqKjU0Mk.js`, before testing. It launched successfully despite the existing GNU linker warning about duplicate non-default manifests.

## Reproduce and inspect

```powershell
python scripts/prepare-unique-color-match-dataset.py
python scripts/test-unique-color-match.py
python scripts/test-reference-workspace-gui.py output/aura-unique-color-match-2026-10-24/dataset/reference.jpg output/aura-unique-color-match-2026-10-24/dataset/targets/01-19403517.jpg output/aura-unique-color-match-2026-10-24/dataset/targets/02-30267960.jpg
```

Pillow, numpy and Playwright are required; the desktop must be built with its bundled UI. The smoke test rejects exact or perceptually near-identical input pairs before launching.

Local review artifacts:

- `output/aura-unique-color-match-2026-10-24/index.html`: latest completed visual assessment.
- `output/aura-unique-color-match-2026-10-24/run-02b85e96/`: completed run, full PNG comparisons, raw results, native workspace screenshot and comparison sheet.
- `output/aura-unique-color-match-2026-10-24/dataset/manifest.json`: all source URLs, authors, dimensions, hashes and 55 duplicate comparisons.
- `.work-checks/reference-workspace-2a835dc7/`: passing distinct-photo desktop smoke test.

Downloaded photos, test catalogues and generated images remain local; the scripts and this measured report are committed for review.
