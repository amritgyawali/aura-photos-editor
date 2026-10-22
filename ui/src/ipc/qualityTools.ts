/** Native read-only measurements, grouped for review without duplicating render logic. */
export const QUALITY_GROUPS = [
  ['Palette & tone review', [
    'channel-information', 'palette-cube', 'hue-energy', 'chroma-light-joint',
    'neutral-cast-trend', 'chroma-covariance', 'tonal-moments', 'level-gap-runs',
  ]],
  ['Detail review', [
    'laplacian-crossings', 'edge-effective-width', 'edge-overshoot', 'luma-autocorrelation',
    'diagonal-phase', 'residual-correlation', 'haar-energy', 'edge-spread', 'border-edge-arrivals',
  ]],
  ['Composition review', [
    'light-centroid', 'detail-centroid', 'quadrant-light', 'border-light-energy',
    'thirds-edge-mass', 'mirror-residual', 'bright-components', 'radial-light-profile',
  ]],
  ['Delivery statistics', [
    'robust-tone-quantiles', 'median-deviation', 'luminance-inequality', 'hue-concentration',
    'tone-hue-drift', 'highlight-headroom', 'chroma-tail', 'exposure-locations',
    'highlight-rolloff', 'gamut-excursion-energy',
  ]],
  ['Surface review', [
    'local-colour-jumps', 'structure-coherence', 'hessian-signature', 'gradient-tailedness',
    'edge-polarity', 'normalized-total-variation', 'dct-energy', 'chroma-edge-alignment',
    'residual-asymmetry', 'colour-variogram',
  ]],
  ['Lighting geometry', [
    'local-bit-entropy', 'illumination-plane', 'illumination-curvature', 'edge-euler', 'edge-box-dimension',
  ]],
  ['Texture fidelity', [
    'ordinal-texture-entropy', 'sample-texture-entropy', 'quantized-run-lengths', 'cooccurrence-texture',
    'bright-mass-lacunarity', 'harris-corner-response', 'local-plane-residual', 'line-spectral-flatness',
    'neighbour-moran-index', 'robust-neighbour-roughness',
  ]],
  ['Colour stability', [
    'log-ratio-neutrality', 'white-patch-illuminant', 'gray-edge-illuminant', 'opponent-colourfulness',
    'hue-gap-coverage', 'local-hue-winding', 'chroma-light-information', 'patch-chroma-dispersion',
  ]],
  ['Delivery integrity', [
    'tukey-tail-support', 'channel-tail-dependence', 'patch-mean-variance-fit', 'clipping-component-support',
    'edge-clipping-overlap', 'robust-strip-drift', 'thumbnail-alias-sensitivity',
  ]],
] as const;
export type QualityGroup = typeof QUALITY_GROUPS[number][0];
