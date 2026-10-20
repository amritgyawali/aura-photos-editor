import { invoke } from '@tauri-apps/api/core';
import { listen } from '@tauri-apps/api/event';
import type { RecipeDto } from './types';
import type { AutoRetouchOptions } from './nativeRetouch';

/** The eighteen stages of Auto advanced retouch, in the order they run (ADR-0093). */
export const ADVANCED_STAGES = [
  ['foundation', 'RAW foundation', 'White balance, exposure, highlights, shadows, moderate contrast and noise - neutral, not a look.'],
  ['lens_perspective', 'Lens & perspective', 'Levels a horizon its own straight lines measure; never reshapes anybody.'],
  ['background', 'Background cleanup', 'Dust and specks on a plain backdrop, healed from clean backdrop beside them.'],
  ['hair', 'Hair cleanup', 'Stray strands outside the hair shape faded; hair detail and shine.'],
  ['skin_cleanup', 'Skin cleanup', 'Pimples, redness and flakes cleared; moles, freckles and pores kept.'],
  ['frequency_separation', 'Selective frequency separation', 'Uneven tone corrected, texture kept and restored; never a blur.'],
  ['micro_dodge_burn', 'Micro dodge & burn', 'Small light irregularities evened, found with a black-and-white visual aid.'],
  ['medium_dodge_burn', 'Medium dodge & burn', 'Larger transitions: cheeks, forehead, under the eyes, shine.'],
  ['global_dodge_burn', 'Global dodge & burn', 'Faint shaping along the light that was there.'],
  ['skin_colour', 'Skin colour', 'Redness and blotches evened toward the person’s own skin, brightness untouched.'],
  ['eyes_lips_teeth', 'Eyes, lips & teeth', 'Redness out of the eyes, teeth less yellow, lip texture kept.'],
  ['clothing', 'Clothing', 'Creases softened; lint, threads and small stains healed.'],
  ['jewellery', 'Jewellery & reflections', 'Burnt-out reflections tamed; sparkle kept.'],
  ['background_toning', 'Background toning', 'A background brighter than the person lowered gently; a bright sky balanced.'],
  ['colour_grade', 'Colour grade', 'Correction first, style second.'],
  ['grain', 'Grain', 'A fine grain to tie retouched and untouched areas together on a clean file.'],
  ['output_sharpening', 'Output sharpening', 'Sharpened last, with skin masked out.'],
  ['quality_control', 'Quality control & export', 'Measures texture kept, skin colour drift, clipping and both halves of each face.'],
] as const;
export type AdvancedStage = typeof ADVANCED_STAGES[number][0];
export type StageOutcome = 'applied' | 'unchanged' | 'not_applicable' | 'protected';

export type AdvancedStageReport = {
  number: number; stage: AdvancedStage; title: string; outcome: StageOutcome;
  checks: string[]; changes: string[]; operations: number; saved: boolean;
};
export type AdvancedQuality = {
  textureRetention: number | null; skinShift: number | null;
  clippedOriginal: number | null; clippedFinal: number | null;
  mirrorBalance: number | null; passed: boolean; corrected: boolean;
};
export type AdvancedRetouchReport = {
  version: string; faces: number; stages: AdvancedStageReport[]; quality: AdvancedQuality;
  historySteps: number; summary: string;
};
export type AdvancedRetouchProgress = {
  photoId: string; number: number; total: number; title: string;
  state: 'running' | 'done'; outcome: StageOutcome | null;
};
export type AdvancedRetouchResult = { recipe: RecipeDto; report: AdvancedRetouchReport };
/**
 * A named starting point the backend owns: `professional` is the workflow's own defaults,
 * `beauty_fashion` is Evoto's homepage "Beauty & Fashion" pass - skin, clothing creases, stray
 * hair and a plain grey backdrop lifted to a clean bright grey, in one run (ADR-0109).
 */
export type AdvancedPreset = 'professional' | 'beauty_fashion';

export const advancedRetouch = {
  /** Runs every stage in order; each stage that changes something is its own history step. */
  run: (projectId: string, photoId: string, options?: AutoRetouchOptions, preset?: AdvancedPreset) =>
    invoke<AdvancedRetouchResult>('auto_advanced_retouch', { input: { projectId, photoId, options: options ?? null, preset: preset ?? null } }),
  /** Stage-by-stage progress of a running pass. */
  onProgress: (handler: (event: AdvancedRetouchProgress) => void): Promise<() => void> =>
    listen<AdvancedRetouchProgress>('advanced-retouch', message => handler(message.payload)).then(unlisten => () => { unlisten(); }),
};

const isRecord = (value: unknown): value is Record<string, unknown> => Boolean(value && typeof value === 'object');
const strings = (value: unknown): string[] => Array.isArray(value) ? value.filter((v): v is string => typeof v === 'string') : [];
const OUTCOMES: readonly StageOutcome[] = ['applied', 'unchanged', 'not_applicable', 'protected'];
const number = (value: unknown): number | null => typeof value === 'number' && Number.isFinite(value) ? value : null;

/** The last run's report, from the recipe it was saved in; `null` when there is none. */
export function readAdvancedReport(recipe: RecipeDto | null | undefined): AdvancedRetouchReport | null {
  if (!recipe?.body) return null;
  try {
    const raw: unknown = JSON.parse(recipe.body).studio_advanced_retouch_v1;
    if (!isRecord(raw) || !Array.isArray(raw.stages)) return null;
    const stages = raw.stages.filter(isRecord).flatMap((s): AdvancedStageReport[] => {
      const known = ADVANCED_STAGES.find(([id]) => id === s.stage);
      const outcome = OUTCOMES.find(o => o === s.outcome);
      if (!known || !outcome || typeof s.number !== 'number') return [];
      return [{ number: s.number, stage: known[0], title: typeof s.title === 'string' ? s.title : known[1], outcome,
        checks: strings(s.checks), changes: strings(s.changes), operations: number(s.operations) ?? 0, saved: s.saved === true }];
    });
    const q = isRecord(raw.quality) ? raw.quality : {};
    return {
      version: typeof raw.version === 'string' ? raw.version : '',
      faces: number(raw.faces) ?? 0,
      stages,
      quality: {
        textureRetention: number(q.textureRetention), skinShift: number(q.skinShift),
        clippedOriginal: number(q.clippedOriginal), clippedFinal: number(q.clippedFinal),
        mirrorBalance: number(q.mirrorBalance), passed: q.passed === true, corrected: q.corrected === true,
      },
      historySteps: number(raw.historySteps) ?? 0,
      summary: typeof raw.summary === 'string' ? raw.summary : '',
    };
  } catch { /* A recipe from before this feature carries no report. */ }
  return null;
}
