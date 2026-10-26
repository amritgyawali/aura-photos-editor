# Adaptive Match a look: native transform selection

Date: 2026-10-25. This extends the main reference-first workflow with adaptive
native transform selection.

## Implementation

New references retain OKLab tone/chroma distributions, neutral-axis casts, hue
sectors and joint colour evidence. The main fitter compares Natural and Palette
transform strategies through the native renderer. Their recipe can express
nonlinear tone/chroma curves, hue changes, lightness-dependent casts, selective
colourfulness and continuous palette transport beyond the old slider limits.
Registered images additionally qualify a monotone per-channel affine RGB model,
using robust residual weights, bounded gains and held-out spatial tiles. The
native-checked candidate can stop further searching below an authored error
tolerance of 0.1 OKLab x100. This is a computational stopping target, not an
accuracy percentage or a perceptual certification.

The existing slider fitter remains a candidate for aligned reconstruction and a
fallback when a richer transform cannot qualify. Original/edited calibration
retains its existing supervised path.

Colour fitting uses up to 40,000 stratified observations from the native baseline
render. Area-reduced input is retained for thumbnail structure analysis, not used
as a substitute for the source colour distribution. A fine fabric can contain
separate red and blue threads even when its thumbnail appears purple.

The main reference decoder also preserves original pixels instead of resizing to
512 pixels before measuring them. This fixes a separate loss of reference colour
evidence in fine patterns. Existing 256 MB / 64 megapixel decode bounds,
orientation, opacity and supported ICC conversion checks remain. Ordinary RGB8
sRGB inputs reuse their pixel buffer instead of allocating a full RGB16 copy.
Whole-image safety descriptors also bound their analysis edge to 512 pixels for
large masters. Compact region sample strips retain all their observations, so
native skin checks do not lose sample support to that memory limit.

Single references additionally store up to 2,048 reproducible native RGB
observations. Affine spatial trends are removed before structure correlation;
local tiles must provide distributed supporting evidence. This can recognize
aligned fine patterns missed by the 32-pixel thumbnail. Registered samples use
direct OKLab pixel error. Unrelated scenes use a joint distribution objective
with separate total/palette nonregression. Neither registration heuristic nor a
smaller distribution distance proves recovery of an unknown editing operation.

Folder evidence is bounded to eight retained images per lighting bucket, chosen
by stable key hashes and pooled with equal sample support per retained image.
There are ten lighting buckets. Four retained references are required for a
lighting-specific distribution. Single references use native statistics instead
of that reduced pool. Stored evidence is validated on load. Older single-photo
caches can migrate from their preview; older folders keep their previous fitter
until reanalysis.

Each strategy has at most four descending strength checks. Every accepted
endpoint is rendered at native resolution and checked for clipping, detected
skin changes and amplified steps in originally smooth shadows. Canonical recipe
hashes deduplicate proposals. Strength, replacement and repetition start from
the reconstructed baseline, rather than applying over the previous transform.

The recipe merge now recognizes `reference_grade_v1` as an optional block and
validates its contents. An automatic proposal can remove it at zero strength,
while a manually protected block survives. Replacement also removes omitted
palette maps, skin-fidelity fields and calibrated residual lattices, preventing
parts of the previous model from leaking into a new treatment. Native fitting and transactional
history saving use the same merged canonical values.

## Validation scope

The native suite includes eight unrelated reference treatments: warm, cool,
matte, punchy, split tone, sepia, monochrome and rose. It checks distinct output
pixels, native improvement, non-compounding repetition, half/full/zero strength,
direct reference replacement, legacy single-reference cache migration and
unchanged input files. Existing known-grade, fine-fabric, calibration, manual
field and cancellation assertions are retained without weaker accuracy gates.

The recipe library's 112 tests pass. The focused colour unit run has 96 passes
and the unchanged pre-existing Natural batch fidelity failure. The reference
picker UI's nine tests pass and the TypeScript/Vite
production build passes; the existing bundle-size warning remains.

The main native suite passes six active tests (one optional local-photo review
is run separately). All eight unrelated-look treatments produce distinct saved
pixels and pass repetition, strength, replacement, reset and source-integrity
checks. Native known-grade mean RGB8 error is 4.593 -> 0.487. The fine striped
fixture is 9.985 -> 0.393, compared with 6.874 in the prior implementation. These
are fixture measurements, not universal accuracy percentages. The studio colour
integration suite passes ten tests, with its optional RAW fixture skipped.

