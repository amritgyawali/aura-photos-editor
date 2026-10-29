import { invoke } from './client';
import type { RecipeDto } from './types';
export const COLOR_LAB_TOOLS = [
  ['spatial_neutral_bridge', 'Two-neutral spatial bridge', 'Measure two neutral reference points and interpolate their chromatic adaptation along the line between them.'],
  ['tonal_neutral_bridge', 'Two-neutral tonal bridge', 'Interpolate two measured neutral corrections by photographed luminance, preserving luminance.'],
  ['chroma_reject', 'Sampled colour-vector rejection', 'Project the selected colour away from the measured sample vector in luminance-normalized colour coordinates.'],
  ['palette_axis', 'Measured palette-axis projection', 'Project colours onto the line through two measured palette colours, preserving luminance.'],
  ['decorrelation_stretch', 'Covariance decorrelation stretch', 'Whiten the two measured chroma principal axes toward a chosen chroma spread, with bounded gain.'],
  ['principal_chroma_suppress', 'Dominant chroma-axis suppression', 'Suppress the selected colour distribution’s principal variation while retaining its mean and orthogonal variation.'],
  ['soft_gamut', 'Soft gamut-ray compression', 'Use a smooth tanh shoulder to shorten colour rays near the nominal white boundary; photographed luminance stays fixed.'],
  ['luma_chroma_compress', 'Brightness-weighted chroma compression', 'Compress intense colour increasingly with photographed brightness and colour radius.'],
  ['chroma_cusp', 'Radial chroma cusp compression', 'Compress rays beyond the chosen chroma radius using their RGB boundary geometry.'],
  ['neutral_exclusion', 'Near-neutral chroma exclusion', 'Suppress weak colour near the neutral axis while retaining stronger intentional colour.'],
  ['histogram_equalize', 'Selected luminance equalization', 'Redistribute selected luminance by its measured cumulative distribution.'],
  ['clipped_equalize', 'Clipped-histogram equalization', 'Cap strong histogram peaks and redistribute their weight before computing the luminance map.'],
  ['log_equalize', 'Log-luminance equalization', 'Equalize in an invertible stop domain and return to linear light.'],
  ['rank_gaussian', 'Rank logistic normalization', 'Map measured luminance ranks through a symmetric logistic quantile with bounded output.'],
  ['bimodal_separate', 'Bimodal tone separation', 'Find the Otsu split of the selected histogram and map its two cumulative populations separately.'],
  ['percentile_remap', 'Percentile landmark remapping', 'Map measured 10th, 50th and 90th luminance landmarks to evenly spaced interior anchors.'],
  ['photographic_map', 'Log-average photographic mapping', 'Normalize by selected geometric-mean luminance and apply a rational highlight shoulder.'],
  ['asinh_map', 'Asinh dynamic-range mapping', 'Use an inverse-hyperbolic-sine light map normalized at unit white.'],
  ['log_toe_shoulder', 'Logarithmic toe and shoulder', 'Combine a normalized log shoulder with a continuous rational toe.'],
  ['local_log_contrast', 'Local log-contrast equalization', 'Boost bounded local stop residuals around a selected-only illumination estimate.'],
  ['retinex_ratio', 'Selected Retinex ratio', 'Normalize luminance by local geometric illumination with an explicit stabilizing anchor.'],
  ['chroma_edge_protect', 'Edge-protected colour enrichment', 'Increase local chroma in calm luminance regions while suppressing enrichment across strong local light changes.'],
  ['chroma_winsorize', 'Chroma outlier Winsorization', 'Clamp local chroma outliers to measured neighborhood variance bounds.'],
  ['opponent_contrast', 'Local opponent-colour contrast', 'Increase chroma deviations from the selected-only local mean, retaining photographed luminance.'],
  ['sampled_replace', 'Sample-bound chroma replacement', 'Shift colours near a measured source toward a chosen linear working-space target with a soft distance gate.'],
  ["median_anchor", "Median exposure anchor", "Normalize selected median luminance to the chosen linear anchor; gain is bounded to three stops."],
  ["interquartile_contrast", "Interquartile contrast fit", "Fit a linear contrast slope to selected 25th and 75th percentile luminance around the median."],
  ["lower_tail_lift", "Lower-tail luminance lift", "Lift shadows with a monotone rational gain measured from the selected tenth percentile, retaining exact black."],
  ["upper_tail_shoulder", "Upper-tail soft shoulder", "Roll luminance above the selected ninetieth percentile into a smooth tanh shoulder."],
  ["luminance_skew_correct", "Luminance skew correction", "Measure selected luminance skew and counterbalance it with a bounded monotone power map."],
  ["rms_exposure", "RMS exposure normalization", "Normalize selected root-mean-square luminance to an explicit linear anchor."],
  ["harmonic_exposure", "Harmonic exposure normalization", "Use a dark-sensitive harmonic luminance estimate for bounded exposure normalization."],
  ["entropy_zone_contrast", "Entropy-weighted zone contrast", "Allocate contrast across four measured luminance zones according to their conditional entropy."],
  ["minkowski_illuminant", "Sixth-moment illuminant balance", "Estimate channel illumination from sixth-power selected channel moments, then correct at constant luminance."],
  ["gray_edge_illuminant", "Gray-edge illuminant balance", "Estimate an illuminant from selected horizontal and vertical channel gradients; excluded pixels cannot influence it."],
  ["trimmed_illuminant", "Trimmed illuminant balance", "Discard the lowest and highest ten percent of channel samples before estimating the illuminant."],
  ["log_ratio_illuminant", "Log-ratio illuminant balance", "Balance the geometric mean of red/green and blue/green ratios while holding photographed luminance."],
  ["chroma_mode_neutral", "Dominant chroma-mode neutralization", "Find the most populated chroma cell and neutralize its measured center at constant luminance."],
  ["tonal_cast_equalize", "Tonal colour-cast equalization", "Measure chroma in four luminance zones and smoothly equalize their casts toward the overall selected chroma."],
  ["mahalanobis_chroma_clip", "Covariance colour outlier clipping", "Clip chroma outliers to an ellipse fitted to selected covariance, leaving interior colours untouched."],
  ["chroma_radius_equalize", "Chroma-radius equalization", "Map selected chroma-radius ranks to an explicit colour radius while retaining hue direction."],
  ["hue_geodesic_pull", "Sample-guided hue arc pull", "Pull hues near the sampled hue toward a target along the shortest angular arc, retaining colour radius."],
  ["palette_reflection", "Measured palette-axis reflection", "Reflect chroma across the line through two sampled palette colours at constant luminance."],
  ["colorfulness_normalize", "Median colourfulness normalization", "Fit selected median chroma radius to an explicit scale without rotating photographed hue."],
  ["highlight_hue_restore", "Highlight neighbourhood colour recovery", "Blend bright pixels toward selected neighbouring chroma; this is a local estimate and cannot recover missing sensor data."],
  ["local_chroma_coherence", "Variance-aware chroma coherence", "Reduce local colour fluctuations in coherent regions using a measured variance gate."],
  ["local_illumination_flatten", "Local illumination flattening", "Compress slow selected log-illumination variation toward its global geometric mean while retaining local residuals."],
  ["local_log_ratio_denoise", "Local log-ratio colour cleanup", "Average selected red/green and blue/green logarithmic ratios and reconstruct at original luminance."],
  ["chroma_hue_diffusion", "Circular hue diffusion", "Average neighbouring hue directions on the unit circle while retaining each pixel colour radius."],
  ["hue_sector_compress", "Target-hue colour compression", "Compress chroma within a smooth angular sector centered on an explicit target hue."],
  ['sigmoid_contrast', 'Normalized sigmoid contrast', 'Apply a normalized logistic curve in bounded luminance coordinates, preserving black and HDR output.'],
  ['exponential_shoulder', 'Exponential highlight rolloff', 'Use an exponential absorption shoulder normalized at unit white.'],
  ['hermite_tone', 'Monotone Hermite contrast', 'Shape a cubic Hermite contrast curve with matching endpoint slopes and a continuous HDR extension.'],
  ['schlick_bias', 'Rational light bias', 'Redistribute linear luminance with a rational bias curve and continuous highlight extension.'],
  ['solarize_luma', 'Luminance solarization', 'Fold luminance above an explicit threshold into an exponential creative solarization shoulder.'],
  ['zone_exposure', 'Stop-zone exposure lift', 'Apply a Gaussian exposure lift around a chosen stop-domain luminance anchor.'],
  ['black_floor', 'Linear black-floor subtraction', 'Subtract an explicit linear black floor and rescale the remaining luminance.'],
  ['dark_channel_dehaze', 'Selected dark-channel haze removal', 'Estimate local transmission from selected dark-channel minima and selected 99th-percentile atmospheric channel values.'],
  ['local_wiener_tone', 'Variance-guided luminance cleanup', 'Shrink local luminance residuals using the Wiener ratio of measured variance to variance plus noise power.'],
  ['local_midrange', 'Local extrema tone contraction', 'Contract luminance toward the midpoint of selected neighbourhood extrema.'],
  ['chroma_shear', 'Opponent-plane colour shear', 'Shear red-opponent chroma by blue-opponent chroma while retaining photographed luminance.'],
  ['chroma_spiral', 'Radius-dependent hue spiral', 'Rotate hue by a continuous angle proportional to colour radius, keeping neutral colours stable.'],
  ['tonal_hue_twist', 'Luminance-dependent hue twist', 'Rotate shadows and highlights in opposing hue directions around a luminance anchor.'],
  ['duotone_gradient', 'Complementary duotone gradient', 'Rebuild chroma along a complementary shadow-to-highlight colour axis at original luminance.'],
  ['channel_crossfeed', 'Cyclic channel crossfeed', 'Mix each linear channel with its cyclic neighbour and restore original luminance.'],
  ['selective_density', 'Subtractive colour density', 'Apply Beer-Lambert channel absorption from an explicit density colour and restore original luminance.'],
  ['bradford_adaptation', 'Bradford cone-response adaptation', 'Adapt the selected mean illuminant to an explicit target using XYZ Bradford cone responses, retaining luminance.'],
  ['channel_ratio_limit', 'RGB channel-ratio limiter', 'Limit peak channels against the darkest channel with a luminance-based stabilizing floor.'],
  ['median_despill', 'Median-channel spill suppression', 'Reduce only channel excursions above the per-pixel median channel and restore luminance.'],
  ['harmonic_hue_snap', 'Three-axis hue harmony', 'Move hue toward a target-aligned threefold harmonic harmony using a continuous sinusoidal angular field.'],
  ['chroma_luma_detrend', 'Colour-luminance regression removal', 'Measure selected chroma-versus-luminance regression and remove its slope around the selected mean.'],
  ['guided_chroma', 'Luminance-guided colour cleanup', 'Fit local chroma to luminance with ridge-regularized selected covariance, preserving aligned colour edges.'],
  ['median_chroma', 'Weighted median colour repair', 'Replace opponent chroma with the weighted selected neighbourhood median, preserving luminance.'],
  ['channel_rank_align', 'Channel-distribution alignment', 'Map each channel rank to the luminance-weighted barycenter of selected channel quantile distributions, preserving luminance.'],
  ['bilateral_chroma', 'Range-aware bilateral colour cleanup', 'Average selected chroma with both spatial and chromatic range weights at constant luminance.'],
  ['spatial_chroma_plane', 'Measured spatial cast removal', 'Fit two selected-only affine chroma surfaces and remove their spatial slopes while retaining the selected mean colour.'],
  ['radial_log_illumination', 'Measured radial illumination correction', 'Fit log-luminance against squared distance from an authored optical centre and remove the fitted radial variation.'],
  ['gaussian_palette_transport', 'Gaussian patch palette transport', 'Compute the symmetric Gaussian optimal-transport map between two selected patch chroma covariances and means.'],
  ['palette_simplex', 'Three-anchor palette constraint', 'Project chroma onto the closest point of the triangle defined by two selected patches and an explicit target colour.'],
  ['hue_density_equalize', 'Measured circular hue redistribution', 'Compute a selected hue CDF with a seam in the least populated angular bin and redistribute hue into an authored angular span.'],
  ['chroma_copula_map', 'Chroma marginal copula remapping', 'Map the two selected opponent-chroma marginal ranks through logistic quantiles while retaining luminance.'],
  ['hue_radius_balance', 'Hue-dependent colourfulness balance', 'Fit log colour radius to first circular harmonics and remove its measured dependence on hue.'],
  ['kmeans_palette_recolor', 'Cluster-residual palette recolouring', 'Fit three deterministic Lloyd clusters and translate each cluster toward authored anchors while retaining within-cluster residuals.'],
  ['channel_regression_recover', 'Measured channel-response reconstruction', 'Fit channel regressions from selected below-threshold pixels and estimate above-threshold channel values from the other channels.'],
  ['kubelka_munk_mix', 'Kubelka-Munk pigment mixing', 'Interpolate reflectance-derived absorption-to-scattering ratios and invert the Kubelka-Munk relation; this is a creative reflectance approximation.'],
  ['pca_monochrome', 'Variance-preserving principal monochrome', 'Project selected RGB covariance onto its dominant eigenvector and normalize projected variance to measured luminance variance.'],
  ['fisher_monochrome', 'Two-patch discriminant monochrome', 'Solve a regularized Fisher discriminant from two selected patch distributions to separate their colours in monochrome.'],
  ['isoluminant_edge_light', 'Isoluminant colour-edge lighting', 'Convert selected chroma differences unsupported by luminance contrast into a bounded luminance lift.'],
  ['luminance_error_diffusion', 'Selected error-diffusion tones', 'Quantize linear luminance and redistribute quantization error to selected forward neighbours with Floyd-Steinberg weights.'],
  ['amplitude_dotscreen', 'Area-modulated photographic dots', 'Render a spatial dot screen whose covered area follows photographed luminance, with antialiased dot boundaries.'],
  ['iso_luma_contours', 'Stop-domain luminance contours', 'Render periodic contour bands on log-luminance isosurfaces, retaining the photographed colour direction.'],
  ['exposure_fusion', 'Analytical exposure fusion', 'Fuse three bounded synthetic exposure renderings using their measured well-exposedness weights.'],
  ['perceptual_lightness_lift', 'Constant-Lab-chroma lightness lift', 'Lift CIE Lab lightness with a bounded bell while keeping Lab a and b fixed, then return to linear Rec.2020.'],
  ['uv_cast_translate', 'Uniform-chromaticity cast translation', 'Translate CIE u-prime/v-prime chromaticity from the selected mean toward a target while retaining luminance.'],
  ['chroma_group_lasso', 'Robust group-lasso chroma cleanup', 'Estimate a selected robust chroma noise scale and apply the vector group-lasso proximal operator.'],
  ['geometric_median_neutral', 'Geometric-median cast balance', 'Estimate the continuous weighted chroma geometric median with fixed-count Weiszfeld updates and remove that robust cast.'],
  ['aitchison_tint', 'Compositional-geodesic colour tint', 'Interpolate channel compositions along an Aitchison log-ratio geodesic toward a chosen target, retaining luminance.'],
  ['energy_conserving_lightness', 'Fixed-energy lightness redistribution', 'Solve an exponential-tilting KL projection toward a luminance anchor while preserving the total RGB channel energy.'],
  ['dichromatic_specular', 'Dichromatic reflection subtraction', 'Fit a two-basis diffuse-plus-illuminant reflection model and subtract a bounded specular component.'],
  ['mirror_hue_fold', 'Mirrored palette hue folding', 'Fold hue through a repeated angular reflection into a bounded palette sector while retaining chroma radius and luminance.'],
] as const;
export type ColorLabTool = typeof COLOR_LAB_TOOLS[number][0];
export type ColorLabOperation = { id: string; tool: ColorLabTool; maskId: string; enabled: boolean; amount: number; strength: number; radius: number; pivot: number; width: number; source: [number, number]; second: [number, number]; target: [number, number, number] };
export const COLOR_LAB_SOURCE_TOOLS: ColorLabTool[] = ['radial_log_illumination', 'gaussian_palette_transport', 'palette_simplex', 'kmeans_palette_recolor', 'fisher_monochrome', 'dichromatic_specular', 'spatial_neutral_bridge', 'tonal_neutral_bridge', 'chroma_reject', 'palette_axis', 'sampled_replace', 'hue_geodesic_pull', 'palette_reflection'];
export const COLOR_LAB_SECOND_TOOLS: ColorLabTool[] = ['gaussian_palette_transport', 'palette_simplex', 'kmeans_palette_recolor', 'fisher_monochrome', 'spatial_neutral_bridge', 'tonal_neutral_bridge', 'palette_axis', 'palette_reflection'];
export const COLOR_LAB_LOCAL_TOOLS: ColorLabTool[] = ['gaussian_palette_transport', 'palette_simplex', 'kmeans_palette_recolor', 'fisher_monochrome', 'isoluminant_edge_light', 'amplitude_dotscreen', 'dichromatic_specular', 'dark_channel_dehaze', 'local_wiener_tone', 'local_midrange', 'guided_chroma', 'median_chroma', 'bilateral_chroma', 'local_log_contrast', 'retinex_ratio', 'chroma_edge_protect', 'chroma_winsorize', 'opponent_contrast', 'highlight_hue_restore', 'local_chroma_coherence', 'local_illumination_flatten', 'local_log_ratio_denoise', 'chroma_hue_diffusion'];
export function colorLabOf(recipe: RecipeDto | null): ColorLabOperation[] {
  try { const value = (JSON.parse(recipe?.body ?? '{}') as { color_lab_v1?: ColorLabOperation[] }).color_lab_v1; return Array.isArray(value) ? value : []; } catch { return []; }
}
export const colorLab = {
  save: (projectId: string, photoId: string, recipeHash: string, operations: ColorLabOperation[]) => invoke<{ recipe: RecipeDto }>('save_color_lab', { input: { projectId, photoId, recipeHash, operations } }),
};


