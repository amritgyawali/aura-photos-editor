import { invoke } from './client';

export type ColorMatchControls = { amount: number; tone: number; color: number; whiteBalance: number };
export const DEFAULT_MATCH_CONTROLS: ColorMatchControls = { amount: 1, tone: 1, color: 1, whiteBalance: 1 };
export type ColorReference = {
  id: string; name: string; width: number; height: number; convertedIcc: boolean; regions: string[];
};
export type ColorMatchReport = {
  sessionId: string; photoId: string; reference: ColorReference; controls: ColorMatchControls;
  matchedRegions: string[]; beforeDistance: number; afterDistance: number; changed: number;
  protectedFields: number; fullResolutionChecked: boolean; current: boolean;
};
export const colorMatch = {
  list: () => invoke<ColorReference[]>('list_color_references'),
  status: (photoId: string) => invoke<ColorMatchReport | null>('color_match_status', { photoId }),
  import: (path: string, cancelId: string) => invoke<ColorReference>('import_color_reference', { input: { path, cancelId } }),
  prepare: (photoId: string, referenceId: string, cancelId: string) =>
    invoke<ColorMatchReport>('prepare_color_match', { input: { photoId, referenceId, cancelId } }),
  apply: (photoId: string, sessionId: string, controls: ColorMatchControls, cancelId: string) =>
    invoke<ColorMatchReport>('apply_color_match', { input: { photoId, sessionId, controls, cancelId } }),
};
export async function pickColorReference(): Promise<string | null> {
  const result = await invoke<string | string[] | null>('plugin:dialog|open', { options: {
    directory: false, multiple: false, title: 'Choose an edited colour reference',
    filters: [{ name: 'Edited photo', extensions: ['jpg', 'jpeg', 'png', 'tif', 'tiff'] }],
  } });
  return Array.isArray(result) ? result[0] ?? null : result;
}
