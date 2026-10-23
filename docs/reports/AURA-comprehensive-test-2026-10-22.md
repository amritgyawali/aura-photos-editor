# AURA comprehensive desktop and photo test - 2026-10-22

**AURA was run and tested; exact Evoto parity and per-tool perfection remain unverified.**

| Check | Fresh result |
|---|---|
| Full native workspace, including doc tests | 4563 passed; 0 failed; 53 ignored |
| Frontend | 1051 tests in 111 files passed; production build passed |
| Tracked capability suites | All 400 ledger entries passed in the final desktop |
| Lab editing methods | All 300 exercised individually on synthetic pixels and a supplied RAW-derived photo proxy |
| Older tools and controls | 271 core cases passed; overlap with diagnostics is deduplicated below |
| Visible workflow | 11 GUI steps passed, including import, skin cleanup, batch editing, manual edits, Undo/Redo and verified export |
| Supplied CR3 photos | 27/27 saved edits reopened with identical rendered pixels; original SHA-256 values unchanged |
| Final full-resolution JPEG export | 27 written; 27 verified; 0 render failures |
| AURA recipe sidecars | 27 written; each exactly matches its saved edit |
| Other export formats | JPEG, lossless PNG renderer equality, and 16-bit TIFF passed on a real-photo proxy |
| Static IPC surface | 347 definitions, registrations and typed calls agree |
| Direct desktop command coverage | 25/347 command names observed; unexercised commands remain explicit |
| Checks | Scoped strict Clippy, formatting, contracts, banned patterns and Python tooling passed |

[Before/after gallery and full-resolution exports](../../output/aura-comprehensive-sidecars-retry-final-2026-10-22/index.html) | [Per-tool CSV](AURA-comprehensive-test-2026-10-22-tools.csv) | [Per-command coverage](AURA-comprehensive-test-2026-10-22-commands.csv) | [Native test cases](AURA-comprehensive-test-2026-10-22-native-tests.csv) | [Machine-readable evidence](AURA-comprehensive-test-2026-10-22.json)

## Repairs

Repeatedly applying unchanged Levels, selective colour or local colour controls created redundant Undo entries. The command now validates the request, compares decoded settings and checks canonical recipe precision before writing. The regression checks identical and subprecision settings, stale writes, exact Undo/Redo and original-file safety. All 13 studio-colour desktop cases were rerun in the rebuilt application.

Requested export sidecars were silently omitted because the export source did not provide recipe data. Each rendered image now carries its exact canonical recipe through resizing and sharpening. Delivery facts are retained ahead of optional processing messages so the sidecar count remains accurate. Parallel exporter tests, the real app workflow and all 27 final photo sidecars verify the repair. These files retain AURA canonical recipe JSON with the existing .xmp extension; Adobe/Evoto XMP compatibility is unverified.

## Coverage and quality limits

The photo edits use AURA automatic enhancement plus the explicit gentle perceptual-lightness layer recorded in each recipe. Creative effects were tested individually and undone. Their successful invocation does not mean each is suitable for every portrait. The final executable reopened a backup of the qualified catalogue, reproduced all saved 1600-pixel preview bytes, ran 95 diagnostics per photo without modifying history, and produced a fresh complete export.

A functional pass means the exercised operation completed and its recorded invariants held. Visible effects, history checks and read-only diagnostics are separate columns. No percentage is assigned to "perfection" because there is no validated ground truth for that score. The complete parameter space, every camera format, physical tethering, live cloud services and all 347 commands were not individually exercised. Native ignored tests are listed with their reasons.

The supplied-photo proxy contains no sky and the sky selection correctly returned an explanatory refusal. Facial-hair selection coverage was empty on that portrait; a visible beard-selection effect was not demonstrated. Some global spatial methods retain reduced working grids, and diagnostics use a 512-pixel preview. The tested executable is an unoptimized Windows debug build; concurrent run times are not release benchmarks.

Several earlier desktop sessions ended with TargetClosedError, including a concurrent export and method suite. Their logs and incomplete results remain retained; the cause has not been established. Fresh final scenarios are reported separately. An initial formatting failure was corrected. A queued duplicate regression command was stopped before execution; it is not counted as a product failure or a test pass. The initial native workspace run reproduced the history defect and was stopped after the corrected target passed. The complete corrected-source run supersedes that partial earlier attempt; only the complete final run supplies the native totals above.

Report review also corrected method matching to use both group and ID, and retained method names containing digits. The final source manifest includes both product repairs and all qualification scripts. Earlier attempts are retained separately; reused checks are identified in qualification provenance. Final retry scenarios use one desktop session at a time.

## Evoto comparison