export const COLOR_LAB_PIVOT_TOOLS: ColorLabTool[] = ['channel_regression_recover', 'isoluminant_edge_light', 'amplitude_dotscreen', 'iso_luma_contours', 'exposure_fusion', 'energy_conserving_lightness', 'sigmoid_contrast', 'solarize_luma', 'zone_exposure', 'black_floor', 'tonal_hue_twist', 'duotone_gradient', "photographic_map", "log_toe_shoulder", "retinex_ratio", "luma_chroma_compress", "median_anchor", "interquartile_contrast", "lower_tail_lift", "rms_exposure", "harmonic_exposure", "highlight_hue_restore"];
export const COLOR_LAB_SCALE_TOOLS: ColorLabTool[] = ['spatial_chroma_plane', 'radial_log_illumination', 'gaussian_palette_transport', 'hue_density_equalize', 'chroma_copula_map', 'hue_radius_balance', 'channel_regression_recover', 'kubelka_munk_mix', 'fisher_monochrome', 'isoluminant_edge_light', 'luminance_error_diffusion', 'iso_luma_contours', 'exposure_fusion', 'perceptual_lightness_lift', 'chroma_group_lasso', 'aitchison_tint', 'dichromatic_specular', 'mirror_hue_fold', 'sigmoid_contrast', 'exponential_shoulder', 'hermite_tone', 'schlick_bias', 'solarize_luma', 'zone_exposure', 'dark_channel_dehaze', 'local_wiener_tone', 'local_midrange', 'chroma_shear', 'chroma_spiral', 'tonal_hue_twist', 'channel_crossfeed', 'selective_density', 'channel_ratio_limit', 'median_despill', 'harmonic_hue_snap', 'guided_chroma', 'bilateral_chroma', "decorrelation_stretch", "soft_gamut", "luma_chroma_compress", "chroma_cusp", "neutral_exclusion", "rank_gaussian", "asinh_map", "log_toe_shoulder", "local_log_contrast", "chroma_edge_protect", "chroma_winsorize", "opponent_contrast", "sampled_replace", "interquartile_contrast", "lower_tail_lift", "upper_tail_shoulder", "luminance_skew_correct", "mahalanobis_chroma_clip", "chroma_radius_equalize", "hue_geodesic_pull", "colorfulness_normalize", "highlight_hue_restore", "local_chroma_coherence", "local_illumination_flatten", "hue_sector_compress"];
export const COLOR_LAB_TARGET_TOOLS: ColorLabTool[] = ['palette_simplex', 'hue_density_equalize', 'kmeans_palette_recolor', 'kubelka_munk_mix', 'uv_cast_translate', 'aitchison_tint', 'dichromatic_specular', 'mirror_hue_fold', 'duotone_gradient', 'selective_density', 'bradford_adaptation', 'harmonic_hue_snap', 'sampled_replace', 'hue_geodesic_pull', 'hue_sector_compress'];

