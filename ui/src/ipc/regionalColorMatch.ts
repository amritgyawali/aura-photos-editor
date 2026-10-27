import { invoke } from './client';
import type { ColorReference } from './colorMatch';

export const REGIONAL_MATCH_LABELS = {
  face_skin: 'Face skin', body_skin: 'Body skin', hair: 'Hair', clothes: 'Clothing', background: 'Background',
} as const;
export type MatchRegion = keyof typeof REGIONAL_MATCH_LABELS;
export type RegionalControls = { amount: number; tone: number; color: number; whiteBalance: number };
export type RegionalMatchReport = {
  sessionId: string; photoId: string; reference: ColorReference; recipeHash: string; current: boolean;
  regions: { name: MatchRegion; controls: RegionalControls; beforeDistance: number; afterDistance: number; correctionAvailable: boolean; reason: string | null }[];
  unavailable: { name: MatchRegion; reason: string }[]; fullResolutionChecked: boolean; changed: number;
};
export type RegionalStrengths = Partial<Record<MatchRegion, RegionalControls>>;
export const regionalColorMatch = {
  status: (photoId: string) => invoke<RegionalMatchReport | null>('regional_color_match_status', { photoId }),
  prepare: (photoId: string, referenceId: string, cancelId: string) =>
    invoke<RegionalMatchReport>('prepare_regional_color_match', { input: { photoId, referenceId, cancelId } }),
  apply: (photoId: string, sessionId: string, recipeHash: string, strengths: RegionalStrengths, cancelId: string) =>
    invoke<RegionalMatchReport>('apply_regional_color_match', { input: { photoId, sessionId, recipeHash, strengths, cancelId } }),
};