| Unrelated synthetic reference | Native appearance distance before | After |
|---|---:|---:|
| Warm (migrated cache) | 5.5411 | 1.2916 |
| Cool | 4.2650 | 1.1507 |
| Matte | 4.8611 | 1.0496 |
| Punchy | 3.4422 | 1.7091 |
| Split tone | 3.9565 | 0.8586 |
| Sepia | 4.3063 | 0.4576 |
| Monochrome | 4.2376 | 0.1049 |
| Rose | 5.3227 | 0.9125 |

Seven locally cached real targets cover night portraiture, forest, a dramatic
portrait, an already monochrome source, fruit, dim studio lighting and colourful
clothing. References are separate images with known declared grades. Native
matches run in isolated catalogues and verify unchanged input bytes. Independent
CIE Lab quantile distances and known-edit CIE76 errors are recorded separately.
The already monochrome source is preserved when a colour fit cannot qualify.

| Real target / reference treatment | Independent reference-colour distance before | After | Known 80% edit error before / after |
|---|---:|---:|---:|
| Night portrait / warm matte | 20.053 | 16.114 | 7.559 / 3.728 |
| Forest / cool cyan | 6.971 | 2.318 | 5.014 / 10.264 |
| Dramatic portrait / teal-orange | 34.930 | 33.252 | 3.201 / 3.420 |
| Already monochrome / cool cyan | 11.038 | 11.038 | 4.516 / 4.516 |
| Fruit / pastel | 40.435 | 17.511 | 18.313 / 12.406 |
| Dim studio / warm punch | 55.380 | 53.143 | 3.906 / 5.696 |
| Pink dress / monochrome | 3.360 | 0.712 | 3.593 / 4.388 |

Six targets move closer under the independent colour metric; the grey source
remains unchanged. The known-edit result improves in two cases, worsens in four,
and is unchanged in one. Neither outcome certifies the other. These are seven
examples, not evidence for every scene or editing operation. Matches request
80% strength. The declared 80% oracle blends the baseline with the known edited
baseline in encoded RGB; AURA's parameter interpolation can differ. Full-strength
oracle comparisons are also retained in the raw measurements.

[Native comparison gallery](../../.work-checks/adaptive-main-2026-10-25-2b85ce1c/native-final/index.html),
[native measurements](../../.work-checks/adaptive-main-2026-10-25-2b85ce1c/native-final/results.json),
[independent measurements](../../.work-checks/adaptive-main-2026-10-25-2b85ce1c/native-final/independent.json).
Photo provenance and duplicate screening remain in the
[existing dataset manifest](../../output/aura-twenty-color-match-2026-10-24/dataset/manifest.json).

Strict workspace Clippy, formatting, banned-pattern scanning, the 353-command IPC
name check and the 83 locked contracts pass. No photographic accuracy assertion
was relaxed. The original/edited-pair cases were rerun after enabling native
reference decoding for both paired inputs and retain their existing results.
After bounding the large-image safety descriptors, all three appearance guard
unit tests, six main native tests and ten studio integration tests pass again.

## Desktop build

The updated [Windows executable](../../target/desktop-launch/debug/aura-desktop.exe)
was rebuilt successfully after the final source change, using:

```powershell
cargo.exe build --locked --manifest-path ui/src-tauri/Cargo.toml --target-dir target/desktop-launch --features custom-protocol -j 1
```

Artifact size: 497,473,657 bytes. SHA-256:
`F1318CF2527355FCC26278B1C4F910BE489445DEC4FF226F0C6D0AFE9A5D6369`.
The existing GNU linker warning about multiple non-default resource manifests
remains; Cargo exits successfully. The frontend production build also retains
its existing bundle-size warning. The executable was built without opening a
personal catalogue. The final
[build log](../../.work-checks/adaptive-main-2026-10-25-2b85ce1c/desktop-verified-build.log)
records the successful build.

## Remaining limits

An edited unrelated reference cannot identify its original illumination, object
colours or exact editing operation. These changes broaden the supported global
looks; they do not establish perfect matching for arbitrary images, local retouch,
relighting, missing/clipped colour information, or every RAW/HDR camera pipeline.
The pre-existing Natural-mode unit and transfer integration accuracy failures
remain separate qualification limits; their assertions are not relaxed.
