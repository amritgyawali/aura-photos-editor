# AURA implementation and validation — 2026-10-22

Publication copy omits the embedded reference-photo thumbnail. Full photo evidence remains local and is excluded from Git. Publication preparation removed embedded photo thumbnails and updated scripts/check-banned.sh to recognize dedicated tests.rs modules. Application code and the qualified executable were unchanged; the source-manifest match describes the qualification snapshot before this checker-only change.

Implemented **100 additional capabilities**: 25 colour methods, 25 spatial methods, 25 selections, 25 read-only diagnostics. The three method catalogues now contain 75 entries each. Three additional review screens bring inspection to 70 measurements. Existing workspace changes were preserved.

**Exact Evoto equivalence, every Evoto feature, recovered proprietary models and per-tool perfection have not been achieved or verified.** These additions are original mathematical implementations.

| Validation | Result |
|---|---|
| Complete native workspace suite | 4502 passed, 0 failed, 53 ignored; 291 result groups |
| Final changed-library retest | 778 passed, 0 failed, 5 ignored; includes checked-coordinate and inspection cleanup |
| Final selection fixture retest | 16 passed after three test-only lint fixes |
| Final renderer fixture retest | 323 passed, 3 ignored after fixture lint cleanup |
| Frontend | 998 tests in 111 files passed; 6 targeted mask-panel retests passed after the label correction; production build passed |
| Formatting, strict Clippy, contracts, IPC, evidence tooling | Passed; 347 command definitions/registrations/typed calls agree; 18 Python tests |
| New methods in rebuilt AURA | 100/100 invoked on synthetic fixture; 100/100 invoked on real-photo proxy; history, stale revisions, inspection immutability checked |
| Prior additions regression | Both earlier 100-capability suites passed in the rebuilt desktop |
| Supplied CR3 photos | 27/27 edited and inspected; original SHA-256 values unchanged |
| Full-resolution JPEG export | 27 written, 27 verified, 0 render failures |
| Executable/source identity | All 5 desktop scenarios match current executable; 1497 source hashes unchanged after freeze |

[Edited-photo gallery](../../output/aura-2026-10-22/index.html) · [Machine-readable report](AURA-validation-2026-10-22.json) · [Per-tool CSV](AURA-validation-2026-10-22-tools.csv) · [Native cases](AURA-validation-2026-10-22-native-tests.csv)

Desktop captures: [colour](assets/aura-2026-10-22-colour.png), [delivery-review](assets/aura-2026-10-22-delivery-review.png), [lighting-review](assets/aura-2026-10-22-lighting-review.png), [selection](assets/aura-2026-10-22-selection.png), [spatial](assets/aura-2026-10-22-spatial.png), [surface-review](assets/aura-2026-10-22-surface-review.png), [workflow](assets/aura-2026-10-22-workflow.png)

## Evoto investigation

REA MCP inspected the supplied native worker and exported retained PE-resource evidence. The refreshed installation inventory hashed all 1860 regular files (4873227287 bytes), with no read gaps; 1854 vendor-manifest checks matched. The worker matches earlier bounded native research by SHA-256.

This connected REA session has no configured deep-analysis provider. Earlier decompilation and runtime findings are reused only for the matching artifact. No proprietary code, weights, shaders or UI assets were incorporated. File coverage is not algorithm coverage. See [provider status](../../.work-checks/evoto-2026-10-22/provider-status.json) and [retained REA evidence](../../.work-checks/evoto-2026-10-22/rea-evidence.json).

A fresh bounded REA-skill follow-up used the existing objdump fallback on the matching worker: five ranges, 23, 198 bytes, 5, 404 decoded instructions, and 25 validated literal references. It found reference-person/landmark/mask prerequisites, conditional size handling, semantic protection paths, body-skin instance masks, an optional LUT path, and an entry-format gate whose tag meanings remain unknown. These are wrapper/composition facts, not recovered numerical filters or trained networks. The older different-worker decompilation was excluded. [Exact findings and limits](AURA-Evoto-evidence-2026-10-22.md).

The optional first-photo variant uses the earlier watermarked Evoto preview as an explicitly labelled display reference. Its AURA matching distances are optimization diagnostics, not Evoto fidelity measurements. The screenshot retains its watermark and unknown display colour transform. The primary qualified recipe was restored after rendering the separate variant.

## Scope of the results

