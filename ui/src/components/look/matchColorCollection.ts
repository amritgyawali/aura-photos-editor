import { api, asIpcError } from '../../ipc/client';
import { colorMatch, type ColorMatchControls, type ColorMatchIntent } from '../../ipc/colorMatch';

export type ColorMatchOutcome = { photoId: string; name: string; outcome: 'matched' | 'unchanged' | 'failed' | 'stopped'; detail: string };

/** Refit appearance for each target; never copy another photo's coordinates or correction. */
export async function matchColorCollection(projectId: string, referenceId: string, controls: ColorMatchControls,
  cancelId: string, stopped: () => boolean, progress: (message: string) => void,
  completed: (result: ColorMatchOutcome) => void,
  options: { photoIds?: readonly string[]; quality?: 'quick' | 'precise'; intent?: ColorMatchIntent } = {}): Promise<ColorMatchOutcome[]> {
  const photos = [];
  const seen = new Set<string>();
  // Resolve membership from the collection, including photos beyond the loaded filmstrip.
  // An explicitly empty selection never expands to the entire collection.
  const selected = options.photoIds === undefined ? null : new Set(options.photoIds);
  if (selected?.size === 0) return [];
  for (let offset = 0; !stopped(); offset += 240) {
    const page = await api.listImages({ projectId, offset, limit: 240, orderBy: 'timeline' });
    for (const photo of page) if (!seen.has(photo.id)) {
      seen.add(photo.id);
      if (!selected || selected.has(photo.id)) photos.push(photo);
    }
    if (page.length < 240) break;
  }
  const results: ColorMatchOutcome[] = [];
  for (const [index, photo] of photos.entries()) {
    if (stopped()) break;
    progress(`Matching colour ${index + 1} of ${photos.length}: ${photo.fileName}`);
    let result: ColorMatchOutcome;
    try {
      const saved = await colorMatch.status(photo.id);
      if (stopped()) break;
      // Reuse the immutable baseline when changing strengths on a previous match.
      const sameQuality = (saved?.fitting?.quality ?? 'precise') === (options.quality ?? 'precise');
      const sameIntent = (saved?.fitting?.intent ?? 'natural') === (options.intent ?? 'natural');
      const currentEngine = options.intent !== 'palette' || saved?.fitting?.engine === 'reference-palette-transport-v8';
      const analysis = saved?.current && saved.reference.id === referenceId && sameQuality && sameIntent && currentEngine ? saved
        : options.intent ? await colorMatch.prepare(photo.id, referenceId, cancelId, options.quality, options.intent)
          : options.quality ? await colorMatch.prepare(photo.id, referenceId, cancelId, options.quality)
          : await colorMatch.prepare(photo.id, referenceId, cancelId);
      if (stopped()) break;
      const applied = await colorMatch.apply(photo.id, analysis.sessionId, controls, cancelId);
      if (!applied.current || !applied.fullResolutionChecked || applied.photoId !== photo.id || applied.reference.id !== referenceId) {
        throw new Error('This photo returned no current native-resolution colour check.');
      }
      result = { photoId: photo.id, name: photo.fileName, outcome: applied.changed > 0 ? 'matched' : 'unchanged',
        detail: `Appearance distance ${applied.beforeDistance.toFixed(2)} → ${applied.afterDistance.toFixed(2)}.` };
    } catch (cause) {
      result = { photoId: photo.id, name: photo.fileName, outcome: stopped() ? 'stopped' : 'failed', detail: asIpcError(cause).message };
    }
    results.push(result); completed(result);
  }
  progress(`${results.filter(r => r.outcome === 'matched').length} matched, ${results.filter(r => r.outcome === 'unchanged').length} unchanged, ${results.filter(r => r.outcome === 'failed').length} failed. ${Math.max(0, photos.length - results.length)} remaining${stopped() ? '; stopped' : ''}.`);
  return results;
}
