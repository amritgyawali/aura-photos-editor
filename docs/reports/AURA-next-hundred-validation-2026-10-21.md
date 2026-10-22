# AURA: another 100 capabilities and full-folder qualification

Date: 2026-10-21, Asia/Katmandu. Local qualification passed.

This adds 100 distinct capabilities after the preceding hundred: 25 colour methods, 25 spatial methods, 25 selection workflows, and 25 read-only inspection tools. The colour and spatial panels now each expose 50 methods; the selection laboratory exposes 50 workflows. Three additional editor tabs provide palette/tone, detail, and composition review. Existing stacks remain bounded to 32 operations.

**Exact Evoto output, every Evoto feature, and proprietary trained-model parity have not been achieved or verified.** No Evoto code, weights, shaders or assets were incorporated into AURA. Tool counts and automated tests do not demonstrate visual superiority.

Actual rebuilt-desktop captures: [Colour lab](assets/aura-next100-colour-2026-10-21.png), [Spatial repair](assets/aura-next100-spatial-2026-10-21.png), [Selection laboratory](assets/aura-next100-selection-2026-10-21.png), [Palette review](assets/aura-next100-palette-review-2026-10-21.png), [Detail review](assets/aura-next100-detail-review-2026-10-21.png), [Composition review](assets/aura-next100-composition-review-2026-10-21.png).

## Recorded validation

|Check|Result|
|---|---|
|Native workspace plus scoped retests|4463 distinct cases verified, 0 unresolved failures, 53 ignored; 291 original result groups; case records agree: True|
|Original full-run history|4461 passed, 2 failed, 53 ignored; both failures remain recorded and were resolved by passing scoped retests|
|Frontend|945 tests in 111 files passed; production build exit 0|
|Strict Clippy/format/contracts/IPC|0/0/0/0; 83 frozen contracts; 347 definitions = registrations = typed calls|
|Python evidence tooling|exit 0; 18 tests|
|New 100 actual-desktop qualification|100/100; 108 checks; completed: True|
|Previous 100 regression|138 checks; completed: True|
|Supplied RAW photos|27/27 edited; original SHA256 unchanged: True|
|Full-resolution JPEG export|27 written, 27 verified, 0 render failures|
|Executable/source identity|All three desktop scenarios match current executable: True; 1375 source hashes checked, 0 changed after freeze|
|Final AURA review window|opened: True; 27 persisted recipe hashes checked against verified exports|

Native tests use the documented host setting `AURA_PERF_HOST_SCALE=6`. Ignored tests are excluded from pass counts. The command inventory explicitly marks routes without a current desktop invocation; this is not an assertion that every parameter of every feature was exhaustively tested.

All 50 new colour/spatial methods changed the representative 640×96 synthetic fixture. Two earlier conditional methods, `chroma_winsorize` and `mosquito_suppress`, produced no pixel change on the earlier fixture; their save/render/history scenarios passed, while algorithm-specific native fixtures provide separate coverage. A pixel change by itself does not establish a better photograph.

Early qualification caught and corrected tone-skew direction, empty tonal-bin colour shifts, spatial boundary leakage above the correction-grid resolution, mislabelled radial/perspective controls, palette percentage accumulation, and incomplete coarse Haar footprints. Their regression fixtures are included in the native/UI suites. Earlier failed development logs remain separate; the table uses final frozen-source runs.

The full workspace run found two stale assertions: the material/inspection integration test expected 20 diagnostics, and the cache identity test expected v2. They now assert 45 distinct diagnostic IDs and v3. The material integration target and complete renderer library target were rerun. Native totals above count distinct original cases with these two failures resolved by explicit passing retests; they do not describe a single clean full-run invocation. The CSV preserves each original status and its resolving log. Both changes are confined to test expectations; production processing and the executable remain unchanged. The desktop linker retains its nonfatal resource-manifest warning, and Vite retains its large-bundle advisory; actual startup is qualified separately.

