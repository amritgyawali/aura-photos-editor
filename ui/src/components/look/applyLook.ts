import { colour, tone } from '../../ipc/client';

/** Selection changes the solver input. Both passes must run to update saved pixels. */
export async function renderSelectedLook(projectId: string, cancelId: string, stopped: () => boolean): Promise<string> {
  if (stopped()) return 'Stopped. Completed edits are saved.';
  const exposure = await tone.estimateTone({ projectId, cancelId });
  if (exposure.cancelled || stopped()) return 'Stopped. Completed tone edits are saved.';
  const grading = await colour.estimateColour({ projectId, cancelId });
  if (grading.cancelled || stopped()) return 'Stopped. Completed edits are saved.';
  const failed = exposure.failed + grading.failed;
  return `Saved ${exposure.recipesWritten} tone edits and ${grading.recipesWritten} color edits.${failed ? ` ${failed} operations failed; review Problems before export.` : ''}${exposure.recipesWritten + grading.recipesWritten === 0 ? ' No photos were ready. Run automatic editing first.' : ''}`;
}
