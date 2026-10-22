# AURA photo inspection

The Photo inspection panel computes twenty original read-only diagnostics from the current saved edit. These are AURA implementations and do not claim to reproduce Evoto algorithms, model weights, or learned skin behavior.

| Tool | Working output |
| --- | --- |
| RGB parade | Three channel density scopes |
| Luminance waveform | Horizontal luminance density scope |
| Chroma vectorscope | Two-dimensional opponent-chroma density scope |
| Exposure zone map | Spatial exposure in stops from 18% working gray |
| Working range excursions | Spatial fraction of pixels outside nominal channel range |
| Spatial edge detail | Regional gradient magnitude |
| Flat-area residual | High frequency residual from low-gradient regions |
| Near-neutral colour balance | RGB/luminance ratios from low-chroma midtones |
| Local contrast map | Regional coefficient of luminance variation |
| Rendered tonal range | Five luminance percentiles and a 1–99% span in stops |
| Exposure zone distribution | Fifteen one-stop bins, including two tails |
| sRGB working gamut occupancy | In-range, negative-channel and above-white shares |
| Row and column banding profile | Second differences of mean row/column luminance |
| Eight-pixel block boundary profile | Adjacent differences by modulo-eight boundary |
| Edge direction balance | Gradient energy across twelve orientations |
| Relative illumination map | Regional exposure offset from the frame average |
| Neutral-edge channel alignment | Red/blue horizontal centroid offsets against green |
| Channel correlation | Three Pearson channel-pair coefficients |
| Detail scale spectrum | Residual magnitude at five neighborhood radii |
| Highlight chroma retention | Mean relative chroma in three luminance ranges |

The native command validates photo membership and saved recipe hash before rendering and again checks the hash before returning. It uses `CatalogFrames` and the renderer's saved-edit working-buffer pipeline at a maximum 512-pixel edge, including retouch and geometry. It never saves recipes or writes imported originals. Preview cache population is an ordinary read-side service effect. Each visualization has at most 4,608 values; malformed, incomplete, oversized, and nonfinite buffers are rejected. UI results are cleared when photo or saved revision changes, and late responses are discarded.

Scopes use the renderer's curve-domain Rec.2020 values. Exposure and spatial measurements use linear Rec.2020. Working range excursions and pre-display sRGB occupancy are explicitly **not display clipping reports**. Noise, stripes, block boundaries, alignment and illumination are preview diagnostics: scene content can contribute, and preview resampling changes pixel-scale signatures. The panel explains these limits beside each measurement and advises 100% original inspection. It performs no automatic edits.

Native tests cover unique/finite/bounded outputs, untouched input buffers, neutral behavior, known exposure ramps/correlation, noise residuals, periodic block discontinuities, payload size, malformed input and tiny black images. UI tests cover explicit invocation, scope navigation, missing samples, stale results, navigation, revision invalidation and native errors.
