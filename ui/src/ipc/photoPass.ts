import { invoke } from '@tauri-apps/api/core';
import type { RecipeDto } from './types';
import type { StageOutcome } from './advancedRetouch';

/** The hundred-step photo pass that runs after Auto enhance on every imported photo (ADR-0186). */
export const PASS_PLANNED = 100;
export type PassCategory = 'capture';
export type PassStepReport = {
  number: number; id: string; title: string; category: PassCategory; outcome: StageOutcome;
  checks: string[]; changes: string[]; operations: number; saved: boolean;
};
export type PhotoPassReport = {
  version: string; planned: number; steps: PassStepReport[]; applied: number; historySteps: number; summary: string;
};
export type PhotoPassResult = { recipe: RecipeDto; report: PhotoPassReport };

export const photoPass = {
  /** Runs every step in order; each step that changes the photo is its own history step. */
  run: (projectId: string, photoId: string) =>
    invoke<PhotoPassResult>('photo_pass', { input: { projectId, photoId } }),
};

const isRecord = (value: unknown): value is Record<string, unknown> => Boolean(value && typeof value === 'object');
const strings = (value: unknown): string[] => Array.isArray(value) ? value.filter((v): v is string => typeof v === 'string') : [];
const OUTCOMES: readonly StageOutcome[] = ['applied', 'unchanged', 'not_applicable', 'protected'];
const count = (value: unknown): number => typeof value === 'number' && Number.isSafeInteger(value) && value >= 0 ? value : 0;
const REPORT_KEY = 'photo_pass_v1';

/** The last pass's report, from the recipe it was saved in; `null` when there is none. */
export function readPassReport(recipe: RecipeDto | null | undefined): PhotoPassReport | null {
  if (!recipe?.body) return null;
  try {
    const body: unknown = JSON.parse(recipe.body);
    const raw = isRecord(body) ? body[REPORT_KEY] : null;
    if (!isRecord(raw) || !Array.isArray(raw.steps)) return null;
    const steps = raw.steps.filter(isRecord).flatMap((s): PassStepReport[] => {
      const outcome = OUTCOMES.find(o => o === s.outcome);
      if (!outcome || typeof s.number !== 'number' || typeof s.id !== 'string' || typeof s.title !== 'string') return [];
      return [{ number: s.number, id: s.id, title: s.title, category: 'capture', outcome,
        checks: strings(s.checks), changes: strings(s.changes), operations: count(s.operations), saved: s.saved === true }];
    });
    return {
      version: typeof raw.version === 'string' ? raw.version : '', planned: count(raw.planned) || PASS_PLANNED, steps,
      applied: count(raw.applied), historySteps: count(raw.historySteps), summary: typeof raw.summary === 'string' ? raw.summary : '',
    };
  } catch {
    return null;
  }
}