Tests use CARGO_INCREMENTAL=0 and the documented AURA_PERF_HOST_SCALE=6. The first incremental attempt failed to link with undefined LLVM anonymous symbols; the nonincremental full run is the final native result. A frequency-filter fixture caught a DC-removal error during development and was corrected. The first desktop suites found a stale 50-workflow label despite 75 available actions; the label now derives its count from the catalogue, with a targeted regression check. Initial failed logs remain retained. The Windows desktop linker also emitted a duplicate-manifest warning; the executable built and was launched for the recorded desktop checks.

After the final desktop build, strict linting required fixture-only edits in three files: split an oversized test, use contains for a membership assertion, replace a temporary zero vector with a fixed slice, enumerate coverage weights, format a small constant, and compare bit representations for exact float-preservation assertions. The affected selection and renderer tests were recompiled and rerun. Production implementation files did not change. Earlier source manifests are preserved alongside the final manifest.

All new tools were invoked through the real native desktop on a synthetic fixture and one rendered photo proxy. Each of the 27 RAWs received enhancement, a gentle new stop-zone layer, and all 70 diagnostics; every new tool was not individually applied to every RAW. Tests do not establish subjective superiority or exhaustive parameter coverage.

Conditional methods with unchanged pixels in at least one fixture: none. Accepted saves/history are recorded separately from visible pixel effect. Native mechanism fixtures provide additional evidence.

Spatial corrections retain the 256-pixel working grid; measurements use a 512-pixel preview. Some colour methods allocate full-frame planes. Semantic trained skin/body reconstruction, training datasets, complete Evoto algorithm recovery and pixel-for-pixel parity remain unresolved.

## Per-tool functional qualification