The first desktop smoke attempts also expected the earlier “25 workflows” caption. Both audit selectors were corrected to 50 and both desktop audits were rerun in fresh catalogues. The failed caption attempts remain archived separately; the table and capability evidence use the completed reruns.

## Your photos

[Open the 27-photo before/after gallery](../../output/aura-next100-qualified/index.html). Full-resolution edited JPEGs are under `output/aura-next100-qualified/full-resolution/edited/`; per-photo recipes, inspection results and hashes are beside the gallery.

The photo run uses native AURA auto enhancement plus a gentle, explicit new lower-tail luminance layer (amount 0.3, strength 0.6, pivot 0.04). Every photo also receives 45 read-only inspection measurements. The recipe preserves originals and separates the added selection/tone layer for later editing. This is an AURA finishing choice, not a recovered Evoto preset.

## Evoto evidence and limits

`F:/evoto`: 1860 regular files, 4873227287 bytes, all hashed with stable reads; read gaps:0. All 1854 vendor-manifest MD5 entries match. Tree manifest SHA256:`61bf5b14dca7394f04d1e0ca576a442dae62fa7156e5c727d9aea2f395363162`. MD5 agreement records consistency, not an authenticity signature.

[Every-file inventory and SHA256](AURA-evoto-file-inventory-2026-10-21.csv) · [REA native-resource evidence index](AURA-evoto-native-resources-2026-10-21.json).

REA 6.7.0 inspected complete resource tables in 159 of 160 PE files. The 2 GB historical `Evoto_Setup_7.3.5-185.exe` remains hashed but excluded from the 256 MiB resource-parser budget. It is not treated as the installed application version. The default Ghidra/REA probe of `TsCameraSDK.dll!TsCPreviewStart` timed out after 180 seconds. A targeted fallback with auto-analysis disabled then successfully decoded 38 instructions and decompiled the 145-byte wrapper. It checks context/SDK state before virtual dispatch; this does not establish physical camera compatibility or image-processing internals. See [camera-boundary evidence](AURA-evoto-camera-boundary-2026-10-21.json).

The installation contains native applications, third-party libraries, camera adapters, configuration carriers and opaque processing resources. Header/resource inspection does not recover model architecture, training data, loss functions, learned skin/body processing or complete processing order. Prior observed Evoto preview evidence remains watermarked; there is no paired native Evoto export to certify identical tone or retouch.

The [earlier feature-family gap inventory](AURA-feature-coverage-2026-10-19.json) remains historical evidence. These 100 additions do not reclassify every Evoto feature family as equivalent or complete.

