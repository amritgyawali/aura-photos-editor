import { invoke } from './client';
import type { FineMatchControls } from './colorMatchDetails';

export type ColorMatchControls = { amount: number; tone: number; color: number; whiteBalance: number; detail?: FineMatchControls };
export const DEFAULT_MATCH_CONTROLS: ColorMatchControls = { amount: 1, tone: 1, color: 1, whiteBalance: 1 };
export type ColorMatchQuality = 'quick' | 'precise';
/** `exact` gives the photo the reference's colours themselves, repainting a different scene (ADR-0182). */
export type ColorMatchIntent = 'natural' | 'palette' | 'exact';
export type ColorReference = {
  id: string; name: string; width: number; height: number; convertedIcc: boolean; regions: string[];
  group?: string; favorite?: boolean; defaultControls?: ColorMatchControls; thumbnail?: string; preview?: string;
};
export type ReferenceMetadata = { name?: string; group?: string; favorite?: boolean; defaultControls?: ColorMatchControls };
export type ColorMatchDistance = { tone: number; cast: number; chroma: number; hue: number; palette: number; total: number };
export type LightroomPreset = {
  xml: string; fileName: string; name: string; recipeHash: string; tableFingerprint: string; tableEdge: number;
  meanTableError: number; p95TableError: number; maxTableError: number; warnings: string[];
};
/** What the scene-prior tone estimate concluded (ADR-0180): where the look puts black and
 *  white, and where this photo's own ends sat relative to an ordinary finished photo. */
export type ColorMatchTone = { lookBlack: number; lookWhite: number; ownBlack: number; ownWhite: number };
export type ColorMatchReport = {
  sessionId: string; photoId: string; reference: ColorReference; controls: ColorMatchControls;
  matchedRegions: string[]; beforeDistance: number; afterDistance: number; changed: number;
  protectedFields: number; fullResolutionChecked: boolean; current: boolean;
  fitting?: { engine: string; refinementCandidates: number; uniqueRenders: number; cachedProposals: number;
    quality?: ColorMatchQuality; intent?: ColorMatchIntent; targetSamples?: number; maxIterations?: number;
    gradeCandidates?: number; nativeCandidateChecked?: boolean; nativeBacktrackFactor?: number;
    structureSimilarity?: number | null; regularizationWeight?: number;
    transfer?: { renderedBefore: ColorMatchDistance; renderedAfter: ColorMatchDistance; toneEstimate?: ColorMatchTone;
      /** The reference's own original in the collection that the look was learned from (ADR-0161). */
      learnedFrom?: string;
      /** Why a photo that looked like the reference's original could not be learned from. */
      originalRefused?: string;
      /** Share of the photo the look would newly have clipped before its highlights were held back. */
      highlightsLimitedFrom?: number } };
  referenceMonochrome?: boolean;
  /** After a native apply: mean OKLab x100 difference between the photo as it was and the saved
   *  result. Below about 0.45 the change is hard to see. */
  visibleChange?: number;
  /** The amount asked for, when it failed the native checks and a smaller share of it was saved. */
  reducedFrom?: number;
};
export const colorMatch = {
  generatePreset: (photoId: string, sessionId: string, name: string, cancelId: string) =>
    invoke<LightroomPreset>('generate_color_match_preset', { input: { photoId, sessionId, name, cancelId } }),
  list: () => invoke<ColorReference[]>('list_color_references'),
  status: (photoId: string) => invoke<ColorMatchReport | null>('color_match_status', { photoId }),
  import: (path: string, cancelId: string) => invoke<ColorReference>('import_color_reference', { input: { path, cancelId } }),
  manage: (referenceId: string, metadata: ReferenceMetadata, action: 'update' | 'remove' = 'update') =>
    invoke<ColorReference | null>('manage_color_reference', { input: { referenceId, action, ...metadata } }),
  capture: (projectId: string, photoId: string, name: string, cancelId: string) =>
    invoke<ColorReference>('capture_color_reference', { input: { projectId, photoId, name, cancelId } }),
  prepare: (photoId: string, referenceId: string, cancelId: string, quality?: ColorMatchQuality, intent?: ColorMatchIntent) =>
    invoke<ColorMatchReport>('prepare_color_match', { input: { photoId, referenceId, cancelId, replacePrevious: true, ...(quality ? { quality } : {}), ...(intent ? { intent } : {}) } }),
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
