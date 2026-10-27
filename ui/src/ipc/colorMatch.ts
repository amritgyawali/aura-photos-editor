import { invoke } from './client';
import type { FineMatchControls } from './colorMatchDetails';

export type ColorMatchControls = { amount: number; tone: number; color: number; whiteBalance: number; detail?: FineMatchControls };
export const DEFAULT_MATCH_CONTROLS: ColorMatchControls = { amount: 1, tone: 1, color: 1, whiteBalance: 1 };
export type ColorReference = {
  id: string; name: string; width: number; height: number; convertedIcc: boolean; regions: string[];
  group?: string; favorite?: boolean; defaultControls?: ColorMatchControls; thumbnail?: string;
};
export type ReferenceMetadata = { name?: string; group?: string; favorite?: boolean; defaultControls?: ColorMatchControls };
export type ColorMatchReport = {
  sessionId: string; photoId: string; reference: ColorReference; controls: ColorMatchControls;
  matchedRegions: string[]; beforeDistance: number; afterDistance: number; changed: number;
  protectedFields: number; fullResolutionChecked: boolean; current: boolean;
  fitting?: { engine: string; refinementCandidates: number; uniqueRenders: number; cachedProposals: number;
    gradeCandidates?: number; nativeCandidateChecked?: boolean; nativeBacktrackFactor?: number;
    structureSimilarity?: number | null; regularizationWeight?: number };
  referenceMonochrome?: boolean;
};
export const colorMatch = {
  list: () => invoke<ColorReference[]>('list_color_references'),
  status: (photoId: string) => invoke<ColorMatchReport | null>('color_match_status', { photoId }),
  import: (path: string, cancelId: string) => invoke<ColorReference>('import_color_reference', { input: { path, cancelId } }),
  manage: (referenceId: string, metadata: ReferenceMetadata, action: 'update' | 'remove' = 'update') =>
    invoke<ColorReference | null>('manage_color_reference', { input: { referenceId, action, ...metadata } }),
  capture: (projectId: string, photoId: string, name: string, cancelId: string) =>
    invoke<ColorReference>('capture_color_reference', { input: { projectId, photoId, name, cancelId } }),
  prepare: (photoId: string, referenceId: string, cancelId: string) =>
    invoke<ColorMatchReport>('prepare_color_match', { input: { photoId, referenceId, cancelId } }),
  apply: (photoId: string, sessionId: string, controls: ColorMatchControls, cancelId: string) =>
    invoke<ColorMatchReport>('apply_color_match', { input: { photoId, sessionId, controls, cancelId } }),
};
/** One-click matching (ADR-0170). The level is the overall amount; the backend steps it down
 * when a full-resolution highlight, skin or distance check refuses it. */
export type PreferenceLevel = 'light' | 'natural' | 'standard' | 'strong' | 'extreme' | 'custom';
export const PREFERENCE_LEVELS: readonly (readonly [Exclude<PreferenceLevel, 'custom'>, string, number])[] = [
  ['light', 'Light', 0.5], ['natural', 'Natural', 0.75], ['standard', 'Standard', 1],
  ['strong', 'Strong', 1.25], ['extreme', 'Extreme', 1.6],
];
export function preferenceControls(level: Exclude<PreferenceLevel, 'custom'>): ColorMatchControls {
  return { ...DEFAULT_MATCH_CONTROLS, amount: PREFERENCE_LEVELS.find(([id]) => id === level)?.[2] ?? 1 };
}
/** The named level these strengths are, or `custom` when anything but the amount was changed. */
export function levelOf(controls: ColorMatchControls): PreferenceLevel {
  const plain = controls.tone === 1 && controls.color === 1 && controls.whiteBalance === 1
    && Object.values(controls.detail ?? {}).every(value => value === undefined || value === 1);
  const level = PREFERENCE_LEVELS.find(([, , amount]) => Math.abs(amount - controls.amount) < 0.005);
  return plain && level ? level[0] : 'custom';
}
export type QuickMatchReport = ColorMatchReport & {
  requestedAmount: number; limited: boolean; createdBaseline: boolean; attempts: number;
  /** The protection that held the match below the request, when one did. */
  limitedBy?: 'highlights' | 'skin' | 'distance';
  /** The look fit could not get closer to this reference, so the cautious estimate was used. */
  cautious?: boolean;
};
export type ColorRecommendation = { referenceId: string; score: number; reasons: string[] };
export type ColorMatchPreset = {
  id: string; name: string; referenceId: string; referenceName: string; level: PreferenceLevel;
  controls: ColorMatchControls; createdAt: string; available: boolean;
};
export type PresetRequest = { id?: string; name?: string; referenceId?: string; level?: PreferenceLevel; controls?: ColorMatchControls };
export const oneClickMatch = {
  quick: (photoId: string, referenceId: string, controls: ColorMatchControls, cancelId: string, reanalyse = false) =>
    invoke<QuickMatchReport>('quick_color_match', { input: { photoId, referenceId, controls, cancelId, stepDown: true, reanalyse } }),
  recommend: (photoId: string, cancelId: string) =>
    invoke<ColorRecommendation[]>('recommend_color_references', { input: { photoId, cancelId } }),
  preview: (referenceId: string) => invoke<string | null>('color_reference_preview', { referenceId }),
  presets: (action: 'list' | 'save' | 'rename' | 'remove', request: PresetRequest = {}) =>
    invoke<ColorMatchPreset[]>('color_match_presets', { input: { action, ...request } }),
};

export async function pickColorReference(title = 'Choose an edited colour reference'): Promise<string | null> {
  const result = await invoke<string | string[] | null>('plugin:dialog|open', { options: {
    directory: false, multiple: false, title,
    filters: [{ name: 'Edited photo', extensions: ['jpg', 'jpeg', 'png', 'tif', 'tiff'] }],
  } });
  return Array.isArray(result) ? result[0] ?? null : result;
}

export async function pickColorReferences(): Promise<string[]> {
  const result = await invoke<string | string[] | null>('plugin:dialog|open', { options: {
    directory: false, multiple: true, title: 'Choose edited colour references',
    filters: [{ name: 'Edited photos', extensions: ['jpg', 'jpeg', 'png', 'tif', 'tiff'] }],
  } });
  return Array.isArray(result) ? result : result ? [result] : [];
}