The current public workflow reference is [Evoto Desktop](https://www.evoto.ai/ai-photo-editor); the documented colour controls are [Evoto Color support](https://support.evoto.ai/color-adjustments-module/). These describe feature families, not source code or independently verified performance.

## Capability-by-capability evidence

|#|Group|New capability|Native qualification|
|---|---|---|---|
|1|colour|Median exposure anchor (`median_anchor`)|pass|
|2|colour|Interquartile contrast fit (`interquartile_contrast`)|pass|
|3|colour|Lower-tail luminance lift (`lower_tail_lift`)|pass|
|4|colour|Upper-tail soft shoulder (`upper_tail_shoulder`)|pass|
|5|colour|Luminance skew correction (`luminance_skew_correct`)|pass|
|6|colour|RMS exposure normalization (`rms_exposure`)|pass|
|7|colour|Harmonic exposure normalization (`harmonic_exposure`)|pass|
|8|colour|Entropy-weighted zone contrast (`entropy_zone_contrast`)|pass|
|9|colour|Sixth-moment illuminant balance (`minkowski_illuminant`)|pass|
|10|colour|Gray-edge illuminant balance (`gray_edge_illuminant`)|pass|
|11|colour|Trimmed illuminant balance (`trimmed_illuminant`)|pass|
|12|colour|Log-ratio illuminant balance (`log_ratio_illuminant`)|pass|
|13|colour|Dominant chroma-mode neutralization (`chroma_mode_neutral`)|pass|
|14|colour|Tonal colour-cast equalization (`tonal_cast_equalize`)|pass|
|15|colour|Covariance colour outlier clipping (`mahalanobis_chroma_clip`)|pass|
|16|colour|Chroma-radius equalization (`chroma_radius_equalize`)|pass|
|17|colour|Sample-guided hue arc pull (`hue_geodesic_pull`)|pass|
|18|colour|Measured palette-axis reflection (`palette_reflection`)|pass|
|19|colour|Median colourfulness normalization (`colorfulness_normalize`)|pass|
|20|colour|Highlight neighbourhood colour recovery (`highlight_hue_restore`)|pass|
|21|colour|Variance-aware chroma coherence (`local_chroma_coherence`)|pass|
|22|colour|Local illumination flattening (`local_illumination_flatten`)|pass|
|23|colour|Local log-ratio colour cleanup (`local_log_ratio_denoise`)|pass|
|24|colour|Circular hue diffusion (`chroma_hue_diffusion`)|pass|
|25|colour|Target-hue colour compression (`hue_sector_compress`)|pass|
|26|spatial|Guided linear smoothing (`guided_linear`)|pass|
|27|spatial|Recursive domain smoothing (`domain_transform`)|pass|
|28|spatial|Iterative deconvolution (`richardson_lucy`)|pass|
|29|spatial|Shock edge sharpening (`shock_sharpen`)|pass|
|30|spatial|Curvature smoothing (`curvature_flow`)|pass|
|31|spatial|Bright component reconstruction (`opening_reconstruct`)|pass|
|32|spatial|Dark component reconstruction (`closing_reconstruct`)|pass|
|33|spatial|Morphological toggle contrast (`toggle_mapping`)|pass|
|34|spatial|Contraharmonic cleanup (`contraharmonic_mean`)|pass|
|35|spatial|Geometric noise smoothing (`geometric_mean`)|pass|
|36|spatial|Huber robust smoothing (`huber_location`)|pass|
|37|spatial|Tukey outlier rejection (`tukey_location`)|pass|
|38|spatial|Plane residual detail (`local_plane_residual`)|pass|
|39|spatial|Luminance-guided chroma repair (`chroma_guided_regression`)|pass|
|40|spatial|Poisson variance smoothing (`anscombe_denoise`)|pass|
|41|spatial|Side-window smoothing (`side_window_mean`)|pass|
|42|spatial|Median-of-means repair (`median_of_means`)|pass|
|43|spatial|Range-separated detail (`bilateral_sharpen`)|pass|
|44|spatial|Radial diffraction halo (`airy_diffraction`)|pass|
|45|spatial|Annular aperture blur (`annular_bokeh`)|pass|
|46|spatial|Focus-plane motion blur (`tilt_shift_integration`)|pass|
|47|spatial|Radial zoom integration (`zoom_integration`)|pass|
|48|spatial|Radial channel correction (`radial_chromatic`)|pass|
|49|spatial|Local projective plane (`perspective_plane`)|pass|
|50|spatial|Compact radial warp (`thin_plate_warp`)|pass|
|51|selection|Circular hue arc (`hue_arc`)|pass|
|52|selection|Opponent colourfulness (`opponent_chroma`)|pass|
|53|selection|Red–blue balance (`temperature_balance`)|pass|
|54|selection|Channel clipping fraction (`channel_clipping`)|pass|
|55|selection|Neutral-axis proximity (`neutral_axis`)|pass|
|56|selection|Local contrast polarity (`local_contrast_polarity`)|pass|
|57|selection|Local tonal entropy (`local_entropy`)|pass|
|58|selection|Laplacian detail (`laplacian_detail`)|pass|
|59|selection|Oriented luminance edges (`edge_orientation`)|pass|
|60|selection|Structure tensor coherence (`tensor_coherence`)|pass|
|61|selection|Harris corner response (`harris_corners`)|pass|
|62|selection|Local luminance rank (`local_rank`)|pass|
|63|selection|Chromatic boundaries (`chromatic_boundary`)|pass|
|64|selection|Neutral bright residual (`specular_residual`)|pass|
|65|selection|Shadow chroma deviation (`shadow_chroma_deviation`)|pass|
|66|selection|Superellipse marquee (`superellipse_marquee`)|pass|
|67|selection|Cubic Bézier ribbon (`bezier_ribbon`)|pass|
|68|selection|Concave star marquee (`star_marquee`)|pass|
|69|selection|Continuous foreground/background seed field (`seed_field`)|pass|
|70|selection|Bilateral overlap (`bilateral_symmetry`)|pass|
|71|selection|Periodic selection tiles (`periodic_selection`)|pass|
|72|selection|Polar coverage unwrap (`polar_selection`)|pass|
|73|selection|Coverage quantile equalization (`coverage_quantiles`)|pass|
|74|selection|Harmonic coverage patch repair (`harmonic_patch`)|pass|
|75|selection|Principal-axis region strip (`region_axis_strip`)|pass|
|76|inspection|Centre of light (`light-centroid`)|pass|
|77|inspection|Centre of detail (`detail-centroid`)|pass|
|78|inspection|Quadrant light balance (`quadrant-light`)|pass|
|79|inspection|Border light energy (`border-light-energy`)|pass|
|80|inspection|Thirds line detail mass (`thirds-edge-mass`)|pass|
|81|inspection|Mirror asymmetry (`mirror-residual`)|pass|
|82|inspection|Bright region connectivity (`bright-components`)|pass|
|83|inspection|Radial light profile (`radial-light-profile`)|pass|
|84|inspection|Curvature sign transitions (`laplacian-crossings`)|pass|
|85|inspection|Horizontal edge support (`edge-effective-width`)|pass|
|86|inspection|Edge overshoot residual (`edge-overshoot`)|pass|
|87|inspection|Horizontal signal autocorrelation (`luma-autocorrelation`)|pass|
|88|inspection|Diagonal parity residual (`diagonal-phase`)|pass|
|89|inspection|Directional residual correlation (`residual-correlation`)|pass|
|90|inspection|Decimated Haar energy (`haar-energy`)|pass|
|91|inspection|Edge position covariance (`edge-spread`)|pass|
|92|inspection|Boundary edge activity (`border-edge-arrivals`)|pass|
|93|inspection|Channel information dependence (`channel-information`)|pass|
|94|inspection|Working RGB palette cube (`palette-cube`)|pass|
|95|inspection|Circular hue energy (`hue-energy`)|pass|
|96|inspection|Chroma and light distribution (`chroma-light-joint`)|pass|
|97|inspection|Neutral cast by luminance (`neutral-cast-trend`)|pass|
|98|inspection|Chroma covariance (`chroma-covariance`)|pass|
|99|inspection|Tonal asymmetry and tails (`tonal-moments`)|pass|
|100|inspection|Missing level runs (`level-gap-runs`)|pass|

Individual formulas, source files and native tests are in [the 100-item implementation ledger](AURA-next-hundred-capabilities-2026-10-21.json). Full outcomes, hashes, commands and per-photo results are in [the validation JSON](AURA-next-hundred-validation-2026-10-21.json). Native cases and command coverage are exported as adjacent CSV files. Detailed raw logs and complete REA envelopes remain under `.work-checks/aura-next100/`.

Spatial methods remain bounded-grid approximations; inspection uses small previews. Fine skin detail, print-size artifacts and subjective portrait quality need full-resolution human review. Camera hardware, other platforms, optional models and external services are outside this fresh desktop qualification.
