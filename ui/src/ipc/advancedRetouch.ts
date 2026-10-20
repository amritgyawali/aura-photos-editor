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
  analysisWidth?: number; analysisHeight?: number;
  measurementStatus?: 'measured' | 'incomplete' | 'not_measured';
  incompleteReasons?: string[];
  contours?: ContourQuality[];
};
const REGION_NAMES = ['eye_image_left', 'eye_image_right', 'nose', 'mouth', 'chin_neck'] as const;
type RegionName = typeof REGION_NAMES[number];
type EdgeCounts = { sampledPairs: number; newSeamPairs: number; sourceEdgePairs: number; weakenedEdgePairs: number; review: boolean };
type RegionQuality = EdgeCounts & { region: RegionName; measured: boolean };
type ContourQuality = EdgeCounts & { face: number; regions: RegionQuality[] };
export type AdvancedRetouchReport = {
  version: string; faces: number; stages: AdvancedStageReport[]; quality: AdvancedQuality;
  historySteps: number; summary: string;
  checkedRecipe?: string;
  checkedPhotoId?: string;
  revisionStatus?: 'current' | 'changed' | 'unbound';
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
const REPORT_KEY = 'studio_advanced_retouch_v1';
// Match the recipe's six-decimal canonical precision. Object order is irrelevant;
// array order (including the retouch stack) is part of the checked state.
function stableParameters(value: unknown): string {
  if (typeof value === 'number') return Number.isFinite(value) ? value.toFixed(6) : 'invalid';
  if (Array.isArray(value)) return `[${value.map(stableParameters).join(',')}]`;
  if (isRecord(value)) return `{${Object.keys(value).sort().map(key => `${JSON.stringify(key)}:${stableParameters(value[key])}`).join(',')}}`;
  return JSON.stringify(value) ?? 'invalid';
}
function parameters(body: Record<string, unknown>): Record<string, unknown> {
  return Object.fromEntries(Object.entries(body).filter(([key]) => key !== REPORT_KEY && key !== 'provenance'));
}
const counter = (value: unknown): value is number => typeof value === 'number' && Number.isSafeInteger(value) && value >= 0;
function edgeCounts(c: Record<string, unknown>): EdgeCounts | null {
  if (!counter(c.sampledPairs) || !counter(c.newSeamPairs) || !counter(c.sourceEdgePairs)
    || !counter(c.weakenedEdgePairs) || typeof c.review !== 'boolean'
    || c.newSeamPairs > c.sampledPairs || c.sourceEdgePairs > c.sampledPairs || c.weakenedEdgePairs > c.sourceEdgePairs) return null;
  return { sampledPairs: c.sampledPairs, newSeamPairs: c.newSeamPairs, sourceEdgePairs: c.sourceEdgePairs,
    weakenedEdgePairs: c.weakenedEdgePairs, review: c.review };
}

/** The last run's report, from the recipe it was saved in; `null` when there is none. */
export function readAdvancedReport(recipe: RecipeDto | null | undefined): AdvancedRetouchReport | null {
  if (!recipe?.body) return null;
  try {
    const body: unknown = JSON.parse(recipe.body);
    if (!isRecord(body)) return null;
    const raw = body[REPORT_KEY];
    if (!isRecord(raw) || !Array.isArray(raw.stages)) return null;
    const stages = raw.stages.filter(isRecord).flatMap((s): AdvancedStageReport[] => {
      const known = ADVANCED_STAGES.find(([id]) => id === s.stage);
      const outcome = OUTCOMES.find(o => o === s.outcome);
      if (!known || !outcome || typeof s.number !== 'number') return [];
      return [{ number: s.number, stage: known[0], title: typeof s.title === 'string' ? s.title : known[1], outcome,
        checks: strings(s.checks), changes: strings(s.changes), operations: number(s.operations) ?? 0, saved: s.saved === true }];
    });
    const q = isRecord(raw.quality) ? raw.quality : {};
    let snapshot: unknown;
    try { snapshot = typeof raw.checkedRecipe === 'string' ? JSON.parse(raw.checkedRecipe) : undefined; }
    catch { snapshot = undefined; }
    const bound = isRecord(snapshot) && typeof raw.checkedPhotoId === 'string';
    const revisionStatus = !bound ? 'unbound' : raw.checkedPhotoId === recipe.photoId
      && stableParameters(snapshot) === stableParameters(parameters(body)) ? 'current' : 'changed';
    const measured = q.measurementStatus === 'measured';
    const contours = Array.isArray(q.contours) ? q.contours.filter(isRecord).flatMap((c): ContourQuality[] => {
      const counts = edgeCounts(c);
      if (!counts || !counter(c.face)) return [];
      const regions = Array.isArray(c.regions) ? c.regions.filter(isRecord).flatMap((r): RegionQuality[] => {
        const region = REGION_NAMES.find(name => name === r.region);
        const edges = edgeCounts(r);
        return region && edges && typeof r.measured === 'boolean' ? [{ ...edges, region, measured: r.measured }] : [];
      }) : [];
      return [{ ...counts, face: c.face, regions }];
    }) : [];
    const complete = measured && revisionStatus === 'current' && (number(q.analysisWidth) ?? 0) > 0 && (number(q.analysisHeight) ?? 0) > 0
      && number(q.textureRetention) !== null && number(q.skinShift) !== null
      && number(q.clippedOriginal) !== null && number(q.clippedFinal) !== null
      && counter(raw.faces) && raw.faces > 0 && contours.length === raw.faces
      && contours.every((c, i) => c.face === i && c.sampledPairs >= 200 && c.regions.length === REGION_NAMES.length
        && REGION_NAMES.every(name => c.regions.filter(r => r.region === name && r.measured && r.sampledPairs >= 64).length === 1));
    const texture = number(q.textureRetention);
    const shift = number(q.skinShift);
    const clipBefore = number(q.clippedOriginal);
    const clipAfter = number(q.clippedFinal);
    // A stored boolean cannot overrule the measurements. These limits match native QC.
    const metricsPass = texture !== null && texture >= 0.6 && shift !== null && shift >= 0 && shift <= 0.012
      && clipBefore !== null && clipBefore >= 0 && clipBefore <= 1
      && clipAfter !== null && clipAfter >= 0 && clipAfter <= 1 && clipAfter <= clipBefore + 0.005;
    const passed = complete && metricsPass && q.passed === true && contours.every(c => !c.review && c.regions.every(r => !r.review));
    const revisionReason = revisionStatus === 'changed'
      ? 'The photo settings changed after this check. Run quality control again.'
      : revisionStatus === 'unbound' ? 'This older report is not tied to checked photo settings. Run quality control again.' : null;
    const claimedPass = typeof raw.summary === 'string' && /quality control passed|measured checks passed/i.test(raw.summary);
    return {
      version: typeof raw.version === 'string' ? raw.version : '',
      faces: number(raw.faces) ?? 0,
      revisionStatus,
      checkedRecipe: typeof raw.checkedRecipe === 'string' ? raw.checkedRecipe : undefined,
      checkedPhotoId: typeof raw.checkedPhotoId === 'string' ? raw.checkedPhotoId : undefined,
      stages,
      quality: {
        textureRetention: number(q.textureRetention), skinShift: number(q.skinShift),
        clippedOriginal: number(q.clippedOriginal), clippedFinal: number(q.clippedFinal),
        mirrorBalance: number(q.mirrorBalance), passed, corrected: q.corrected === true,
        analysisWidth: number(q.analysisWidth) ?? 0, analysisHeight: number(q.analysisHeight) ?? 0,
        measurementStatus: complete ? 'measured' : q.measurementStatus === 'incomplete' || measured ? 'incomplete' : 'not_measured',
        incompleteReasons: [...strings(q.incompleteReasons), ...(revisionReason ? [revisionReason] : [])], contours,
      },
      historySteps: number(raw.historySteps) ?? 0,
      summary: revisionStatus === 'changed' ? revisionReason!
        : claimedPass && !passed ? !complete
          ? 'This saved report lacks complete QC measurements for the current settings. Review the photograph before delivery.'
          : 'Quality control requires review. See the facial-region checks and measurements.'
        : typeof raw.summary === 'string' ? raw.summary : '',
    };
  } catch { /* A recipe from before this feature carries no report. */ }
  return null;
}
