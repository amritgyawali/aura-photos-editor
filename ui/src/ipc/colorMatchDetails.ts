export const FINE_MATCH_FIELDS = [
  ['exposure', 'Exposure match'], ['contrast', 'Contrast match'], ['highlights', 'Highlights match'],
  ['shadows', 'Shadows match'], ['whites', 'Whites match'], ['blacks', 'Blacks match'],
  ['curves', 'Curve match'], ['temperature', 'Temperature match'], ['tint', 'Tint match'],
  ['vibrance', 'Vibrance match'], ['saturation', 'Saturation match'], ['hue', 'Hue match'],
] as const;
export type FineMatchControls = Partial<Record<typeof FINE_MATCH_FIELDS[number][0], number>>;