| Group | Tool | Synthetic / real-photo pixel effect | Evoto perfection |
|---|---|---|---|
|colour|Normalized sigmoid contrast|True / True; route passed|unverified|
|colour|Exponential highlight rolloff|True / True; route passed|unverified|
|colour|Monotone Hermite contrast|True / True; route passed|unverified|
|colour|Rational light bias|True / True; route passed|unverified|
|colour|Luminance solarization|True / True; route passed|unverified|
|colour|Stop-zone exposure lift|True / True; route passed|unverified|
|colour|Linear black-floor subtraction|True / True; route passed|unverified|
|colour|Selected dark-channel haze removal|True / True; route passed|unverified|
|colour|Variance-guided luminance cleanup|True / True; route passed|unverified|
|colour|Local extrema tone contraction|True / True; route passed|unverified|
|colour|Opponent-plane colour shear|True / True; route passed|unverified|
|colour|Radius-dependent hue spiral|True / True; route passed|unverified|
|colour|Luminance-dependent hue twist|True / True; route passed|unverified|
|colour|Complementary duotone gradient|True / True; route passed|unverified|
|colour|Cyclic channel crossfeed|True / True; route passed|unverified|
|colour|Subtractive colour density|True / True; route passed|unverified|
|colour|Bradford cone-response adaptation|True / True; route passed|unverified|
|colour|RGB channel-ratio limiter|True / True; route passed|unverified|
|colour|Median-channel spill suppression|True / True; route passed|unverified|
|colour|Three-axis hue harmony|True / True; route passed|unverified|
|colour|Colour-luminance regression removal|True / True; route passed|unverified|
|colour|Luminance-guided colour cleanup|True / True; route passed|unverified|
|colour|Weighted median colour repair|True / True; route passed|unverified|
|colour|Channel-distribution alignment|True / True; route passed|unverified|
|colour|Range-aware bilateral colour cleanup|True / True; route passed|unverified|
|spatial|Adaptive impulse median|True / True; route passed|unverified|
|spatial|Sigma-gated colour smoothing|True / True; route passed|unverified|
|spatial|Distance-weighted colour median|True / True; route passed|unverified|
|spatial|Luminance percentile limiter|True / True; route passed|unverified|
|spatial|Chroma range smoothing|True / True; route passed|unverified|
|spatial|Chroma covariance denoising|True / True; route passed|unverified|
|spatial|Colour covariance denoising|True / True; route passed|unverified|
|spatial|Entropy-weighted detail|True / True; route passed|unverified|
|spatial|Local RMS contrast|True / True; route passed|unverified|
|spatial|Log-illumination correction|True / True; route passed|unverified|
|spatial|Oriented periodic notch|True / True; route passed|unverified|
|spatial|Cross-footprint median|True / True; route passed|unverified|
|spatial|Curvature ridge detail|True / True; route passed|unverified|
|spatial|Coherent structure detail|True / True; route passed|unverified|
|spatial|Least-dispersion line median|True / True; route passed|unverified|
|spatial|Curved-surface residual detail|True / True; route passed|unverified|
|spatial|Windowed-sinc selection shift|True / True; route passed|unverified|
|spatial|Elliptical aperture integration|True / True; route passed|unverified|
|spatial|Hexagonal aperture integration|True / True; route passed|unverified|
|spatial|Cat-eye aperture integration|True / True; route passed|unverified|
|spatial|One-sided highlight streak|True / True; route passed|unverified|
|spatial|Range-aware base compression|True / True; route passed|unverified|
|spatial|Selected dark-channel haze repair|True / True; route passed|unverified|
|spatial|Connected spot repair|True / True; route passed|unverified|
|spatial|Luminance-preserving chroma median|True / True; route passed|unverified|
|selection|CIE Lab sampled distance|True / True; route passed|unverified|
|selection|Covariance colour distance|True / True; route passed|unverified|
|selection|Geodesic colour propagation|True / True; route passed|unverified|
|selection|Local Otsu threshold|True / True; route passed|unverified|
|selection|Sauvola adaptive threshold|True / True; route passed|unverified|
|selection|Gabor frequency response|True / True; route passed|unverified|
|selection|Directional bright ridges|True / True; route passed|unverified|
|selection|Local tonal skewness|True / True; route passed|unverified|
|selection|Local tonal kurtosis|True / True; route passed|unverified|
|selection|Colour-line residual|True / True; route passed|unverified|
|selection|Grayscale morphological opening|True / True; route passed|unverified|
|selection|Grayscale morphological closing|True / True; route passed|unverified|
|selection|Opening by reconstruction|True / True; route passed|unverified|
|selection|Grayscale area opening|True / True; route passed|unverified|
|selection|Keep border-connected regions|True / True; route passed|unverified|
|selection|Seed geodesic distance bands|True / True; route passed|unverified|
|selection|Keep regions enclosing holes|True / True; route passed|unverified|
|selection|Regional coverage maxima|True / True; route passed|unverified|
|selection|Archimedean spiral ribbon|True / True; route passed|unverified|
|selection|Barycentric triangle gradient|True / True; route passed|unverified|
|selection|Affine selection shear|True / True; route passed|unverified|
|selection|Radial selection twirl|True / True; route passed|unverified|
|selection|Spherical selection remap|True / True; route passed|unverified|
|selection|Image-gradient displacement|True / True; route passed|unverified|
|selection|Signed-distance edge smoothing|True / True; route passed|unverified|
|inspection|Luminance quantile landmarks|read-only; route passed|unverified|
|inspection|Robust luminance dispersion|read-only; route passed|unverified|
|inspection|Light energy inequality|read-only; route passed|unverified|
|inspection|Circular hue concentration|read-only; route passed|unverified|
|inspection|Hue drift through light levels|read-only; route passed|unverified|
|inspection|Channel highlight headroom|read-only; route passed|unverified|
|inspection|Absolute chroma tails|read-only; route passed|unverified|
|inspection|Light averaging models|read-only; route passed|unverified|
|inspection|Local colour transitions|read-only; route passed|unverified|
|inspection|Upper quantile spacing|read-only; route passed|unverified|
|inspection|Working range excursion energy|read-only; route passed|unverified|
|inspection|Colour spatial variogram|read-only; route passed|unverified|
|inspection|Regional structure coherence|read-only; route passed|unverified|
|inspection|Surface curvature classes|read-only; route passed|unverified|
|inspection|Gradient tail weight|read-only; route passed|unverified|
|inspection|Signed edge transitions|read-only; route passed|unverified|
|inspection|Relative total variation|read-only; route passed|unverified|
|inspection|Cosine frequency energy|read-only; route passed|unverified|
|inspection|Chroma and light edge alignment|read-only; route passed|unverified|
|inspection|Fine residual polarity|read-only; route passed|unverified|
|inspection|Regional tone entropy|read-only; route passed|unverified|
|inspection|Linear lighting trend|read-only; route passed|unverified|
|inspection|Quadratic lighting trend|read-only; route passed|unverified|
|inspection|Edge topology|read-only; route passed|unverified|
|inspection|Edge occupancy scaling|read-only; route passed|unverified|
