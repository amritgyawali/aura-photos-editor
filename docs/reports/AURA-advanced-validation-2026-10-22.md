# AURA advanced implementation and validation - 2026-10-22

Publication copy omits the embedded reference-photo thumbnail. Full photo evidence remains local and is excluded from Git. Publication preparation removed embedded photo thumbnails and updated scripts/check-banned.sh to recognize dedicated tests.rs modules. Application code and the qualified executable were unchanged; the source-manifest match describes the qualification snapshot before this checker-only change.

Added 100 further independent capabilities: 25 colour methods, 25 spatial methods, 25 selections and 25 read-only diagnostics. Each editing catalogue now contains 100 methods; inspection contains 95 measurements. Texture fidelity, Colour stability and Delivery integrity add three review screens.

**Complete Evoto reconstruction, proprietary model recovery, exact tone/output parity and per-tool perfection have not been achieved.** Functional passes below do not measure equivalence to Evoto.

| Check | Actual result |
|---|---|
| Initial complete native suite | 4552 passed, one architecture-guard failure, 53 ignored; corrected below |
| Corrected architecture target | 6 passed, 0 failed; canonical matrix imports accepted, local definitions rejected; no unresolved native failures |
| Final changed-library retest | 834 passed, 0 failed, 5 ignored |
| Frontend | 1051 tests in 111 files passed |
| Strict Clippy, formatting, contracts, IPC, Python tooling | Passed |
| New capability desktop tests | 100/100 synthetic and 100/100 real-photo proxy invocations; exact Undo/Redo and stale-revision guards where applicable |
| Existing capability regression | 300 previously tracked capabilities invoked in the rebuilt desktop |
| Additional photo-tool matrix | 27 supplied-photo proxies, 100 additions invoked on each |
| Supplied originals and export | 27 RAWs edited and inspected; 27 full-resolution JPEGs verified; original SHA-256 values unchanged |
| Source and executable identity | 1529 source files matched the frozen manifest at qualification; all desktop suites used the current executable; publication checker change noted above |

[Edited-photo gallery](../../output/aura-advanced-2026-10-22/index.html) | [Per-tool CSV](AURA-advanced-validation-2026-10-22-tools.csv) | [Machine report](AURA-advanced-validation-2026-10-22.json) | [Native cases](AURA-advanced-validation-2026-10-22-native-tests.csv)

## Existing-tool improvements

Colour methods now evaluate selected local stencil targets while blending, avoiding the retained full-frame RGB target plane. The relevant implementation ledger and native fixtures identify exact applicability and allocation limits. Spatial repair adds a bounded native-resolution path for supported local operators, preserving small selections that disappeared on the former working grid; global/geometric methods retain an explicitly documented reduced grid. Selection controls validate only parameters used by the active method, and resource preflight happens before source loading. Existing horizontal autocorrelation now centres each valid lagged support separately, making an affine ramp correlate correctly. Renderer identity advanced to labs.selected-v5 so cached older output is invalidated.

See [implementation and limits](../advanced-editing-methods-2026-10-22.md) and the three mechanism ledgers for the exact changed operators. Improvements apply to documented mechanisms; every existing tool was not individually redesigned.

## Evoto investigation

REA MCP inspected the current worker section layout and retained evidence ev_3f7a60e930868de6e68156879bfc76e697535b837a336fb0be273ea659b61f54. Its worker SHA-256 remains add7588f94aed190b97076f949709ba9622aa14f046b661783135d838d3d3bb2. The refreshed inventory hashed all 1860 regular files (4873227287 bytes); vendor consistency results: {'match': 1854}.

All 621 resources with a .json extension were examined by a strict UTF-8 JSON parser: {'valid_utf8_json': 3, 'opaque_or_non_json': 618}. Opaque content does not establish its encoding, encryption, tensor shapes, learned weights or algorithm. No inspected code, model, configuration values or image assets were incorporated into AURA. Deep native analysis remains unavailable because no supported decompiler is configured. [REA evidence](../../.work-checks/evoto-advanced-2026-10-22/rea-evidence.json), [provider status](../../.work-checks/evoto-advanced-2026-10-22/provider-status.json), [file inventory](../../.work-checks/evoto-advanced-2026-10-22/file-inventory.json), [configuration classification](../../.work-checks/evoto-advanced-2026-10-22/configuration-classification.json).

## Limits of the checks

Automated tests establish selected functional mechanisms, serialization, finite outputs and the exercised history/control paths. They do not establish subjective superiority or exhaustive parameter coverage. The photo-tool matrix uses rendered proxies, not full-resolution RAW invocations of every method. RAW exports use native automatic enhancement plus the explicit gentle colour method recorded per photo; creative effects were not stacked indiscriminately. Inspection remains limited to a 512-pixel preview. Some global spatial methods use a reduced working grid; new local native-resolution operators have bounded scratch and halo limits. Trained semantic face/body model reconstruction, matching training data and full Evoto algorithm recovery remain unresolved. AURA is open with the qualified catalogue.

## Per-tool functional results

An invocation/history pass does not require a visible change. Conditional operators can be neutral on a fixture; the following pixel-effect observation is separate from the route check and from numerical mechanism tests. No-op routes are not labelled as demonstrated effects.

The full workspace build started during boundary review. The final changed-library rerun covers the frozen implementation and final regression fixtures. Initial incomplete-module attempts, pre-final build records and source manifests remain retained. The first scenario startup was stopped before qualifying any cases when artifact timestamps exposed a renderer compiled before the last fixes; the qualified executable was then rebuilt and all scenarios restarted. The Windows linker retained its duplicate-resource-manifest warning.