Evoto documents semantic skin retouching, local and batch colour matching, and a broader 8.0 workflow. These documents establish the reference feature categories; they do not specify the numerical algorithms or establish output equivalence. [Skin retouching](https://support.evoto.ai/skin-retouching/), [AI Color Match](https://support.evoto.ai/feature-introduction-ai-color-match/), [Colour and tone](https://www.evoto.ai/features/color-and-tone), [Evoto 8.0](https://www.evoto.ai/launch26).

AURA retains unresolved differences in trained semantic corrections, model-dependent generative tools, RAW/colour interpretation and subject-specific retouching. No paired Evoto export with identical settings and colour management was produced for this run. Exact matching and every Evoto feature remain unfinished. Existing historical reference investigations remain linked from the earlier reports and are not presented as fresh measurements.

## Tool-by-tool results

| Group | Qualified rows |
|---|---|
| colour | 100 |
| colour-measurement | 4 |
| export | 3 |
| inspection | 95 |
| local-parameter | 11 |
| material | 15 |
| parameter | 32 |
| reference-library | 14 |
| reference-matching | 1 |
| retouch | 42 |
| selection | 100 |
| selection-management | 27 |
| semantic-selection | 14 |
| spatial | 100 |
| studio-colour | 13 |
| workflow | 25 |

These rows include individual controls and validation workflows, and are not a count of independent algorithms.

| Group | Tool/control | Route | Synthetic effect | Real-photo proxy effect | Evoto perfection |
|---|---|---|---|---|---|
| colour | Two-neutral spatial bridge | passed | changed | changed | unverified |
| colour | Two-neutral tonal bridge | passed | changed | changed | unverified |
| colour | Sampled colour-vector rejection | passed | changed | changed | unverified |
| colour | Measured palette-axis projection | passed | changed | changed | unverified |
| colour | Covariance decorrelation stretch | passed | changed | changed | unverified |
| colour | Dominant chroma-axis suppression | passed | changed | changed | unverified |
| colour | Soft gamut-ray compression | passed | changed | changed | unverified |
| colour | Brightness-weighted chroma compression | passed | changed | changed | unverified |
| colour | Radial chroma cusp compression | passed | changed | changed | unverified |
| colour | Near-neutral chroma exclusion | passed | changed | changed | unverified |
| colour | Selected luminance equalization | passed | changed | changed | unverified |
| colour | Clipped-histogram equalization | passed | changed | changed | unverified |
| colour | Log-luminance equalization | passed | changed | changed | unverified |
| colour | Rank logistic normalization | passed | changed | changed | unverified |
| colour | Bimodal tone separation | passed | changed | changed | unverified |
| colour | Percentile landmark remapping | passed | changed | changed | unverified |
| colour | Log-average photographic mapping | passed | changed | changed | unverified |
| colour | Asinh dynamic-range mapping | passed | changed | changed | unverified |
| colour | Logarithmic toe and shoulder | passed | changed | changed | unverified |
| colour | Local log-contrast equalization | passed | changed | changed | unverified |
| colour | Selected Retinex ratio | passed | changed | changed | unverified |
| colour | Edge-protected colour enrichment | passed | changed | changed | unverified |
| colour | Chroma outlier Winsorization | passed | changed | changed | unverified |
| colour | Local opponent-colour contrast | passed | changed | changed | unverified |
| colour | Sample-bound chroma replacement | passed | changed | changed | unverified |
| spatial | Edge-stopped diffusion | passed | changed | changed | unverified |
| spatial | Total variation repair | passed | changed | changed | unverified |
| spatial | Patch similarity denoising | passed | changed | changed | unverified |
| spatial | Vector medoid cleanup | passed | changed | changed | unverified |
| spatial | Trimmed neighborhood repair | passed | changed | changed | unverified |
| spatial | Adaptive Wiener restoration | passed | changed | changed | unverified |
| spatial | Haar residual thresholding | passed | changed | changed | unverified |
| spatial | Haar detail gain | passed | changed | changed | unverified |
| spatial | Laplacian band contrast | passed | changed | changed | unverified |
| spatial | Variance-region smoothing | passed | changed | changed | unverified |
| spatial | Structure-guided strand smoothing | passed | changed | changed | unverified |
| spatial | Block seam repair | passed | changed | changed | unverified |
| spatial | Disc aperture bokeh | passed | changed | changed | unverified |
| spatial | Diffraction star filter | passed | changed | changed | unverified |
| spatial | Rotational motion integration | passed | changed | changed | unverified |
| spatial | Drawn-path motion integration | passed | changed | changed | unverified |
| spatial | Harmonic selection fill | passed | changed | changed | unverified |
| spatial | Gradient-domain donor repair | passed | changed | changed | unverified |
| spatial | Local channel registration | passed | changed | changed | unverified |
| spatial | Bright/dark structure cleanup | passed | changed | changed | unverified |
| spatial | Local rank contrast | passed | changed | changed | unverified |
| spatial | Edge-adjacent noise repair | passed | changed | changed | unverified |
| spatial | Directional edge interpolation | passed | changed | changed | unverified |
| spatial | Donor-guided low-frequency repair | passed | changed | changed | unverified |
| spatial | Measured texture patch quilting | passed | changed | changed | unverified |
| selection | Polygon marquee | passed | changed | changed | unverified |
| selection | Rounded rectangle marquee | passed | changed | changed | unverified |
| selection | Angular sector marquee | passed | changed | changed | unverified |
| selection | Sampled global colour | passed | changed | changed | unverified |
| selection | Contiguous colour wand | passed | changed | changed | unverified |
| selection | Sampled luminance band | passed | changed | changed | unverified |
| selection | Saturation range | passed | changed | changed | unverified |
| selection | Texture range | passed | changed | changed | unverified |
| selection | Image edge selection | passed | changed | changed | unverified |
| selection | Image-guided mask smoothing | passed | changed | changed | unverified |
| selection | Edge-constrained growth | passed | changed | changed | unverified |
| selection | Interior distance falloff | passed | changed | changed | unverified |
| selection | Enclosed negative-space selection | passed | changed | changed | unverified |
| selection | Selection centreline | passed | changed | changed | unverified |
| selection | Split disconnected regions into masks | passed | changed | changed | unverified |
| selection | Append whole component stack | passed | changed | changed | unverified |
| selection | Replace destination selection | passed | changed | changed | unverified |
| selection | Synchronize local adjustments | passed | changed | changed | unverified |
| selection | Align selection centroids | passed | changed | changed | unverified |
| selection | Fit selection to bounds | passed | changed | changed | unverified |
| selection | Four-corner perspective placement | passed | changed | changed | unverified |
| selection | Automatic coverage threshold | passed | changed | changed | unverified |
| selection | Sampled chroma-key exclusion | passed | changed | changed | unverified |
| selection | Seed partition of selection | passed | changed | changed | unverified |
| selection | Filter regions by shape and area | passed | changed | changed | unverified |
| workflow | Multi-photo recipe checkpoint | passed | read-only or workflow; see evidence | not tested in this scenario | unverified |
| workflow | Checkpoint drift comparison | passed | read-only or workflow; see evidence | not tested in this scenario | unverified |
| workflow | Section-selective checkpoint restoration | passed | read-only or workflow; see evidence | not tested in this scenario | unverified |
| workflow | Conflict-aware rollback preview | passed | read-only or workflow; see evidence | not tested in this scenario | unverified |
| workflow | Relative edit transfer | passed | read-only or workflow; see evidence | not tested in this scenario | unverified |
| workflow | Batch change manifest | passed | read-only or workflow; see evidence | not tested in this scenario | unverified |
| workflow | Baseline-aware three-way transfer | passed | read-only or workflow; see evidence | not tested in this scenario | unverified |
| workflow | Batch saved-revision verification | passed | read-only or workflow; see evidence | not tested in this scenario | unverified |
| workflow | Edit fingerprint cohorts | passed | read-only or workflow; see evidence | not tested in this scenario | unverified |
| workflow | Cross-photo settings-section matrix | passed | read-only or workflow; see evidence | not tested in this scenario | unverified |
| workflow | Durable ordered review queue | passed | read-only or workflow; see evidence | not tested in this scenario | unverified |
| workflow | Revision-bound custom checklist signoff | passed | read-only or workflow; see evidence | not tested in this scenario | unverified |
| workflow | Approval drift tracking | passed | read-only or workflow; see evidence | not tested in this scenario | unverified |
| workflow | Photo-coordinate correction issues | passed | read-only or workflow; see evidence | not tested in this scenario | unverified |
| workflow | Correction prerequisite gating | passed | read-only or workflow; see evidence | not tested in this scenario | unverified |
| workflow | Batch correction propagation | passed | read-only or workflow; see evidence | not tested in this scenario | unverified |
| workflow | Local reviewer workload assignment | passed | read-only or workflow; see evidence | not tested in this scenario | unverified |
| workflow | Due-date review scheduling | passed | read-only or workflow; see evidence | not tested in this scenario | unverified |
| workflow | Saved-revision pair preference | passed | read-only or workflow; see evidence | not tested in this scenario | unverified |
| workflow | Unresolved comparison review | passed | read-only or workflow; see evidence | not tested in this scenario | unverified |
| workflow | Saved compound review filters | passed | read-only or workflow; see evidence | not tested in this scenario | unverified |
| workflow | Checklist-gated handoff manifest | passed | read-only or workflow; see evidence | not tested in this scenario | unverified |
| workflow | Portable review metadata export | passed | read-only or workflow; see evidence | not tested in this scenario | unverified |
| workflow | Validated review metadata import | passed | read-only or workflow; see evidence | not tested in this scenario | unverified |
| workflow | Durable workflow activity history | passed | read-only or workflow; see evidence | not tested in this scenario | unverified |
| colour | Median exposure anchor | passed | changed | changed | unverified |
| colour | Interquartile contrast fit | passed | changed | changed | unverified |
| colour | Lower-tail luminance lift | passed | changed | changed | unverified |
| colour | Upper-tail soft shoulder | passed | changed | changed | unverified |
| colour | Luminance skew correction | passed | changed | changed | unverified |
| colour | RMS exposure normalization | passed | changed | changed | unverified |
| colour | Harmonic exposure normalization | passed | changed | changed | unverified |
| colour | Entropy-weighted zone contrast | passed | changed | changed | unverified |
| colour | Sixth-moment illuminant balance | passed | changed | changed | unverified |
| colour | Gray-edge illuminant balance | passed | changed | changed | unverified |
| colour | Trimmed illuminant balance | passed | changed | changed | unverified |
| colour | Log-ratio illuminant balance | passed | changed | changed | unverified |
| colour | Dominant chroma-mode neutralization | passed | changed | changed | unverified |
| colour | Tonal colour-cast equalization | passed | changed | changed | unverified |
| colour | Covariance colour outlier clipping | passed | changed | changed | unverified |
| colour | Chroma-radius equalization | passed | changed | changed | unverified |
| colour | Sample-guided hue arc pull | passed | changed | changed | unverified |
| colour | Measured palette-axis reflection | passed | changed | changed | unverified |
| colour | Median colourfulness normalization | passed | changed | changed | unverified |
| colour | Highlight neighbourhood colour recovery | passed | changed | changed | unverified |
| colour | Variance-aware chroma coherence | passed | changed | changed | unverified |
| colour | Local illumination flattening | passed | changed | changed | unverified |
| colour | Local log-ratio colour cleanup | passed | changed | changed | unverified |
| colour | Circular hue diffusion | passed | changed | changed | unverified |
| colour | Target-hue colour compression | passed | changed | changed | unverified |
| spatial | Guided linear smoothing | passed | changed | changed | unverified |
| spatial | Recursive domain smoothing | passed | changed | changed | unverified |
| spatial | Iterative deconvolution | passed | changed | changed | unverified |
| spatial | Shock edge sharpening | passed | changed | changed | unverified |
| spatial | Curvature smoothing | passed | changed | changed | unverified |
| spatial | Bright component reconstruction | passed | changed | changed | unverified |
| spatial | Dark component reconstruction | passed | changed | changed | unverified |
| spatial | Morphological toggle contrast | passed | changed | changed | unverified |
| spatial | Contraharmonic cleanup | passed | changed | changed | unverified |
| spatial | Geometric noise smoothing | passed | changed | changed | unverified |
| spatial | Huber robust smoothing | passed | changed | changed | unverified |
| spatial | Tukey outlier rejection | passed | changed | changed | unverified |
| spatial | Plane residual detail | passed | changed | changed | unverified |
| spatial | Luminance-guided chroma repair | passed | changed | changed | unverified |
| spatial | Poisson variance smoothing | passed | changed | changed | unverified |
| spatial | Side-window smoothing | passed | changed | changed | unverified |
| spatial | Median-of-means repair | passed | changed | changed | unverified |
| spatial | Range-separated detail | passed | changed | changed | unverified |
| spatial | Radial diffraction halo | passed | changed | changed | unverified |
| spatial | Annular aperture blur | passed | changed | changed | unverified |
| spatial | Focus-plane motion blur | passed | changed | changed | unverified |
| spatial | Radial zoom integration | passed | changed | changed | unverified |
| spatial | Radial channel correction | passed | changed | changed | unverified |
| spatial | Local projective plane | passed | changed | changed | unverified |
| spatial | Compact radial warp | passed | changed | changed | unverified |
| selection | Circular hue arc | passed | changed | changed | unverified |
| selection | Opponent colourfulness | passed | changed | changed | unverified |
| selection | Red–blue balance | passed | changed | changed | unverified |
| selection | Channel clipping fraction | passed | changed | changed | unverified |
| selection | Neutral-axis proximity | passed | changed | changed | unverified |
| selection | Local contrast polarity | passed | changed | changed | unverified |
| selection | Local tonal entropy | passed | changed | changed | unverified |
| selection | Laplacian detail | passed | changed | changed | unverified |
| selection | Oriented luminance edges | passed | changed | changed | unverified |
| selection | Structure tensor coherence | passed | changed | changed | unverified |
| selection | Harris corner response | passed | changed | changed | unverified |
| selection | Local luminance rank | passed | changed | changed | unverified |
| selection | Chromatic boundaries | passed | changed | changed | unverified |
| selection | Neutral bright residual | passed | changed | changed | unverified |
| selection | Shadow chroma deviation | passed | changed | changed | unverified |
| selection | Superellipse marquee | passed | changed | changed | unverified |
| selection | Cubic Bézier ribbon | passed | changed | changed | unverified |
| selection | Concave star marquee | passed | changed | changed | unverified |
| selection | Continuous foreground/background seed field | passed | changed | changed | unverified |
| selection | Bilateral overlap | passed | changed | changed | unverified |
| selection | Periodic selection tiles | passed | changed | changed | unverified |
| selection | Polar coverage unwrap | passed | changed | changed | unverified |
| selection | Coverage quantile equalization | passed | changed | changed | unverified |
| selection | Harmonic coverage patch repair | passed | changed | changed | unverified |
| selection | Principal-axis region strip | passed | changed | changed | unverified |
| inspection | Centre of light | passed | read-only or workflow; see evidence | read-only or workflow; see evidence | unverified |
| inspection | Centre of detail | passed | read-only or workflow; see evidence | read-only or workflow; see evidence | unverified |
| inspection | Quadrant light balance | passed | read-only or workflow; see evidence | read-only or workflow; see evidence | unverified |
| inspection | Border light energy | passed | read-only or workflow; see evidence | read-only or workflow; see evidence | unverified |
| inspection | Thirds line detail mass | passed | read-only or workflow; see evidence | read-only or workflow; see evidence | unverified |
| inspection | Mirror asymmetry | passed | read-only or workflow; see evidence | read-only or workflow; see evidence | unverified |
| inspection | Bright region connectivity | passed | read-only or workflow; see evidence | read-only or workflow; see evidence | unverified |
| inspection | Radial light profile | passed | read-only or workflow; see evidence | read-only or workflow; see evidence | unverified |
| inspection | Curvature sign transitions | passed | read-only or workflow; see evidence | read-only or workflow; see evidence | unverified |
| inspection | Horizontal edge support | passed | read-only or workflow; see evidence | read-only or workflow; see evidence | unverified |
| inspection | Edge overshoot residual | passed | read-only or workflow; see evidence | read-only or workflow; see evidence | unverified |
| inspection | Horizontal signal autocorrelation | passed | read-only or workflow; see evidence | read-only or workflow; see evidence | unverified |
| inspection | Diagonal parity residual | passed | read-only or workflow; see evidence | read-only or workflow; see evidence | unverified |
| inspection | Directional residual correlation | passed | read-only or workflow; see evidence | read-only or workflow; see evidence | unverified |
| inspection | Decimated Haar energy | passed | read-only or workflow; see evidence | read-only or workflow; see evidence | unverified |
| inspection | Edge position covariance | passed | read-only or workflow; see evidence | read-only or workflow; see evidence | unverified |
| inspection | Boundary edge activity | passed | read-only or workflow; see evidence | read-only or workflow; see evidence | unverified |
| inspection | Channel information dependence | passed | read-only or workflow; see evidence | read-only or workflow; see evidence | unverified |
| inspection | Working RGB palette cube | passed | read-only or workflow; see evidence | read-only or workflow; see evidence | unverified |
| inspection | Circular hue energy | passed | read-only or workflow; see evidence | read-only or workflow; see evidence | unverified |
| inspection | Chroma and light distribution | passed | read-only or workflow; see evidence | read-only or workflow; see evidence | unverified |
| inspection | Neutral cast by luminance | passed | read-only or workflow; see evidence | read-only or workflow; see evidence | unverified |
| inspection | Chroma covariance | passed | read-only or workflow; see evidence | read-only or workflow; see evidence | unverified |
| inspection | Tonal asymmetry and tails | passed | read-only or workflow; see evidence | read-only or workflow; see evidence | unverified |
| inspection | Missing level runs | passed | read-only or workflow; see evidence | read-only or workflow; see evidence | unverified |
| colour | Normalized sigmoid contrast | passed | changed | changed | unverified |
| colour | Exponential highlight rolloff | passed | changed | changed | unverified |
| colour | Monotone Hermite contrast | passed | changed | changed | unverified |
| colour | Rational light bias | passed | changed | changed | unverified |
| colour | Luminance solarization | passed | changed | changed | unverified |
| colour | Stop-zone exposure lift | passed | changed | changed | unverified |
| colour | Linear black-floor subtraction | passed | changed | changed | unverified |
| colour | Selected dark-channel haze removal | passed | changed | changed | unverified |
| colour | Variance-guided luminance cleanup | passed | changed | changed | unverified |
| colour | Local extrema tone contraction | passed | changed | changed | unverified |
| colour | Opponent-plane colour shear | passed | changed | changed | unverified |
| colour | Radius-dependent hue spiral | passed | changed | changed | unverified |
| colour | Luminance-dependent hue twist | passed | changed | changed | unverified |
| colour | Complementary duotone gradient | passed | changed | changed | unverified |
| colour | Cyclic channel crossfeed | passed | changed | changed | unverified |
| colour | Subtractive colour density | passed | changed | changed | unverified |
| colour | Bradford cone-response adaptation | passed | changed | changed | unverified |
| colour | RGB channel-ratio limiter | passed | changed | changed | unverified |
| colour | Median-channel spill suppression | passed | changed | changed | unverified |
| colour | Three-axis hue harmony | passed | changed | changed | unverified |
| colour | Colour-luminance regression removal | passed | changed | changed | unverified |
| colour | Luminance-guided colour cleanup | passed | changed | changed | unverified |
| colour | Weighted median colour repair | passed | changed | changed | unverified |
| colour | Channel-distribution alignment | passed | changed | changed | unverified |
| colour | Range-aware bilateral colour cleanup | passed | changed | changed | unverified |
| spatial | Adaptive impulse median | passed | changed | changed | unverified |
| spatial | Sigma-gated colour smoothing | passed | changed | changed | unverified |
| spatial | Distance-weighted colour median | passed | changed | changed | unverified |
| spatial | Luminance percentile limiter | passed | changed | changed | unverified |
| spatial | Chroma range smoothing | passed | changed | changed | unverified |
| spatial | Chroma covariance denoising | passed | changed | changed | unverified |
| spatial | Colour covariance denoising | passed | changed | changed | unverified |
| spatial | Entropy-weighted detail | passed | changed | changed | unverified |
| spatial | Local RMS contrast | passed | changed | changed | unverified |
| spatial | Log-illumination correction | passed | changed | changed | unverified |
| spatial | Oriented periodic notch | passed | changed | changed | unverified |
| spatial | Cross-footprint median | passed | changed | changed | unverified |
| spatial | Curvature ridge detail | passed | changed | changed | unverified |
| spatial | Coherent structure detail | passed | changed | changed | unverified |
| spatial | Least-dispersion line median | passed | changed | changed | unverified |
| spatial | Curved-surface residual detail | passed | changed | changed | unverified |
| spatial | Windowed-sinc selection shift | passed | changed | changed | unverified |
| spatial | Elliptical aperture integration | passed | changed | changed | unverified |
| spatial | Hexagonal aperture integration | passed | changed | changed | unverified |
| spatial | Cat-eye aperture integration | passed | changed | changed | unverified |
| spatial | One-sided highlight streak | passed | changed | changed | unverified |
| spatial | Range-aware base compression | passed | changed | changed | unverified |
| spatial | Selected dark-channel haze repair | passed | changed | changed | unverified |
| spatial | Connected spot repair | passed | changed | changed | unverified |
| spatial | Luminance-preserving chroma median | passed | changed | changed | unverified |
| selection | CIE Lab sampled distance | passed | changed | changed | unverified |
| selection | Covariance colour distance | passed | changed | changed | unverified |
| selection | Geodesic colour propagation | passed | changed | changed | unverified |
| selection | Local Otsu threshold | passed | changed | changed | unverified |
| selection | Sauvola adaptive threshold | passed | changed | changed | unverified |
| selection | Gabor frequency response | passed | changed | changed | unverified |
| selection | Directional bright ridges | passed | changed | changed | unverified |
| selection | Local tonal skewness | passed | changed | changed | unverified |
| selection | Local tonal kurtosis | passed | changed | changed | unverified |
| selection | Colour-line residual | passed | changed | changed | unverified |
| selection | Grayscale morphological opening | passed | changed | changed | unverified |
| selection | Grayscale morphological closing | passed | changed | changed | unverified |
| selection | Opening by reconstruction | passed | changed | changed | unverified |
| selection | Grayscale area opening | passed | changed | changed | unverified |
| selection | Keep border-connected regions | passed | changed | changed | unverified |
| selection | Seed geodesic distance bands | passed | changed | changed | unverified |
| selection | Keep regions enclosing holes | passed | changed | changed | unverified |
| selection | Regional coverage maxima | passed | changed | changed | unverified |
| selection | Archimedean spiral ribbon | passed | changed | changed | unverified |
| selection | Barycentric triangle gradient | passed | changed | changed | unverified |
| selection | Affine selection shear | passed | changed | changed | unverified |
| selection | Radial selection twirl | passed | changed | changed | unverified |
| selection | Spherical selection remap | passed | changed | changed | unverified |
| selection | Image-gradient displacement | passed | changed | changed | unverified |
| selection | Signed-distance edge smoothing | passed | changed | changed | unverified |
| inspection | Luminance quantile landmarks | passed | read-only or workflow; see evidence | read-only or workflow; see evidence | unverified |
| inspection | Robust luminance dispersion | passed | read-only or workflow; see evidence | read-only or workflow; see evidence | unverified |
| inspection | Light energy inequality | passed | read-only or workflow; see evidence | read-only or workflow; see evidence | unverified |
| inspection | Circular hue concentration | passed | read-only or workflow; see evidence | read-only or workflow; see evidence | unverified |
| inspection | Hue drift through light levels | passed | read-only or workflow; see evidence | read-only or workflow; see evidence | unverified |
| inspection | Channel highlight headroom | passed | read-only or workflow; see evidence | read-only or workflow; see evidence | unverified |
| inspection | Absolute chroma tails | passed | read-only or workflow; see evidence | read-only or workflow; see evidence | unverified |
| inspection | Light averaging models | passed | read-only or workflow; see evidence | read-only or workflow; see evidence | unverified |
| inspection | Local colour transitions | passed | read-only or workflow; see evidence | read-only or workflow; see evidence | unverified |
| inspection | Upper quantile spacing | passed | read-only or workflow; see evidence | read-only or workflow; see evidence | unverified |
| inspection | Working range excursion energy | passed | read-only or workflow; see evidence | read-only or workflow; see evidence | unverified |
| inspection | Colour spatial variogram | passed | read-only or workflow; see evidence | read-only or workflow; see evidence | unverified |
| inspection | Regional structure coherence | passed | read-only or workflow; see evidence | read-only or workflow; see evidence | unverified |
| inspection | Surface curvature classes | passed | read-only or workflow; see evidence | read-only or workflow; see evidence | unverified |
| inspection | Gradient tail weight | passed | read-only or workflow; see evidence | read-only or workflow; see evidence | unverified |
| inspection | Signed edge transitions | passed | read-only or workflow; see evidence | read-only or workflow; see evidence | unverified |
| inspection | Relative total variation | passed | read-only or workflow; see evidence | read-only or workflow; see evidence | unverified |
| inspection | Cosine frequency energy | passed | read-only or workflow; see evidence | read-only or workflow; see evidence | unverified |
| inspection | Chroma and light edge alignment | passed | read-only or workflow; see evidence | read-only or workflow; see evidence | unverified |
| inspection | Fine residual polarity | passed | read-only or workflow; see evidence | read-only or workflow; see evidence | unverified |
| inspection | Regional tone entropy | passed | read-only or workflow; see evidence | read-only or workflow; see evidence | unverified |
| inspection | Linear lighting trend | passed | read-only or workflow; see evidence | read-only or workflow; see evidence | unverified |
| inspection | Quadratic lighting trend | passed | read-only or workflow; see evidence | read-only or workflow; see evidence | unverified |
| inspection | Edge topology | passed | read-only or workflow; see evidence | read-only or workflow; see evidence | unverified |
| inspection | Edge occupancy scaling | passed | read-only or workflow; see evidence | read-only or workflow; see evidence | unverified |
| colour | Measured spatial cast removal | passed | changed | changed | unverified |
| colour | Measured radial illumination correction | passed | changed | changed | unverified |
| colour | Gaussian patch palette transport | passed | changed | changed | unverified |
| colour | Three-anchor palette constraint | passed | changed | changed | unverified |
| colour | Measured circular hue redistribution | passed | changed | changed | unverified |
| colour | Chroma marginal copula remapping | passed | changed | changed | unverified |
| colour | Hue-dependent colourfulness balance | passed | changed | changed | unverified |
| colour | Cluster-residual palette recolouring | passed | changed | changed | unverified |
| colour | Measured channel-response reconstruction | passed | changed | changed | unverified |
| colour | Kubelka-Munk pigment mixing | passed | changed | changed | unverified |
| colour | Variance-preserving principal monochrome | passed | changed | changed | unverified |
| colour | Two-patch discriminant monochrome | passed | changed | changed | unverified |
| colour | Isoluminant colour-edge lighting | passed | changed | changed | unverified |
| colour | Selected error-diffusion tones | passed | changed | changed | unverified |
| colour | Area-modulated photographic dots | passed | changed | changed | unverified |
| colour | Stop-domain luminance contours | passed | changed | changed | unverified |
| colour | Analytical exposure fusion | passed | changed | changed | unverified |
| colour | Constant-Lab-chroma lightness lift | passed | changed | changed | unverified |
| colour | Uniform-chromaticity cast translation | passed | changed | changed | unverified |
| colour | Robust group-lasso chroma cleanup | passed | changed | changed | unverified |
| colour | Geometric-median cast balance | passed | changed | changed | unverified |
| colour | Compositional-geodesic colour tint | passed | changed | changed | unverified |
| colour | Fixed-energy lightness redistribution | passed | changed | changed | unverified |
| colour | Dichromatic reflection subtraction | passed | changed | changed | unverified |
| colour | Mirrored palette hue folding | passed | changed | changed | unverified |
| spatial | Local cosine shrinkage | passed | changed | changed | unverified |
| spatial | Biorthogonal lifting cleanup | passed | changed | changed | unverified |
| spatial | Geometric colour median | passed | changed | changed | unverified |
| spatial | Iterative colour mode | passed | changed | changed | unverified |
| spatial | Pairwise robust colour location | passed | changed | changed | unverified |
| spatial | Geodesic colour averaging | passed | changed | changed | unverified |
| spatial | Spatial patch low-rank repair | passed | changed | changed | unverified |
| spatial | Covariance-distance smoothing | passed | changed | changed | unverified |
| spatial | Multiplicative Lee restoration | passed | changed | changed | unverified |
| spatial | Frost spatial damping | passed | changed | changed | unverified |
| spatial | Gamma-MAP speckle restoration | passed | changed | changed | unverified |
| spatial | Fourth-order surface repair | passed | changed | changed | unverified |
| spatial | Fractional spatial detail | passed | changed | changed | unverified |
| spatial | Parabolic background separation | passed | changed | changed | unverified |
| spatial | Oriented scratch repair | passed | changed | changed | unverified |
| spatial | Quadrature phase detail | passed | changed | changed | unverified |
| spatial | Spectral surprise detail | passed | changed | changed | unverified |
| spatial | MAD impulse repair | passed | changed | changed | unverified |
| spatial | Circular hue smoothing | passed | changed | changed | unverified |
| spatial | Chroma strength median | passed | changed | changed | unverified |
| spatial | Local white-patch adaptation | passed | changed | changed | unverified |
| spatial | Local gray-edge adaptation | passed | changed | changed | unverified |
| spatial | Luminance skewness balance | passed | changed | changed | unverified |
| spatial | Tail-weighted detail | passed | changed | changed | unverified |
| spatial | Monotonic edge limiter | passed | changed | changed | unverified |
| selection | Robust median deviation | passed | changed | changed | unverified |
| selection | Local binary-pattern transitions | passed | changed | changed | unverified |
| selection | Directional patch autocorrelation | passed | changed | changed | unverified |
| selection | Directional semivariance | passed | changed | changed | unverified |
| selection | Log-chromaticity sample distance | passed | changed | changed | unverified |
| selection | Gradient nonmaximum suppression | passed | changed | changed | unverified |
| selection | Connected hysteresis edges | passed | changed | changed | unverified |
| selection | Local Moran spatial association | passed | changed | changed | unverified |
| selection | Sampled patch correlation | passed | changed | changed | unverified |
| selection | Circular hue dispersion | passed | changed | changed | unverified |
| selection | Colour prototype partition | passed | changed | changed | unverified |
| selection | Connectivity articulation cells | passed | changed | changed | unverified |
| selection | Selection graph core | passed | changed | changed | unverified |
| selection | Seeded coverage watershed | passed | changed | changed | unverified |
| selection | Anisotropic coverage diffusion | passed | changed | changed | unverified |
| selection | Coverage curvature flow | passed | changed | changed | unverified |
| selection | Region maximum inscribed disk | passed | changed | changed | unverified |
| selection | Directional selected run width | passed | changed | changed | unverified |
| selection | Thin-plate landmark deformation | passed | changed | changed | unverified |
| selection | Mean-value cage deformation | passed | changed | changed | unverified |
| selection | Triangulated cage deformation | passed | changed | changed | unverified |
| selection | Conformal Mobius deformation | passed | changed | changed | unverified |
| selection | Path-normal coverage unwrap | passed | changed | changed | unverified |
| selection | Coverage-aware seam compression | passed | changed | changed | unverified |
| selection | Lissajous ribbon construction | passed | changed | changed | unverified |
| inspection | Log channel-ratio distribution | passed | read-only or workflow; see evidence | read-only or workflow; see evidence | unverified |
| inspection | White-patch chromaticity | passed | read-only or workflow; see evidence | read-only or workflow; see evidence | unverified |
| inspection | Gray-edge chromaticity | passed | read-only or workflow; see evidence | read-only or workflow; see evidence | unverified |
| inspection | Opponent colourfulness proxy | passed | read-only or workflow; see evidence | read-only or workflow; see evidence | unverified |
| inspection | Hue occupancy and largest gap | passed | read-only or workflow; see evidence | read-only or workflow; see evidence | unverified |
| inspection | Local hue discontinuity | passed | read-only or workflow; see evidence | read-only or workflow; see evidence | unverified |
| inspection | Chroma-light mutual information | passed | read-only or workflow; see evidence | read-only or workflow; see evidence | unverified |
| inspection | Regional colour dispersion | passed | read-only or workflow; see evidence | read-only or workflow; see evidence | unverified |
| inspection | Robust luminance outliers | passed | read-only or workflow; see evidence | read-only or workflow; see evidence | unverified |
| inspection | Channel highlight dependence | passed | read-only or workflow; see evidence | read-only or workflow; see evidence | unverified |
| inspection | Regional mean-variance fit | passed | read-only or workflow; see evidence | read-only or workflow; see evidence | unverified |
| inspection | Connected clipping support | passed | read-only or workflow; see evidence | read-only or workflow; see evidence | unverified |
| inspection | Clipping-weighted edge fraction | passed | read-only or workflow; see evidence | read-only or workflow; see evidence | unverified |
| inspection | Robust strip lighting profile | passed | read-only or workflow; see evidence | read-only or workflow; see evidence | unverified |
| inspection | Thumbnail phase sensitivity | passed | read-only or workflow; see evidence | read-only or workflow; see evidence | unverified |
| inspection | Ordinal texture entropy | passed | read-only or workflow; see evidence | read-only or workflow; see evidence | unverified |
| inspection | Sequence sample entropy | passed | read-only or workflow; see evidence | read-only or workflow; see evidence | unverified |
| inspection | Quantized texture runs | passed | read-only or workflow; see evidence | read-only or workflow; see evidence | unverified |
| inspection | Neighbour co-occurrence texture | passed | read-only or workflow; see evidence | read-only or workflow; see evidence | unverified |
| inspection | Bright mass lacunarity | passed | read-only or workflow; see evidence | read-only or workflow; see evidence | unverified |
| inspection | Regional corner response | passed | read-only or workflow; see evidence | read-only or workflow; see evidence | unverified |
| inspection | Local illumination fit residual | passed | read-only or workflow; see evidence | read-only or workflow; see evidence | unverified |
| inspection | Line spectral flatness | passed | read-only or workflow; see evidence | read-only or workflow; see evidence | unverified |
| inspection | Neighbour Moran index | passed | read-only or workflow; see evidence | read-only or workflow; see evidence | unverified |
| inspection | Robust local roughness | passed | read-only or workflow; see evidence | read-only or workflow; see evidence | unverified |
| selection-management | duplicate | passed | not tested in this scenario | changed | unverified |
| selection-management | invert | passed | not tested in this scenario | changed | unverified |
| selection-management | union | passed | not tested in this scenario | changed | unverified |
| selection-management | intersection | passed | not tested in this scenario | changed | unverified |
| selection-management | subtraction | passed | not tested in this scenario | changed | unverified |
| selection-management | freeze | passed | not tested in this scenario | changed | unverified |
| selection-management | feather | passed | not tested in this scenario | changed | unverified |
| selection-management | expand | passed | not tested in this scenario | changed | unverified |
| selection-management | contract | passed | not tested in this scenario | changed | unverified |
| selection-management | edge_contrast | passed | not tested in this scenario | changed | unverified |
| selection-management | remove_islands | passed | not tested in this scenario | changed | unverified |
| selection-management | fill_holes | passed | not tested in this scenario | changed | unverified |
| selection-management | edit_linear | passed | not tested in this scenario | changed | unverified |
| selection-management | edit_radial | passed | not tested in this scenario | changed | unverified |
| selection-management | edit_brush | passed | not tested in this scenario | changed | unverified |
| selection-management | edit_luminance | passed | not tested in this scenario | changed | unverified |
| selection-management | component_mode | passed | not tested in this scenario | changed | unverified |
| selection-management | reorder_component | passed | not tested in this scenario | unchanged; conditional or neutral on this fixture | unverified |
| selection-management | extract_component | passed | not tested in this scenario | changed | unverified |
| selection-management | copy_component | passed | not tested in this scenario | changed | unverified |
| selection-management | transform_selection | passed | not tested in this scenario | changed | unverified |
| selection-management | border_selection | passed | not tested in this scenario | changed | unverified |
| selection-management | keep_largest | passed | not tested in this scenario | changed | unverified |
| selection-management | keep_connected | passed | not tested in this scenario | changed | unverified |
| selection-management | median_cleanup | passed | not tested in this scenario | changed | unverified |
| selection-management | exclusive_union | passed | not tested in this scenario | changed | unverified |
| selection-management | convex_hull | passed | not tested in this scenario | changed | unverified |
| semantic-selection | subject | passed | not tested in this scenario | unchanged; conditional or neutral on this fixture | unverified |
| semantic-selection | background | passed | not tested in this scenario | unchanged; conditional or neutral on this fixture | unverified |
| semantic-selection | sky | passed | not tested in this scenario | no selection; safe refusal: No sky found: no open sky at the top of the frame. | unverified |
| semantic-selection | face_skin | passed | not tested in this scenario | unchanged; conditional or neutral on this fixture | unverified |
| semantic-selection | body_skin | passed | not tested in this scenario | unchanged; conditional or neutral on this fixture | unverified |
| semantic-selection | hair | passed | not tested in this scenario | unchanged; conditional or neutral on this fixture | unverified |
| semantic-selection | clothes | passed | not tested in this scenario | unchanged; conditional or neutral on this fixture | unverified |
| semantic-selection | eyes | passed | not tested in this scenario | unchanged; conditional or neutral on this fixture | unverified |
| semantic-selection | iris | passed | not tested in this scenario | unchanged; conditional or neutral on this fixture | unverified |
| semantic-selection | sclera | passed | not tested in this scenario | unchanged; conditional or neutral on this fixture | unverified |
| semantic-selection | eyebrows | passed | not tested in this scenario | unchanged; conditional or neutral on this fixture | unverified |
| semantic-selection | lips | passed | not tested in this scenario | unchanged; conditional or neutral on this fixture | unverified |
| semantic-selection | teeth | passed | not tested in this scenario | unchanged; conditional or neutral on this fixture | unverified |
| semantic-selection | facial_hair | passed | not tested in this scenario | empty coverage on this fixture; selection effect not demonstrated | unverified |
| local-parameter | exposure | passed | not tested in this scenario | changed | unverified |
| local-parameter | contrast | passed | not tested in this scenario | changed | unverified |
| local-parameter | highlights | passed | not tested in this scenario | changed | unverified |
| local-parameter | shadows | passed | not tested in this scenario | changed | unverified |
| local-parameter | whites | passed | not tested in this scenario | changed | unverified |
| local-parameter | blacks | passed | not tested in this scenario | changed | unverified |
| local-parameter | temperature | passed | not tested in this scenario | changed | unverified |
| local-parameter | tint | passed | not tested in this scenario | changed | unverified |
| local-parameter | saturation | passed | not tested in this scenario | changed | unverified |
| local-parameter | clarity | passed | not tested in this scenario | changed | unverified |
| local-parameter | texture | passed | not tested in this scenario | changed | unverified |
| retouch | heal | passed | not tested in this scenario | changed | unverified |
| retouch | clone | passed | not tested in this scenario | changed | unverified |
| retouch | auto_blemish | passed | not tested in this scenario | changed | unverified |
| retouch | frequency | passed | not tested in this scenario | changed | unverified |
| retouch | micro_dodge_burn | passed | not tested in this scenario | changed | unverified |
| retouch | dodge | passed | not tested in this scenario | changed | unverified |
| retouch | burn | passed | not tested in this scenario | changed | unverified |
| retouch | skin_color | passed | not tested in this scenario | changed | unverified |
| retouch | color_match | passed | not tested in this scenario | changed | unverified |
| retouch | mattify | passed | not tested in this scenario | changed | unverified |
| retouch | under_eye | passed | not tested in this scenario | unchanged; conditional or neutral on this fixture | unverified |
| retouch | wrinkle | passed | not tested in this scenario | changed | unverified |
| retouch | teeth | passed | not tested in this scenario | changed | unverified |
| retouch | eye_clean | passed | not tested in this scenario | changed | unverified |
| retouch | eye_detail | passed | not tested in this scenario | changed | unverified |
| retouch | red_eye | passed | not tested in this scenario | changed | unverified |
| retouch | fabric | passed | not tested in this scenario | changed | unverified |
| retouch | fabric_moire | passed | not tested in this scenario | changed | unverified |
| retouch | fabric_stain | passed | not tested in this scenario | changed | unverified |
| retouch | shadow_balance | passed | not tested in this scenario | changed | unverified |
| retouch | background_blur | passed | not tested in this scenario | changed | unverified |
| retouch | highlight_glow | passed | not tested in this scenario | changed | unverified |
| retouch | motion_blur | passed | not tested in this scenario | changed | unverified |
| retouch | zoom_blur | passed | not tested in this scenario | changed | unverified |
| retouch | color_despill | passed | not tested in this scenario | changed | unverified |
| retouch | highlight_compress | passed | not tested in this scenario | changed | unverified |
| retouch | color_saturation | passed | not tested in this scenario | changed | unverified |
| retouch | backdrop | passed | not tested in this scenario | changed | unverified |
| retouch | glare | passed | not tested in this scenario | changed | unverified |
| retouch | makeup | passed | not tested in this scenario | changed | unverified |
| retouch | colorize | passed | not tested in this scenario | changed | unverified |
| retouch | background_color | passed | not tested in this scenario | changed | unverified |
| retouch | reshape | passed | not tested in this scenario | changed | unverified |
| retouch | skin_smooth | passed | not tested in this scenario | changed | unverified |
| retouch | skin_uniformity | passed | not tested in this scenario | changed | unverified |
| retouch | portrait_dodge_burn | passed | not tested in this scenario | changed | unverified |
| retouch | patch_heal | passed | not tested in this scenario | changed | unverified |
| retouch | frequency_heal | passed | not tested in this scenario | unchanged; conditional or neutral on this fixture | unverified |
| retouch | texture_graft | passed | not tested in this scenario | changed | unverified |
| retouch | acne_clear | passed | not tested in this scenario | changed | unverified |
| retouch | stretch_marks | passed | not tested in this scenario | changed | unverified |
| retouch | defringe | passed | not tested in this scenario | unchanged; conditional or neutral on this fixture | unverified |
| material | directional_smooth | passed | not tested in this scenario | changed | unverified |
| material | silver_hair | passed | not tested in this scenario | changed | unverified |
| material | fabric_bands | passed | not tested in this scenario | changed | unverified |
| material | luma_moire | passed | not tested in this scenario | changed | unverified |
| material | deband | passed | not tested in this scenario | changed | unverified |
| material | chroma_median | passed | not tested in this scenario | changed | unverified |
| material | impulse_repair | passed | not tested in this scenario | changed | unverified |
| material | line_scratch | passed | not tested in this scenario | changed | unverified |
| material | destripe | passed | not tested in this scenario | changed | unverified |
| material | checkerboard | passed | not tested in this scenario | changed | unverified |
| material | ringing | passed | not tested in this scenario | changed | unverified |
| material | specular_chroma | passed | not tested in this scenario | changed | unverified |
| material | chroma_mottle | passed | not tested in this scenario | changed | unverified |
| material | illumination_plane | passed | not tested in this scenario | changed | unverified |
| material | channel_detail | passed | not tested in this scenario | changed | unverified |
| studio-colour | levels-rgb | passed | not tested in this scenario | changed | unverified |
| studio-colour | levels-red | passed | not tested in this scenario | changed | unverified |
| studio-colour | levels-green | passed | not tested in this scenario | changed | unverified |
| studio-colour | levels-blue | passed | not tested in this scenario | changed | unverified |
| studio-colour | luminosity-levels | passed | not tested in this scenario | changed | unverified |
| studio-colour | selective-relative | passed | not tested in this scenario | changed | unverified |
| studio-colour | selective-absolute | passed | not tested in this scenario | changed | unverified |
| studio-colour | local-curve | passed | not tested in this scenario | changed | unverified |
| studio-colour | local-hsl | passed | not tested in this scenario | changed | unverified |
| studio-colour | local-grade | passed | not tested in this scenario | changed | unverified |
| studio-colour | local-sharpen | passed | not tested in this scenario | changed | unverified |
| studio-colour | local-dehaze | passed | not tested in this scenario | changed | unverified |
| studio-colour | local-vibrance | passed | not tested in this scenario | changed | unverified |
| colour-measurement | auto | passed | not tested in this scenario | unchanged; conditional or neutral on this fixture | unverified |
| colour-measurement | black | passed | not tested in this scenario | unchanged; conditional or neutral on this fixture | unverified |
| colour-measurement | white | passed | not tested in this scenario | unchanged; conditional or neutral on this fixture | unverified |
| colour-measurement | hsl | passed | not tested in this scenario | unchanged; conditional or neutral on this fixture | unverified |
| parameter | global.exposure | passed | not tested in this scenario | changed | unverified |
| parameter | global.temperature | passed | not tested in this scenario | changed | unverified |
| parameter | global.tint | passed | not tested in this scenario | changed | unverified |
| parameter | global.contrast | passed | not tested in this scenario | changed | unverified |
| parameter | global.highlights | passed | not tested in this scenario | changed | unverified |
| parameter | global.shadows | passed | not tested in this scenario | changed | unverified |
| parameter | global.whites | passed | not tested in this scenario | changed | unverified |
| parameter | global.blacks | passed | not tested in this scenario | changed | unverified |
| parameter | global.texture | passed | not tested in this scenario | changed | unverified |
| parameter | global.clarity | passed | not tested in this scenario | changed | unverified |
| parameter | global.dehaze | passed | not tested in this scenario | changed | unverified |
| parameter | global.vibrance | passed | not tested in this scenario | changed | unverified |
| parameter | global.saturation | passed | not tested in this scenario | changed | unverified |
| parameter | global.hsl.red.h | passed | not tested in this scenario | changed | unverified |
| parameter | global.hsl.orange.s | passed | not tested in this scenario | changed | unverified |
| parameter | global.hsl.blue.l | passed | not tested in this scenario | changed | unverified |
| parameter | global.curve.points | passed | not tested in this scenario | changed | unverified |
| parameter | global.channel_curves.red.points | passed | not tested in this scenario | changed | unverified |
| parameter | global.parametric.lights | passed | not tested in this scenario | changed | unverified |
| parameter | global.sharpen.amount | passed | not tested in this scenario | changed | unverified |
| parameter | global.noise.luminance | passed | not tested in this scenario | changed | unverified |
| parameter | global.noise.colour | passed | not tested in this scenario | changed | unverified |
| parameter | global.effects.vignette.amount | passed | not tested in this scenario | changed | unverified |
| parameter | global.effects.grain.amount | passed | not tested in this scenario | changed | unverified |
| parameter | global.calibration.red_hue | passed | not tested in this scenario | changed | unverified |
| parameter | global.colour_grade.shadows.saturation | passed | not tested in this scenario | changed | unverified |
| parameter | bw | passed | not tested in this scenario | changed | unverified |
| parameter | geometry.rotate | passed | not tested in this scenario | changed | unverified |
| parameter | geometry.crop | passed | not tested in this scenario | changed | unverified |
| parameter | lens.distortion | passed | not tested in this scenario | unchanged; conditional or neutral on this fixture | unverified |
| parameter | lens.ca | passed | not tested in this scenario | unchanged; conditional or neutral on this fixture | unverified |
| parameter | lens.vignette | passed | not tested in this scenario | changed | unverified |
| inspection | RGB parade | passed | not tested in this scenario | read-only or workflow; see evidence | unverified |
| inspection | Luminance waveform | passed | not tested in this scenario | read-only or workflow; see evidence | unverified |
| inspection | Chroma vectorscope | passed | not tested in this scenario | read-only or workflow; see evidence | unverified |
| inspection | Exposure zone map | passed | not tested in this scenario | read-only or workflow; see evidence | unverified |
| inspection | Working range excursions | passed | not tested in this scenario | read-only or workflow; see evidence | unverified |
| inspection | Spatial edge detail | passed | not tested in this scenario | read-only or workflow; see evidence | unverified |
| inspection | Flat-area residual | passed | not tested in this scenario | read-only or workflow; see evidence | unverified |
| inspection | Near-neutral colour balance | passed | not tested in this scenario | read-only or workflow; see evidence | unverified |
| inspection | Local contrast map | passed | not tested in this scenario | read-only or workflow; see evidence | unverified |
| inspection | Rendered tonal range | passed | not tested in this scenario | read-only or workflow; see evidence | unverified |
| inspection | Exposure zone distribution | passed | not tested in this scenario | read-only or workflow; see evidence | unverified |
| inspection | sRGB working gamut occupancy | passed | not tested in this scenario | read-only or workflow; see evidence | unverified |
| inspection | Row and column banding profile | passed | not tested in this scenario | read-only or workflow; see evidence | unverified |
| inspection | Eight-pixel block boundary profile | passed | not tested in this scenario | read-only or workflow; see evidence | unverified |
| inspection | Edge direction balance | passed | not tested in this scenario | read-only or workflow; see evidence | unverified |
| inspection | Relative illumination map | passed | not tested in this scenario | read-only or workflow; see evidence | unverified |
| inspection | Neutral-edge channel alignment | passed | not tested in this scenario | read-only or workflow; see evidence | unverified |
| inspection | Channel correlation | passed | not tested in this scenario | read-only or workflow; see evidence | unverified |
| inspection | Detail scale spectrum | passed | not tested in this scenario | read-only or workflow; see evidence | unverified |
| inspection | Highlight chroma retention | passed | not tested in this scenario | read-only or workflow; see evidence | unverified |
| export | jpeg | passed | not tested in this scenario | unchanged; conditional or neutral on this fixture | unverified |
| export | png | passed | not tested in this scenario | unchanged; conditional or neutral on this fixture | unverified |
| export | tiff | passed | not tested in this scenario | unchanged; conditional or neutral on this fixture | unverified |
| reference-matching | prepare-and-apply-reference | passed | not tested in this scenario | changed | unverified |
| reference-library | create_group-Qualification A | passed | not tested in this scenario | unchanged; conditional or neutral on this fixture | unverified |
| reference-library | create_group-Qualification B | passed | not tested in this scenario | unchanged; conditional or neutral on this fixture | unverified |
| reference-library | rename_group | passed | not tested in this scenario | unchanged; conditional or neutral on this fixture | unverified |
| reference-library | reorder_groups | passed | not tested in this scenario | unchanged; conditional or neutral on this fixture | unverified |
| reference-library | move | passed | not tested in this scenario | unchanged; conditional or neutral on this fixture | unverified |
| reference-library | reorder_references | passed | not tested in this scenario | unchanged; conditional or neutral on this fixture | unverified |
| reference-library | favorite | passed | not tested in this scenario | unchanged; conditional or neutral on this fixture | unverified |
| reference-library | defaults | passed | not tested in this scenario | unchanged; conditional or neutral on this fixture | unverified |
| reference-library | reset_defaults | passed | not tested in this scenario | unchanged; conditional or neutral on this fixture | unverified |
| reference-library | export | passed | not tested in this scenario | unchanged; conditional or neutral on this fixture | unverified |
| reference-library | import | passed | not tested in this scenario | unchanged; conditional or neutral on this fixture | unverified |
| reference-library | delete_group | passed | not tested in this scenario | unchanged; conditional or neutral on this fixture | unverified |
| reference-library | stale-revision | passed | not tested in this scenario | unchanged; conditional or neutral on this fixture | unverified |
| reference-library | remove | passed | not tested in this scenario | unchanged; conditional or neutral on this fixture | unverified |

Executable SHA-256: `e6d5618879b90b6b181805bc143c15c469e8ffa0711fcda5f0f7e7620be29d9e`. 1529 source-file hashes matched the qualification manifest.