export function colorLabGroup(tool: ColorLabTool): string {
  const index = COLOR_LAB_TOOLS.findIndex(entry => entry[0] === tool);
  if (index < 25) return "Foundations";
  if (index < 50) return "Measured corrections";
  if (index < 60) return "Tone and local light";
  if (index < 70) return "Colour design and adaptation";
  if (index < 75) return "Colour statistics and cleanup";
  if (index < 84) return "Measured colour models";
  if (index < 94) return "Colour rendering and interpretation";
  return "Colour constraints and compositing";
}

const ADVANCED_CONTROL_LABELS: Partial<Record<ColorLabTool, Partial<Record<'width' | 'pivot' | 'radius', string>>>> = {
  spatial_chroma_plane: { width: 'Spatial cast correction scale' },
  radial_log_illumination: { width: 'Radial illumination correction scale' },
  gaussian_palette_transport: { width: 'Transport covariance regularization', radius: 'Reference patch reach' },
  palette_simplex: { radius: 'Palette reference patch reach' },
  hue_density_equalize: { width: 'Redistributed hue span' },
  chroma_copula_map: { width: 'Marginal chroma spread' },
  hue_radius_balance: { width: 'Hue colourfulness correction scale' },
  kmeans_palette_recolor: { radius: 'Palette anchor patch reach' },
  channel_regression_recover: { pivot: 'Unclipped training threshold linear', width: 'Channel reconstruction transition' },
  kubelka_munk_mix: { width: 'Pigment mixture ratio scale' },
  fisher_monochrome: { width: 'Discriminant contrast and regularization', radius: 'Discriminant reference patch reach' },
  isoluminant_edge_light: { radius: 'Colour edge measurement reach', pivot: 'Edge luminance stabilizer linear', width: 'Colour edge light gain' },
  luminance_error_diffusion: { width: 'Linear luminance quantization step' },
  amplitude_dotscreen: { radius: 'Dot screen cell reach', pivot: 'Dot screen white luminance linear' },
  iso_luma_contours: { pivot: 'Contour luminance anchor linear', width: 'Contour spacing in stops' },
  exposure_fusion: { pivot: 'Fusion well-exposedness centre', width: 'Fusion well-exposedness width' },
  perceptual_lightness_lift: { width: 'Lab lightness lift scale' },
  chroma_group_lasso: { width: 'Robust chroma shrinkage scale' },
  aitchison_tint: { width: 'Compositional tint ratio scale' },
  energy_conserving_lightness: { pivot: 'Fixed-energy target luminance linear' },
  dichromatic_specular: { radius: 'Diffuse reference patch reach', width: 'Specular subtraction ratio scale' },
  mirror_hue_fold: { width: 'Mirrored hue sector half-width scale' },
};
export function colorLabControlLabel(tool: ColorLabTool, field: 'width' | 'pivot' | 'radius', fallback: string): string {
  return ADVANCED_CONTROL_LABELS[tool]?.[field] ?? fallback;
}
