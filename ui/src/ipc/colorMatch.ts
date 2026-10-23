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