The complete workspace run found one false positive in the lexical colour-space guard: the new perceptual helper already imports the single canonical matrix from aura-raw, but the guard allowed its symbol only in two historical filenames. The corrected guard checks canonical imports and rejects local const/static/let redefinitions, with positive and negative fixtures. All six architecture cases and scoped strict Clippy passed. Only the integration test and report generator changed afterward; the qualified executable and every production implementation stayed identical. The original failed workspace log is retained, and the report does not describe it as a single clean full-run pass.

| Iteration | Group | Method | Route/history; synthetic pixel effect | New photo matrix pixel changes | Evoto perfection |
|---|---|---|---|---|---|
| new-synthetic | colour | Measured spatial cast removal | passed; True | 27/27 | unverified |
| new-synthetic | colour | Measured radial illumination correction | passed; True | 27/27 | unverified |
| new-synthetic | colour | Gaussian patch palette transport | passed; True | 27/27 | unverified |
| new-synthetic | colour | Three-anchor palette constraint | passed; True | 27/27 | unverified |
| new-synthetic | colour | Measured circular hue redistribution | passed; True | 27/27 | unverified |
| new-synthetic | colour | Chroma marginal copula remapping | passed; True | 27/27 | unverified |
| new-synthetic | colour | Hue-dependent colourfulness balance | passed; True | 27/27 | unverified |
| new-synthetic | colour | Cluster-residual palette recolouring | passed; True | 27/27 | unverified |
| new-synthetic | colour | Measured channel-response reconstruction | passed; True | 27/27 | unverified |
| new-synthetic | colour | Kubelka-Munk pigment mixing | passed; True | 27/27 | unverified |
| new-synthetic | colour | Variance-preserving principal monochrome | passed; True | 27/27 | unverified |
| new-synthetic | colour | Two-patch discriminant monochrome | passed; True | 27/27 | unverified |
| new-synthetic | colour | Isoluminant colour-edge lighting | passed; True | 27/27 | unverified |
| new-synthetic | colour | Selected error-diffusion tones | passed; True | 27/27 | unverified |
| new-synthetic | colour | Area-modulated photographic dots | passed; True | 27/27 | unverified |
| new-synthetic | colour | Stop-domain luminance contours | passed; True | 27/27 | unverified |
| new-synthetic | colour | Analytical exposure fusion | passed; True | 27/27 | unverified |
| new-synthetic | colour | Constant-Lab-chroma lightness lift | passed; True | 27/27 | unverified |
| new-synthetic | colour | Uniform-chromaticity cast translation | passed; True | 27/27 | unverified |
| new-synthetic | colour | Robust group-lasso chroma cleanup | passed; True | 27/27 | unverified |
| new-synthetic | colour | Geometric-median cast balance | passed; True | 27/27 | unverified |
| new-synthetic | colour | Compositional-geodesic colour tint | passed; True | 27/27 | unverified |
| new-synthetic | colour | Fixed-energy lightness redistribution | passed; True | 27/27 | unverified |
| new-synthetic | colour | Dichromatic reflection subtraction | passed; True | 27/27 | unverified |
| new-synthetic | colour | Mirrored palette hue folding | passed; True | 27/27 | unverified |
| new-synthetic | spatial | Local cosine shrinkage | passed; True | 27/27 | unverified |
| new-synthetic | spatial | Biorthogonal lifting cleanup | passed; True | 27/27 | unverified |
| new-synthetic | spatial | Geometric colour median | passed; True | 27/27 | unverified |
| new-synthetic | spatial | Iterative colour mode | passed; True | 27/27 | unverified |
| new-synthetic | spatial | Pairwise robust colour location | passed; True | 27/27 | unverified |
| new-synthetic | spatial | Geodesic colour averaging | passed; True | 27/27 | unverified |
| new-synthetic | spatial | Spatial patch low-rank repair | passed; True | 27/27 | unverified |
| new-synthetic | spatial | Covariance-distance smoothing | passed; True | 27/27 | unverified |
| new-synthetic | spatial | Multiplicative Lee restoration | passed; True | 27/27 | unverified |
| new-synthetic | spatial | Frost spatial damping | passed; True | 27/27 | unverified |
| new-synthetic | spatial | Gamma-MAP speckle restoration | passed; True | 27/27 | unverified |
| new-synthetic | spatial | Fourth-order surface repair | passed; True | 27/27 | unverified |
| new-synthetic | spatial | Fractional spatial detail | passed; True | 27/27 | unverified |
| new-synthetic | spatial | Parabolic background separation | passed; True | 27/27 | unverified |
| new-synthetic | spatial | Oriented scratch repair | passed; True | 27/27 | unverified |
| new-synthetic | spatial | Quadrature phase detail | passed; True | 27/27 | unverified |
| new-synthetic | spatial | Spectral surprise detail | passed; True | 27/27 | unverified |
| new-synthetic | spatial | MAD impulse repair | passed; True | 27/27 | unverified |
| new-synthetic | spatial | Circular hue smoothing | passed; True | 27/27 | unverified |
| new-synthetic | spatial | Chroma strength median | passed; True | 27/27 | unverified |
| new-synthetic | spatial | Local white-patch adaptation | passed; True | 27/27 | unverified |
| new-synthetic | spatial | Local gray-edge adaptation | passed; True | 27/27 | unverified |
| new-synthetic | spatial | Luminance skewness balance | passed; True | 27/27 | unverified |
| new-synthetic | spatial | Tail-weighted detail | passed; True | 27/27 | unverified |
| new-synthetic | spatial | Monotonic edge limiter | passed; True | 27/27 | unverified |
| new-synthetic | selection | Robust median deviation | passed; True | 27/27 | unverified |
| new-synthetic | selection | Local binary-pattern transitions | passed; True | 27/27 | unverified |
| new-synthetic | selection | Directional patch autocorrelation | passed; True | 27/27 | unverified |
| new-synthetic | selection | Directional semivariance | passed; True | 27/27 | unverified |
| new-synthetic | selection | Log-chromaticity sample distance | passed; True | 27/27 | unverified |
| new-synthetic | selection | Gradient nonmaximum suppression | passed; True | 27/27 | unverified |
| new-synthetic | selection | Connected hysteresis edges | passed; True | 27/27 | unverified |
| new-synthetic | selection | Local Moran spatial association | passed; True | 27/27 | unverified |
| new-synthetic | selection | Sampled patch correlation | passed; True | 27/27 | unverified |
| new-synthetic | selection | Circular hue dispersion | passed; True | 27/27 | unverified |
| new-synthetic | selection | Colour prototype partition | passed; True | 27/27 | unverified |
| new-synthetic | selection | Connectivity articulation cells | passed; True | 27/27 | unverified |
| new-synthetic | selection | Selection graph core | passed; True | 27/27 | unverified |
| new-synthetic | selection | Seeded coverage watershed | passed; True | 27/27 | unverified |
| new-synthetic | selection | Anisotropic coverage diffusion | passed; True | 27/27 | unverified |
| new-synthetic | selection | Coverage curvature flow | passed; True | 27/27 | unverified |
| new-synthetic | selection | Region maximum inscribed disk | passed; True | 27/27 | unverified |
| new-synthetic | selection | Directional selected run width | passed; True | 27/27 | unverified |
| new-synthetic | selection | Thin-plate landmark deformation | passed; True | 27/27 | unverified |
| new-synthetic | selection | Mean-value cage deformation | passed; True | 27/27 | unverified |
| new-synthetic | selection | Triangulated cage deformation | passed; True | 27/27 | unverified |
| new-synthetic | selection | Conformal Mobius deformation | passed; True | 27/27 | unverified |
| new-synthetic | selection | Path-normal coverage unwrap | passed; True | 27/27 | unverified |
| new-synthetic | selection | Coverage-aware seam compression | passed; True | 27/27 | unverified |
| new-synthetic | selection | Lissajous ribbon construction | passed; True | 27/27 | unverified |
| new-synthetic | inspection | Log channel-ratio distribution | passed; read-only/workflow | read-only | unverified |
| new-synthetic | inspection | White-patch chromaticity | passed; read-only/workflow | read-only | unverified |
| new-synthetic | inspection | Gray-edge chromaticity | passed; read-only/workflow | read-only | unverified |
| new-synthetic | inspection | Opponent colourfulness proxy | passed; read-only/workflow | read-only | unverified |
| new-synthetic | inspection | Hue occupancy and largest gap | passed; read-only/workflow | read-only | unverified |
| new-synthetic | inspection | Local hue discontinuity | passed; read-only/workflow | read-only | unverified |
| new-synthetic | inspection | Chroma-light mutual information | passed; read-only/workflow | read-only | unverified |
| new-synthetic | inspection | Regional colour dispersion | passed; read-only/workflow | read-only | unverified |
| new-synthetic | inspection | Robust luminance outliers | passed; read-only/workflow | read-only | unverified |
| new-synthetic | inspection | Channel highlight dependence | passed; read-only/workflow | read-only | unverified |
| new-synthetic | inspection | Regional mean-variance fit | passed; read-only/workflow | read-only | unverified |
| new-synthetic | inspection | Connected clipping support | passed; read-only/workflow | read-only | unverified |
| new-synthetic | inspection | Clipping-weighted edge fraction | passed; read-only/workflow | read-only | unverified |
| new-synthetic | inspection | Robust strip lighting profile | passed; read-only/workflow | read-only | unverified |
| new-synthetic | inspection | Thumbnail phase sensitivity | passed; read-only/workflow | read-only | unverified |
| new-synthetic | inspection | Ordinal texture entropy | passed; read-only/workflow | read-only | unverified |
| new-synthetic | inspection | Sequence sample entropy | passed; read-only/workflow | read-only | unverified |
| new-synthetic | inspection | Quantized texture runs | passed; read-only/workflow | read-only | unverified |
| new-synthetic | inspection | Neighbour co-occurrence texture | passed; read-only/workflow | read-only | unverified |
| new-synthetic | inspection | Bright mass lacunarity | passed; read-only/workflow | read-only | unverified |
| new-synthetic | inspection | Regional corner response | passed; read-only/workflow | read-only | unverified |
| new-synthetic | inspection | Local illumination fit residual | passed; read-only/workflow | read-only | unverified |
| new-synthetic | inspection | Line spectral flatness | passed; read-only/workflow | read-only | unverified |
| new-synthetic | inspection | Neighbour Moran index | passed; read-only/workflow | read-only | unverified |
| new-synthetic | inspection | Robust local roughness | passed; read-only/workflow | read-only | unverified |
| previous100 | colour | Normalized sigmoid contrast | passed; True | not individually checked | unverified |
| previous100 | colour | Exponential highlight rolloff | passed; True | not individually checked | unverified |
| previous100 | colour | Monotone Hermite contrast | passed; True | not individually checked | unverified |
| previous100 | colour | Rational light bias | passed; True | not individually checked | unverified |
| previous100 | colour | Luminance solarization | passed; True | not individually checked | unverified |
| previous100 | colour | Stop-zone exposure lift | passed; True | not individually checked | unverified |
| previous100 | colour | Linear black-floor subtraction | passed; True | not individually checked | unverified |
| previous100 | colour | Selected dark-channel haze removal | passed; True | not individually checked | unverified |
| previous100 | colour | Variance-guided luminance cleanup | passed; True | not individually checked | unverified |
| previous100 | colour | Local extrema tone contraction | passed; True | not individually checked | unverified |
| previous100 | colour | Opponent-plane colour shear | passed; True | not individually checked | unverified |
| previous100 | colour | Radius-dependent hue spiral | passed; True | not individually checked | unverified |
| previous100 | colour | Luminance-dependent hue twist | passed; True | not individually checked | unverified |
| previous100 | colour | Complementary duotone gradient | passed; True | not individually checked | unverified |
| previous100 | colour | Cyclic channel crossfeed | passed; True | not individually checked | unverified |
| previous100 | colour | Subtractive colour density | passed; True | not individually checked | unverified |
| previous100 | colour | Bradford cone-response adaptation | passed; True | not individually checked | unverified |
| previous100 | colour | RGB channel-ratio limiter | passed; True | not individually checked | unverified |
| previous100 | colour | Median-channel spill suppression | passed; True | not individually checked | unverified |
| previous100 | colour | Three-axis hue harmony | passed; True | not individually checked | unverified |
| previous100 | colour | Colour-luminance regression removal | passed; True | not individually checked | unverified |
| previous100 | colour | Luminance-guided colour cleanup | passed; True | not individually checked | unverified |
| previous100 | colour | Weighted median colour repair | passed; True | not individually checked | unverified |
| previous100 | colour | Channel-distribution alignment | passed; True | not individually checked | unverified |
| previous100 | colour | Range-aware bilateral colour cleanup | passed; True | not individually checked | unverified |
| previous100 | spatial | Adaptive impulse median | passed; True | not individually checked | unverified |
| previous100 | spatial | Sigma-gated colour smoothing | passed; True | not individually checked | unverified |
| previous100 | spatial | Distance-weighted colour median | passed; True | not individually checked | unverified |
| previous100 | spatial | Luminance percentile limiter | passed; True | not individually checked | unverified |
| previous100 | spatial | Chroma range smoothing | passed; True | not individually checked | unverified |
| previous100 | spatial | Chroma covariance denoising | passed; True | not individually checked | unverified |
| previous100 | spatial | Colour covariance denoising | passed; True | not individually checked | unverified |
| previous100 | spatial | Entropy-weighted detail | passed; True | not individually checked | unverified |
| previous100 | spatial | Local RMS contrast | passed; True | not individually checked | unverified |
| previous100 | spatial | Log-illumination correction | passed; True | not individually checked | unverified |
| previous100 | spatial | Oriented periodic notch | passed; True | not individually checked | unverified |
| previous100 | spatial | Cross-footprint median | passed; True | not individually checked | unverified |
| previous100 | spatial | Curvature ridge detail | passed; True | not individually checked | unverified |
| previous100 | spatial | Coherent structure detail | passed; True | not individually checked | unverified |
| previous100 | spatial | Least-dispersion line median | passed; True | not individually checked | unverified |
| previous100 | spatial | Curved-surface residual detail | passed; True | not individually checked | unverified |
| previous100 | spatial | Windowed-sinc selection shift | passed; True | not individually checked | unverified |
| previous100 | spatial | Elliptical aperture integration | passed; True | not individually checked | unverified |
| previous100 | spatial | Hexagonal aperture integration | passed; True | not individually checked | unverified |
| previous100 | spatial | Cat-eye aperture integration | passed; True | not individually checked | unverified |
| previous100 | spatial | One-sided highlight streak | passed; True | not individually checked | unverified |
| previous100 | spatial | Range-aware base compression | passed; True | not individually checked | unverified |
| previous100 | spatial | Selected dark-channel haze repair | passed; True | not individually checked | unverified |
| previous100 | spatial | Connected spot repair | passed; True | not individually checked | unverified |
| previous100 | spatial | Luminance-preserving chroma median | passed; True | not individually checked | unverified |
| previous100 | selection | CIE Lab sampled distance | passed; True | not individually checked | unverified |
| previous100 | selection | Covariance colour distance | passed; True | not individually checked | unverified |
| previous100 | selection | Geodesic colour propagation | passed; True | not individually checked | unverified |
| previous100 | selection | Local Otsu threshold | passed; True | not individually checked | unverified |
| previous100 | selection | Sauvola adaptive threshold | passed; True | not individually checked | unverified |
| previous100 | selection | Gabor frequency response | passed; True | not individually checked | unverified |
| previous100 | selection | Directional bright ridges | passed; True | not individually checked | unverified |
| previous100 | selection | Local tonal skewness | passed; True | not individually checked | unverified |
| previous100 | selection | Local tonal kurtosis | passed; True | not individually checked | unverified |
| previous100 | selection | Colour-line residual | passed; True | not individually checked | unverified |
| previous100 | selection | Grayscale morphological opening | passed; True | not individually checked | unverified |
| previous100 | selection | Grayscale morphological closing | passed; True | not individually checked | unverified |
| previous100 | selection | Opening by reconstruction | passed; True | not individually checked | unverified |
| previous100 | selection | Grayscale area opening | passed; True | not individually checked | unverified |
| previous100 | selection | Keep border-connected regions | passed; True | not individually checked | unverified |
| previous100 | selection | Seed geodesic distance bands | passed; True | not individually checked | unverified |
| previous100 | selection | Keep regions enclosing holes | passed; True | not individually checked | unverified |
| previous100 | selection | Regional coverage maxima | passed; True | not individually checked | unverified |
| previous100 | selection | Archimedean spiral ribbon | passed; True | not individually checked | unverified |
| previous100 | selection | Barycentric triangle gradient | passed; True | not individually checked | unverified |
| previous100 | selection | Affine selection shear | passed; True | not individually checked | unverified |
| previous100 | selection | Radial selection twirl | passed; True | not individually checked | unverified |
| previous100 | selection | Spherical selection remap | passed; True | not individually checked | unverified |
| previous100 | selection | Image-gradient displacement | passed; True | not individually checked | unverified |
| previous100 | selection | Signed-distance edge smoothing | passed; True | not individually checked | unverified |
| previous100 | inspection | Luminance quantile landmarks | passed; read-only/workflow | not individually checked | unverified |
| previous100 | inspection | Robust luminance dispersion | passed; read-only/workflow | not individually checked | unverified |
| previous100 | inspection | Light energy inequality | passed; read-only/workflow | not individually checked | unverified |
| previous100 | inspection | Circular hue concentration | passed; read-only/workflow | not individually checked | unverified |
| previous100 | inspection | Hue drift through light levels | passed; read-only/workflow | not individually checked | unverified |
| previous100 | inspection | Channel highlight headroom | passed; read-only/workflow | not individually checked | unverified |
| previous100 | inspection | Absolute chroma tails | passed; read-only/workflow | not individually checked | unverified |
| previous100 | inspection | Light averaging models | passed; read-only/workflow | not individually checked | unverified |
| previous100 | inspection | Local colour transitions | passed; read-only/workflow | not individually checked | unverified |
| previous100 | inspection | Upper quantile spacing | passed; read-only/workflow | not individually checked | unverified |
| previous100 | inspection | Working range excursion energy | passed; read-only/workflow | not individually checked | unverified |
| previous100 | inspection | Colour spatial variogram | passed; read-only/workflow | not individually checked | unverified |
| previous100 | inspection | Regional structure coherence | passed; read-only/workflow | not individually checked | unverified |
| previous100 | inspection | Surface curvature classes | passed; read-only/workflow | not individually checked | unverified |
| previous100 | inspection | Gradient tail weight | passed; read-only/workflow | not individually checked | unverified |
| previous100 | inspection | Signed edge transitions | passed; read-only/workflow | not individually checked | unverified |
| previous100 | inspection | Relative total variation | passed; read-only/workflow | not individually checked | unverified |
| previous100 | inspection | Cosine frequency energy | passed; read-only/workflow | not individually checked | unverified |
| previous100 | inspection | Chroma and light edge alignment | passed; read-only/workflow | not individually checked | unverified |
| previous100 | inspection | Fine residual polarity | passed; read-only/workflow | not individually checked | unverified |
| previous100 | inspection | Regional tone entropy | passed; read-only/workflow | not individually checked | unverified |
| previous100 | inspection | Linear lighting trend | passed; read-only/workflow | not individually checked | unverified |
| previous100 | inspection | Quadratic lighting trend | passed; read-only/workflow | not individually checked | unverified |
| previous100 | inspection | Edge topology | passed; read-only/workflow | not individually checked | unverified |
| previous100 | inspection | Edge occupancy scaling | passed; read-only/workflow | not individually checked | unverified |
| second100 | colour | Median exposure anchor | passed; True | not individually checked | unverified |
| second100 | colour | Interquartile contrast fit | passed; True | not individually checked | unverified |
| second100 | colour | Lower-tail luminance lift | passed; True | not individually checked | unverified |
| second100 | colour | Upper-tail soft shoulder | passed; True | not individually checked | unverified |
| second100 | colour | Luminance skew correction | passed; True | not individually checked | unverified |
| second100 | colour | RMS exposure normalization | passed; True | not individually checked | unverified |
| second100 | colour | Harmonic exposure normalization | passed; True | not individually checked | unverified |
| second100 | colour | Entropy-weighted zone contrast | passed; True | not individually checked | unverified |
| second100 | colour | Sixth-moment illuminant balance | passed; True | not individually checked | unverified |
| second100 | colour | Gray-edge illuminant balance | passed; True | not individually checked | unverified |
| second100 | colour | Trimmed illuminant balance | passed; True | not individually checked | unverified |
| second100 | colour | Log-ratio illuminant balance | passed; True | not individually checked | unverified |
| second100 | colour | Dominant chroma-mode neutralization | passed; True | not individually checked | unverified |
| second100 | colour | Tonal colour-cast equalization | passed; True | not individually checked | unverified |
| second100 | colour | Covariance colour outlier clipping | passed; True | not individually checked | unverified |
| second100 | colour | Chroma-radius equalization | passed; True | not individually checked | unverified |
| second100 | colour | Sample-guided hue arc pull | passed; True | not individually checked | unverified |
| second100 | colour | Measured palette-axis reflection | passed; True | not individually checked | unverified |
| second100 | colour | Median colourfulness normalization | passed; True | not individually checked | unverified |
| second100 | colour | Highlight neighbourhood colour recovery | passed; True | not individually checked | unverified |
| second100 | colour | Variance-aware chroma coherence | passed; True | not individually checked | unverified |
| second100 | colour | Local illumination flattening | passed; True | not individually checked | unverified |
| second100 | colour | Local log-ratio colour cleanup | passed; True | not individually checked | unverified |
| second100 | colour | Circular hue diffusion | passed; True | not individually checked | unverified |
| second100 | colour | Target-hue colour compression | passed; True | not individually checked | unverified |
| second100 | spatial | Guided linear smoothing | passed; True | not individually checked | unverified |
| second100 | spatial | Recursive domain smoothing | passed; True | not individually checked | unverified |
| second100 | spatial | Iterative deconvolution | passed; True | not individually checked | unverified |
| second100 | spatial | Shock edge sharpening | passed; True | not individually checked | unverified |
| second100 | spatial | Curvature smoothing | passed; True | not individually checked | unverified |
| second100 | spatial | Bright component reconstruction | passed; True | not individually checked | unverified |
| second100 | spatial | Dark component reconstruction | passed; True | not individually checked | unverified |
| second100 | spatial | Morphological toggle contrast | passed; True | not individually checked | unverified |
| second100 | spatial | Contraharmonic cleanup | passed; True | not individually checked | unverified |
| second100 | spatial | Geometric noise smoothing | passed; True | not individually checked | unverified |
| second100 | spatial | Huber robust smoothing | passed; True | not individually checked | unverified |
| second100 | spatial | Tukey outlier rejection | passed; True | not individually checked | unverified |
| second100 | spatial | Plane residual detail | passed; True | not individually checked | unverified |
| second100 | spatial | Luminance-guided chroma repair | passed; True | not individually checked | unverified |
| second100 | spatial | Poisson variance smoothing | passed; True | not individually checked | unverified |
| second100 | spatial | Side-window smoothing | passed; True | not individually checked | unverified |
| second100 | spatial | Median-of-means repair | passed; True | not individually checked | unverified |
| second100 | spatial | Range-separated detail | passed; True | not individually checked | unverified |
| second100 | spatial | Radial diffraction halo | passed; True | not individually checked | unverified |
| second100 | spatial | Annular aperture blur | passed; True | not individually checked | unverified |
| second100 | spatial | Focus-plane motion blur | passed; True | not individually checked | unverified |
| second100 | spatial | Radial zoom integration | passed; True | not individually checked | unverified |
| second100 | spatial | Radial channel correction | passed; True | not individually checked | unverified |
| second100 | spatial | Local projective plane | passed; True | not individually checked | unverified |
| second100 | spatial | Compact radial warp | passed; True | not individually checked | unverified |
| second100 | selection | Circular hue arc | passed; True | not individually checked | unverified |
| second100 | selection | Opponent colourfulness | passed; True | not individually checked | unverified |
| second100 | selection | Red–blue balance | passed; True | not individually checked | unverified |
| second100 | selection | Channel clipping fraction | passed; True | not individually checked | unverified |
| second100 | selection | Neutral-axis proximity | passed; True | not individually checked | unverified |
| second100 | selection | Local contrast polarity | passed; True | not individually checked | unverified |
| second100 | selection | Local tonal entropy | passed; True | not individually checked | unverified |
| second100 | selection | Laplacian detail | passed; True | not individually checked | unverified |
| second100 | selection | Oriented luminance edges | passed; True | not individually checked | unverified |
| second100 | selection | Structure tensor coherence | passed; True | not individually checked | unverified |
| second100 | selection | Harris corner response | passed; True | not individually checked | unverified |
| second100 | selection | Local luminance rank | passed; True | not individually checked | unverified |
| second100 | selection | Chromatic boundaries | passed; True | not individually checked | unverified |
| second100 | selection | Neutral bright residual | passed; True | not individually checked | unverified |
| second100 | selection | Shadow chroma deviation | passed; True | not individually checked | unverified |
| second100 | selection | Superellipse marquee | passed; True | not individually checked | unverified |
| second100 | selection | Cubic Bézier ribbon | passed; True | not individually checked | unverified |
| second100 | selection | Concave star marquee | passed; True | not individually checked | unverified |
| second100 | selection | Continuous foreground/background seed field | passed; True | not individually checked | unverified |
| second100 | selection | Bilateral overlap | passed; True | not individually checked | unverified |
| second100 | selection | Periodic selection tiles | passed; True | not individually checked | unverified |
| second100 | selection | Polar coverage unwrap | passed; True | not individually checked | unverified |
| second100 | selection | Coverage quantile equalization | passed; True | not individually checked | unverified |
| second100 | selection | Harmonic coverage patch repair | passed; True | not individually checked | unverified |
| second100 | selection | Principal-axis region strip | passed; True | not individually checked | unverified |
| second100 | inspection | Centre of light | passed; read-only/workflow | not individually checked | unverified |
| second100 | inspection | Centre of detail | passed; read-only/workflow | not individually checked | unverified |
| second100 | inspection | Quadrant light balance | passed; read-only/workflow | not individually checked | unverified |
| second100 | inspection | Border light energy | passed; read-only/workflow | not individually checked | unverified |
| second100 | inspection | Thirds line detail mass | passed; read-only/workflow | not individually checked | unverified |
| second100 | inspection | Mirror asymmetry | passed; read-only/workflow | not individually checked | unverified |
| second100 | inspection | Bright region connectivity | passed; read-only/workflow | not individually checked | unverified |
| second100 | inspection | Radial light profile | passed; read-only/workflow | not individually checked | unverified |
| second100 | inspection | Curvature sign transitions | passed; read-only/workflow | not individually checked | unverified |
| second100 | inspection | Horizontal edge support | passed; read-only/workflow | not individually checked | unverified |
| second100 | inspection | Edge overshoot residual | passed; read-only/workflow | not individually checked | unverified |
| second100 | inspection | Horizontal signal autocorrelation | passed; read-only/workflow | not individually checked | unverified |
| second100 | inspection | Diagonal parity residual | passed; read-only/workflow | not individually checked | unverified |
| second100 | inspection | Directional residual correlation | passed; read-only/workflow | not individually checked | unverified |
| second100 | inspection | Decimated Haar energy | passed; read-only/workflow | not individually checked | unverified |
| second100 | inspection | Edge position covariance | passed; read-only/workflow | not individually checked | unverified |
| second100 | inspection | Boundary edge activity | passed; read-only/workflow | not individually checked | unverified |
| second100 | inspection | Channel information dependence | passed; read-only/workflow | not individually checked | unverified |
| second100 | inspection | Working RGB palette cube | passed; read-only/workflow | not individually checked | unverified |
| second100 | inspection | Circular hue energy | passed; read-only/workflow | not individually checked | unverified |
| second100 | inspection | Chroma and light distribution | passed; read-only/workflow | not individually checked | unverified |
| second100 | inspection | Neutral cast by luminance | passed; read-only/workflow | not individually checked | unverified |
| second100 | inspection | Chroma covariance | passed; read-only/workflow | not individually checked | unverified |
| second100 | inspection | Tonal asymmetry and tails | passed; read-only/workflow | not individually checked | unverified |
| second100 | inspection | Missing level runs | passed; read-only/workflow | not individually checked | unverified |
| first100 | colour | Two-neutral spatial bridge | passed; True | not individually checked | unverified |
| first100 | colour | Two-neutral tonal bridge | passed; True | not individually checked | unverified |
| first100 | colour | Sampled colour-vector rejection | passed; True | not individually checked | unverified |
| first100 | colour | Measured palette-axis projection | passed; True | not individually checked | unverified |
| first100 | colour | Covariance decorrelation stretch | passed; True | not individually checked | unverified |
| first100 | colour | Dominant chroma-axis suppression | passed; True | not individually checked | unverified |
| first100 | colour | Soft gamut-ray compression | passed; True | not individually checked | unverified |
| first100 | colour | Brightness-weighted chroma compression | passed; True | not individually checked | unverified |
| first100 | colour | Radial chroma cusp compression | passed; True | not individually checked | unverified |
| first100 | colour | Near-neutral chroma exclusion | passed; True | not individually checked | unverified |
| first100 | colour | Selected luminance equalization | passed; True | not individually checked | unverified |
| first100 | colour | Clipped-histogram equalization | passed; True | not individually checked | unverified |
| first100 | colour | Log-luminance equalization | passed; True | not individually checked | unverified |
| first100 | colour | Rank logistic normalization | passed; True | not individually checked | unverified |
| first100 | colour | Bimodal tone separation | passed; True | not individually checked | unverified |
| first100 | colour | Percentile landmark remapping | passed; True | not individually checked | unverified |
| first100 | colour | Log-average photographic mapping | passed; True | not individually checked | unverified |
| first100 | colour | Asinh dynamic-range mapping | passed; True | not individually checked | unverified |
| first100 | colour | Logarithmic toe and shoulder | passed; True | not individually checked | unverified |
| first100 | colour | Local log-contrast equalization | passed; True | not individually checked | unverified |
| first100 | colour | Selected Retinex ratio | passed; True | not individually checked | unverified |
| first100 | colour | Edge-protected colour enrichment | passed; True | not individually checked | unverified |
| first100 | colour | Chroma outlier Winsorization | passed; False | not individually checked | unverified |
| first100 | colour | Local opponent-colour contrast | passed; True | not individually checked | unverified |
| first100 | colour | Sample-bound chroma replacement | passed; True | not individually checked | unverified |
| first100 | spatial | Edge-stopped diffusion | passed; True | not individually checked | unverified |
| first100 | spatial | Total variation repair | passed; True | not individually checked | unverified |
| first100 | spatial | Patch similarity denoising | passed; True | not individually checked | unverified |
| first100 | spatial | Vector medoid cleanup | passed; True | not individually checked | unverified |
| first100 | spatial | Trimmed neighborhood repair | passed; True | not individually checked | unverified |
| first100 | spatial | Adaptive Wiener restoration | passed; True | not individually checked | unverified |
| first100 | spatial | Haar residual thresholding | passed; True | not individually checked | unverified |
| first100 | spatial | Haar detail gain | passed; True | not individually checked | unverified |
| first100 | spatial | Laplacian band contrast | passed; True | not individually checked | unverified |
| first100 | spatial | Variance-region smoothing | passed; True | not individually checked | unverified |
| first100 | spatial | Structure-guided strand smoothing | passed; True | not individually checked | unverified |
| first100 | spatial | Block seam repair | passed; True | not individually checked | unverified |
| first100 | spatial | Disc aperture bokeh | passed; True | not individually checked | unverified |
| first100 | spatial | Diffraction star filter | passed; True | not individually checked | unverified |
| first100 | spatial | Rotational motion integration | passed; True | not individually checked | unverified |
| first100 | spatial | Drawn-path motion integration | passed; True | not individually checked | unverified |
| first100 | spatial | Harmonic selection fill | passed; True | not individually checked | unverified |
| first100 | spatial | Gradient-domain donor repair | passed; True | not individually checked | unverified |
| first100 | spatial | Local channel registration | passed; True | not individually checked | unverified |
| first100 | spatial | Bright/dark structure cleanup | passed; True | not individually checked | unverified |
| first100 | spatial | Local rank contrast | passed; True | not individually checked | unverified |
| first100 | spatial | Edge-adjacent noise repair | passed; False | not individually checked | unverified |
| first100 | spatial | Directional edge interpolation | passed; True | not individually checked | unverified |
| first100 | spatial | Donor-guided low-frequency repair | passed; True | not individually checked | unverified |
| first100 | spatial | Measured texture patch quilting | passed; True | not individually checked | unverified |
| first100 | selection | Polygon marquee | passed; True | not individually checked | unverified |
| first100 | selection | Rounded rectangle marquee | passed; True | not individually checked | unverified |
| first100 | selection | Angular sector marquee | passed; True | not individually checked | unverified |
| first100 | selection | Sampled global colour | passed; True | not individually checked | unverified |
| first100 | selection | Contiguous colour wand | passed; True | not individually checked | unverified |
| first100 | selection | Sampled luminance band | passed; True | not individually checked | unverified |
| first100 | selection | Saturation range | passed; True | not individually checked | unverified |
| first100 | selection | Texture range | passed; True | not individually checked | unverified |
| first100 | selection | Image edge selection | passed; True | not individually checked | unverified |
| first100 | selection | Image-guided mask smoothing | passed; True | not individually checked | unverified |
| first100 | selection | Edge-constrained growth | passed; True | not individually checked | unverified |
| first100 | selection | Interior distance falloff | passed; True | not individually checked | unverified |
| first100 | selection | Enclosed negative-space selection | passed; True | not individually checked | unverified |
| first100 | selection | Selection centreline | passed; True | not individually checked | unverified |
| first100 | selection | Split disconnected regions into masks | passed; True | not individually checked | unverified |
| first100 | selection | Append whole component stack | passed; True | not individually checked | unverified |
| first100 | selection | Replace destination selection | passed; True | not individually checked | unverified |
| first100 | selection | Synchronize local adjustments | passed; True | not individually checked | unverified |
| first100 | selection | Align selection centroids | passed; True | not individually checked | unverified |
| first100 | selection | Fit selection to bounds | passed; True | not individually checked | unverified |
| first100 | selection | Four-corner perspective placement | passed; True | not individually checked | unverified |
| first100 | selection | Automatic coverage threshold | passed; True | not individually checked | unverified |
| first100 | selection | Sampled chroma-key exclusion | passed; True | not individually checked | unverified |
| first100 | selection | Seed partition of selection | passed; True | not individually checked | unverified |
| first100 | selection | Filter regions by shape and area | passed; True | not individually checked | unverified |
| first100 | workflow | Multi-photo recipe checkpoint | passed; read-only/workflow | not individually checked | unverified |
| first100 | workflow | Checkpoint drift comparison | passed; read-only/workflow | not individually checked | unverified |
| first100 | workflow | Section-selective checkpoint restoration | passed; read-only/workflow | not individually checked | unverified |
| first100 | workflow | Conflict-aware rollback preview | passed; read-only/workflow | not individually checked | unverified |
| first100 | workflow | Relative edit transfer | passed; read-only/workflow | not individually checked | unverified |
| first100 | workflow | Batch change manifest | passed; read-only/workflow | not individually checked | unverified |
| first100 | workflow | Baseline-aware three-way transfer | passed; read-only/workflow | not individually checked | unverified |
| first100 | workflow | Batch saved-revision verification | passed; read-only/workflow | not individually checked | unverified |
| first100 | workflow | Edit fingerprint cohorts | passed; read-only/workflow | not individually checked | unverified |
| first100 | workflow | Cross-photo settings-section matrix | passed; read-only/workflow | not individually checked | unverified |
| first100 | workflow | Durable ordered review queue | passed; read-only/workflow | not individually checked | unverified |
| first100 | workflow | Revision-bound custom checklist signoff | passed; read-only/workflow | not individually checked | unverified |
| first100 | workflow | Approval drift tracking | passed; read-only/workflow | not individually checked | unverified |
| first100 | workflow | Photo-coordinate correction issues | passed; read-only/workflow | not individually checked | unverified |
| first100 | workflow | Correction prerequisite gating | passed; read-only/workflow | not individually checked | unverified |
| first100 | workflow | Batch correction propagation | passed; read-only/workflow | not individually checked | unverified |
| first100 | workflow | Local reviewer workload assignment | passed; read-only/workflow | not individually checked | unverified |
| first100 | workflow | Due-date review scheduling | passed; read-only/workflow | not individually checked | unverified |
| first100 | workflow | Saved-revision pair preference | passed; read-only/workflow | not individually checked | unverified |
| first100 | workflow | Unresolved comparison review | passed; read-only/workflow | not individually checked | unverified |
| first100 | workflow | Saved compound review filters | passed; read-only/workflow | not individually checked | unverified |
| first100 | workflow | Checklist-gated handoff manifest | passed; read-only/workflow | not individually checked | unverified |
| first100 | workflow | Portable review metadata export | passed; read-only/workflow | not individually checked | unverified |
| first100 | workflow | Validated review metadata import | passed; read-only/workflow | not individually checked | unverified |
| first100 | workflow | Durable workflow activity history | passed; read-only/workflow | not individually checked | unverified |
