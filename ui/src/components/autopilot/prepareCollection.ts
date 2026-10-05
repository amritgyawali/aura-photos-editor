import { api, asIpcError, develop, editProfiles, type ProfileSelection } from '../../ipc/client';
import { referenceStyle, type ReferenceSelection } from '../look/referenceStyle';
import { nativeRetouch } from '../../ipc/nativeRetouch';
import { portraitMessage } from '../develop/PortraitAutoReport';
import type { RecipeDto } from '../../ipc/types';

export type PreparedPhoto = { photoId: string; name: string; outcome: 'ready' | 'failed'; detail: string; settings?: string };

/** Summarise the saved recipe, never a shared preset or an unsaved proposal. */
function savedSettings(recipe: RecipeDto): string | undefined {
  try {
    const body = JSON.parse(recipe.body);
    const g = body.global;
    if (!g) return undefined;
    const parts: string[] = [];
    for (const [key, label, unit] of [['exposure', 'Exposure', ' EV'], ['temperature', 'White balance', ' K'],
      ['highlights', 'Highlights', ''], ['shadows', 'Shadows', ''], ['vibrance', 'Vibrance', '']] as const) {
      const value: unknown = g[key];
      if (typeof value === 'number' && Number.isFinite(value)) parts.push(`${label} ${key === 'exposure' ? value.toFixed(2) : value}${unit}`);
    }
    return parts.join(' · ') || undefined;
  } catch { return undefined; }
}

/** A real pixel-based baseline before optional model stages. Every outcome is reviewable. */
export async function prepareCollection(projectId: string, stopped: () => boolean,
  onProgress: (message: string) => void, onPhoto: (photo: PreparedPhoto) => void, reference?: ReferenceSelection | null, profile?: ProfileSelection | null,
  profileName?: string, photoIds?: readonly string[]): Promise<PreparedPhoto[]> {
  const selected = photoIds ? new Set(photoIds) : null;
  if (selected?.size === 0) return [];
  const photos = [];
  const seen = new Set<string>();
  for (let offset = 0; !stopped(); offset += 240) {
    const page = await api.listImages({ projectId, offset, limit: 240, orderBy: 'timeline' });
    for (const photo of page) {
      if ((!selected || selected.has(photo.id)) && !seen.has(photo.id)) {
        photos.push(photo);
        seen.add(photo.id);
      }
    }
    if (page.length < 240) break;
  }
  const outcomes: PreparedPhoto[] = [];
  const checkStop = () => {
    if (stopped()) throw new Error('Stopped before preview verification. Completed edits are saved; retry this photo to finish.');
  };
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
        checkStop();
        const portrait = portraitMessage(await nativeRetouch.autoPortrait(photo.id));
        if (portrait) adaptations.push(portrait);
      } else {
        const portrait = portraitMessage(await develop.enhancePhoto({ photoId: photo.id }));
        if (portrait) adaptations.push(portrait);
      }
      checkStop();
      if (reference) {
        onProgress(`Matching reference style ${index + 1} of ${photos.length}: ${photo.fileName}`);
        if (profile) await referenceStyle.apply(photo.id, reference.analysis.id, reference.strength, profile);
        else await referenceStyle.apply(photo.id, reference.analysis.id, reference.strength);
        checkStop();
      }
      // Drain both reads before moving on, even if one fails, to bound native render work.
      const [saved, preview] = await Promise.allSettled([
        develop.imageRecipe({ photoId: photo.id }),
        develop.renderImage({ photoId: photo.id, level: 'screen', screen: [512, 512], purpose: 'interactive' }),
      ]);
      if (saved.status === 'rejected') throw saved.reason;
      if (preview.status === 'rejected') throw preview.reason;
      const look = profile ? `${profileName ?? profile.profileId} profile at ${Math.round(profile.strength * 100)}%` : 'Local exposure, contrast, highlights and shadows';
      const detail = reference
        ? `${profile ? `${look}, then m` : 'M'}atched to ${reference.analysis.origin} at ${Math.round(reference.strength * 100)}% strength. Tone, white balance and color fit checked in rendered previews. Manual settings protected. ${adaptations.join(' ')}`
        : `${look} saved. Preview rendered. Manual settings protected.${adaptations.length ? ` ${adaptations.join(' ')}` : ''}`;
      result = { photoId: photo.id, name: photo.fileName, outcome: 'ready', detail, settings: savedSettings(saved.value) };
    } catch (error) {
      result = { photoId: photo.id, name: photo.fileName, outcome: 'failed', detail: asIpcError(error).message };
    }
    outcomes.push(result);
    onPhoto(result);
  }
  return outcomes;
}
