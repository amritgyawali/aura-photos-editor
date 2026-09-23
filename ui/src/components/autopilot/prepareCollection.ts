import { api, asIpcError, develop } from '../../ipc/client';
import { referenceStyle, type ReferenceSelection } from '../look/referenceStyle';

export type PreparedPhoto = { photoId: string; name: string; outcome: 'ready' | 'failed'; detail: string };

/** A real pixel-based baseline before optional model stages. Every outcome is reviewable. */
export async function prepareCollection(projectId: string, stopped: () => boolean,
  onProgress: (message: string) => void, onPhoto: (photo: PreparedPhoto) => void, reference?: ReferenceSelection | null): Promise<PreparedPhoto[]> {
  const photos = [];
  for (let offset = 0; !stopped(); offset += 240) {
    const page = await api.listImages({ projectId, offset, limit: 240, orderBy: 'timeline' });
    photos.push(...page);
    if (page.length < 240) break;
  }
  const outcomes: PreparedPhoto[] = [];
  for (const [index, photo] of photos.entries()) {
    if (stopped()) break;
    onProgress(`Editing photo ${index + 1} of ${photos.length}: ${photo.fileName}`);
    let result: PreparedPhoto;
    try {
      await develop.enhancePhoto({ photoId: photo.id });
      if (stopped()) break;
      if (reference) {
        onProgress(`Matching reference style ${index + 1} of ${photos.length}: ${photo.fileName}`);
        await referenceStyle.apply(photo.id, reference.analysis.id, reference.strength);
        if (stopped()) break;
      }
      await develop.renderImage({ photoId: photo.id, level: 'screen', screen: [512, 512], purpose: 'interactive' });
      result = { photoId: photo.id, name: photo.fileName, outcome: 'ready', detail: reference ? `Matched to ${reference.analysis.origin} at ${Math.round(reference.strength * 100)}% strength. Tone, white balance and color fit checked in rendered previews. Manual settings protected.` : 'Local exposure, contrast, highlights and shadows saved. Preview rendered. Manual settings protected.' };
    } catch (error) {
      result = { photoId: photo.id, name: photo.fileName, outcome: 'failed', detail: asIpcError(error).message };
    }
    outcomes.push(result);
    onPhoto(result);
  }
  return outcomes;
}
