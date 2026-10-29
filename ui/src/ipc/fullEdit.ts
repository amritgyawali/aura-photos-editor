import { invoke } from './client';
import { listen } from '@tauri-apps/api/event';
import type { RecipeDto } from './types';
import type { StageOutcome } from './advancedRetouch';

/** The ten stages of the full automatic edit, in the order they are reported (ADR-0187). */
export const FULL_EDIT_STAGES = [
  ['basic', 'Basic edit', 'Edit'],
  ['colour_correction', 'Colour correction', 'Edit'],
  ['calibration', 'Calibration', 'Edit'],
  ['colour_grading', 'Colour grading', 'Edit'],
  ['crop_straighten', 'Crop & straighten', 'Edit'],
  ['blemishes', 'Blemishes', 'Retouch'],
  ['skin_smoothing', 'Skin smoothing', 'Retouch'],
  ['face', 'Face', 'Retouch'],
  ['body', 'Body', 'Retouch'],
  ['advanced', 'Advanced edit', 'Advanced'],
] as const;
export type FullStageKey = typeof FULL_EDIT_STAGES[number][0];

/** What the photographer chose as the look: none, a reference matched afterwards, or a profile/provider already applied. */
export type FullEditLook = 'none' | 'coming' | 'applied';

export type FullStage = {
  number: number; key: FullStageKey; title: string; group: string; outcome: StageOutcome;
  changes: string[]; checks: string[];
};
export type FullEditResult = {
  photoId: string; recipe: RecipeDto; stages: FullStage[]; faces: number;
  qualityPassed: boolean; summary: string; millis: number;
};
/** The photograph as it stood after one step, sent only while someone is watching. */
export type StagePicture = { step: string; image: string; width: number; height: number };
export type FullEditProgress = {
  photoId: string; number: number; total: number; title: string; state: 'running' | 'done' | 'picture';
  /** With `state: 'picture'`: the original (number 0), then the photograph after each step that changed it. */
  picture?: StagePicture;
};

/** How an outcome reads in a report. */
export const OUTCOME_WORDS: Record<StageOutcome, string> = {
  applied: 'Applied', unchanged: 'Nothing needed', not_applicable: 'Does not apply', protected: 'Kept your setting',
};

/** "6 applied · 3 nothing needed · 1 does not apply" for one photograph. */
export function stageTally(stages: readonly FullStage[]): string {
  const counts = new Map<StageOutcome, number>();
  for (const stage of stages) counts.set(stage.outcome, (counts.get(stage.outcome) ?? 0) + 1);
  return (['applied', 'unchanged', 'not_applicable', 'protected'] as const)
    .filter(outcome => counts.has(outcome))
    .map(outcome => `${counts.get(outcome)} ${OUTCOME_WORDS[outcome].toLowerCase()}`)
    .join(' · ');
}

export const fullEdit = {
  /** Runs the ten stages in order; each stage that changes something is its own history step.
   *  `preview` asks for a picture before the edit and after every step that changed the photo. */
  run: (projectId: string, photoId: string, look: FullEditLook = 'none', preview = false) =>
    invoke<FullEditResult>('full_edit_photo', { input: { projectId, photoId, look, preview } }),
  /** The photograph as its saved edit stands now, as a stage picture named `step`. */
  picture: (projectId: string, photoId: string, step: string) =>
    invoke<StagePicture>('full_edit_picture', { input: { projectId, photoId, step } }),
  /** Stage-by-stage progress of the photograph being edited. */
  onProgress: (handler: (event: FullEditProgress) => void): Promise<() => void> =>
    listen<FullEditProgress>('full-edit', message => handler(message.payload)).then(unlisten => () => { unlisten(); }),
};
