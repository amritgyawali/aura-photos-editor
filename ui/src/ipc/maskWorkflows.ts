import { invoke } from './client';
import type { LocalMasksDto } from './localMasks';
import type { MaskMode, MaskSource } from './localMasks';

export type MaskWorkflowAction = 'duplicate' | 'invert' | 'union' | 'intersection' | 'subtraction' | 'freeze' | 'feather' | 'expand' | 'contract' | 'edge_contrast' | 'remove_islands' | 'fill_holes'
  | 'edit_linear' | 'edit_radial' | 'edit_brush' | 'edit_luminance' | 'component_mode' | 'reorder_component' | 'extract_component' | 'copy_component'
  | 'transform_selection' | 'border_selection' | 'keep_largest' | 'keep_connected' | 'median_cleanup' | 'exclusive_union' | 'convex_hull'
  | 'polygon_marquee' | 'rounded_marquee' | 'sector_marquee' | 'sample_colour' | 'colour_wand' | 'sample_luminance' | 'saturation_range' | 'texture_range' | 'image_edges' | 'guided_smooth' | 'edge_grow'
  | 'interior_falloff' | 'negative_space' | 'skeleton' | 'split_regions' | 'append_stack' | 'replace_selection' | 'sync_adjustments' | 'align_centroid' | 'fit_bounds' | 'perspective_selection' | 'auto_threshold' | 'chroma_key' | 'voronoi_partition' | 'filter_regions'
  | 'hue_arc' | 'opponent_chroma' | 'temperature_balance' | 'channel_clipping' | 'neutral_axis' | 'local_contrast_polarity' | 'local_entropy' | 'laplacian_detail' | 'edge_orientation' | 'tensor_coherence' | 'harris_corners' | 'local_rank' | 'chromatic_boundary' | 'specular_residual' | 'shadow_chroma_deviation'
  | 'superellipse_marquee' | 'bezier_ribbon' | 'star_marquee' | 'seed_field' | 'bilateral_symmetry' | 'periodic_selection' | 'polar_selection' | 'coverage_quantiles' | 'harmonic_patch' | 'region_axis_strip'
  | 'lab_sample_distance' | 'mahalanobis_colour' | 'geodesic_colour' | 'local_otsu' | 'sauvola_threshold' | 'gabor_response' | 'directional_ridge' | 'local_skewness' | 'local_kurtosis' | 'colour_line_residual' | 'coverage_opening' | 'coverage_closing' | 'morphological_reconstruction' | 'grayscale_area_opening' | 'border_connected' | 'geodesic_rings' | 'hole_bearing_regions' | 'regional_maxima' | 'spiral_marquee' | 'barycentric_triangle' | 'coverage_shear' | 'coverage_twirl' | 'coverage_spherize' | 'coverage_displace' | 'signed_distance_smooth'
  | 'median_absolute_deviation' | 'binary_pattern_transitions' | 'patch_autocorrelation' | 'patch_semivariance' | 'log_chroma_distance' | 'edge_nonmaximum' | 'edge_hysteresis' | 'local_moran' | 'template_patch_correlation' | 'local_hue_dispersion' | 'colour_prototype_partition' | 'selection_articulations' | 'selection_k_core' | 'coverage_watershed' | 'coverage_anisotropic_diffusion' | 'coverage_curvature_flow' | 'region_inscribed_disk' | 'directional_run_width' | 'thin_plate_coverage_warp' | 'mean_value_cage_warp' | 'triangulated_cage_warp' | 'mobius_coverage_warp' | 'path_normal_unwrap' | 'coverage_seam_carve' | 'lissajous_ribbon';
export type MaskLabOptions = { vertices: [number, number][]; bounds: [number, number, number, number]; range: [number, number]; tolerance: number; radius: number; corner: number; angle: number; sweep: number; seedIndex: number; minArea: number; maxArea: number; compactness: number };
export type MaskWorkflowRequest = { action: MaskWorkflowAction; otherMaskId?: string; value?: number; componentIndex?: number; targetIndex?: number; source?: MaskSource; mode?: MaskMode; point?: [number, number]; transform?: { offset: [number, number]; scale: [number, number]; angle: number }; advanced?: MaskLabOptions };
export const maskWorkflows = {
  apply: (projectId: string, photoId: string, maskId: string, recipeHash: string, request: MaskWorkflowRequest, cancelId: string) =>
    invoke<LocalMasksDto>('mask_workflow', { input: { projectId, photoId, maskId, recipeHash, cancelId, ...request, otherMaskId: request.otherMaskId ?? null, value: request.value ?? 0 } }),
};
