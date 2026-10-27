import { api, asIpcError } from '../../ipc/client';
import { oneClickMatch, type ColorMatchControls } from '../../ipc/colorMatch';

export type SyncTarget = { id: string; fileName: string };
export type SyncOutcome = {
  photoId: string; name: string; outcome: 'matched' | 'limited' | 'unchanged' | 'skipped' | 'failed' | 'stopped'; detail: string;
};

/** The backend's refusal for a reference that is the photo being matched (ADR-0170). */
export const SAME_PHOTO = 'This reference is the same photo you are matching';
/** The backend's refusal when no amount of the look passes its checks on a photo. */
export const NOT_SUITED = "This reference's look does not suit this photo:";

/** Every photo in a collection, in timeline order. */
export async function collectionTargets(projectId: string, stopped: () => boolean): Promise<SyncTarget[]> {
  const out: SyncTarget[] = [];
  const seen = new Set<string>();
  for (let offset = 0; !stopped(); offset += 240) {
    const page = await api.listImages({ projectId, offset, limit: 240, orderBy: 'timeline' });
    for (const photo of page) if (!seen.has(photo.id)) { seen.add(photo.id); out.push({ id: photo.id, fileName: photo.fileName }); }
    if (page.length < 240) break;
  }
  return out;
}

/** Match each target to the same reference with the same strengths. Each photo is measured
 * and checked on its own; nothing is copied from one photo to another. */
export async function syncColorMatch(targets: readonly SyncTarget[], referenceId: string, controls: ColorMatchControls,
  cancelId: string, stopped: () => boolean, progress: (message: string) => void,
  completed: (result: SyncOutcome) => void): Promise<SyncOutcome[]> {
  const results: SyncOutcome[] = [];
  for (const [index, photo] of targets.entries()) {
    if (stopped()) break;
    progress(`Matching ${index + 1} of ${targets.length}: ${photo.fileName}`);
    let result: SyncOutcome;
    try {
      const report = await oneClickMatch.quick(photo.id, referenceId, controls, cancelId);
      if (!report.current || !report.fullResolutionChecked || report.photoId !== photo.id) {
        throw new Error('This photo returned no current full-resolution colour check.');
      }
      const outcome = report.changed === 0 ? 'unchanged' : report.limited ? 'limited' : 'matched';
      result = { photoId: photo.id, name: photo.fileName, outcome,
        detail: report.limited ? `Reduced to ${Math.round(report.controls.amount * 100)}% to protect highlights and skin.`
          : `Colour distance ${report.beforeDistance.toFixed(1)} → ${report.afterDistance.toFixed(1)}.` };
    } catch (cause) {
      const message = asIpcError(cause).message;
      result = message.startsWith(SAME_PHOTO)
        ? { photoId: photo.id, name: photo.fileName, outcome: 'skipped', detail: 'This is the reference photo itself, so it was left as it is.' }
        : message.startsWith(NOT_SUITED)
          ? { photoId: photo.id, name: photo.fileName, outcome: 'unchanged', detail: message }
          : { photoId: photo.id, name: photo.fileName, outcome: stopped() ? 'stopped' : 'failed', detail: message };
    }
    results.push(result); completed(result);
  }
  const count = (kind: SyncOutcome['outcome']) => results.filter(r => r.outcome === kind).length;
  progress(`Synced ${count('matched') + count('limited')} of ${targets.length}` +
    `${count('limited') ? ` (${count('limited')} reduced)` : ''}, ${count('unchanged')} unchanged, ` +
    `${count('skipped') ? `${count('skipped')} skipped (the reference itself), ` : ''}${count('failed')} failed` +
    `${stopped() ? '. Stopped; finished photos stay matched.' : '.'}`);
  return results;
}
