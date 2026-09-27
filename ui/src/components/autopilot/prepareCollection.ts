import { api, asIpcError, develop, editProfiles, type ProfileSelection } from '../../ipc/client';
import { referenceStyle, type ReferenceSelection } from '../look/referenceStyle';

export type PreparedPhoto = { photoId: string; name: string; outcome: 'ready' | 'failed'; detail: string };

/** A real pixel-based baseline before optional model stages. Every outcome is reviewable. */
export async function prepareCollection(projectId: string, stopped: () => boolean,
  onProgress: (message: string) => void, onPhoto: (photo: PreparedPhoto) => void, reference?: ReferenceSelection | null, profile?: ProfileSelection | null,
  profileName?: string): Promise<PreparedPhoto[]> {
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
      // A profile already contains the measured correction, so it replaces the plain enhancement.
      const adaptations: string[] = [];
      if (profile) {
        const report = await editProfiles.apply(photo.id, profile.profileId, profile.strength);
        adaptations.push(...report.adaptations);
      } else await develop.enhancePhoto({ photoId: photo.id });
      if (stopped()) break;
      if (reference) {
        onProgress(`Matching reference style ${index + 1} of ${photos.length}: ${photo.fileName}`);
        if (profile) await referenceStyle.apply(photo.id, reference.analysis.id, reference.strength, profile);
        else await referenceStyle.apply(photo.id, reference.analysis.id, reference.strength);
        if (stopped()) break;
      }
      await develop.renderImage({ photoId: photo.id, level: 'screen', screen: [512, 512], purpose: 'interactive' });
      const look = profile ? `${profileName ?? profile.profileId} profile at ${Math.round(profile.strength * 100)}%` : 'Local exposure, contrast, highlights and shadows';
      const detail = reference
        ? `${profile ? `${look}, then m` : 'M'}atched to ${reference.analysis.origin} at ${Math.round(reference.strength * 100)}% strength. Tone, white balance and color fit checked in rendered previews. Manual settings protected.`
        : `${look} saved. Preview rendered. Manual settings protected.${adaptations.length ? ` ${adaptations.join(' ')}` : ''}`;
      result = { photoId: photo.id, name: photo.fileName, outcome: 'ready', detail };
    } catch (error) {
      result = { photoId: photo.id, name: photo.fileName, outcome: 'failed', detail: asIpcError(error).message };
    }
    outcomes.push(result);
    onPhoto(result);
  }
  return outcomes;
}
