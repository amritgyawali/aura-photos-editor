import { api, develop, editProfiles, type ApplyReport, type ProfileSelection } from '../../ipc/client';
import { Refusal, reasonOf } from '../look/refusal';
import { referenceStyle, type ReferenceSelection } from '../look/referenceStyle';
import { fullEdit, stageTally, type FullEditProgress, type FullStage } from '../../ipc/fullEdit';
import type { RecipeDto } from '../../ipc/types';

export type PreparedPhoto = { photoId: string; name: string; outcome: 'ready' | 'failed'; detail: string; settings?: string; stages?: FullStage[]; millis?: number };

/** Someone watching the edit: each photograph as it starts, every stage event and picture, and
 *  when it is finished. `watching` is asked per photograph, so leaving the screen stops the
 *  pictures from the next photograph on. */
export type LiveWatch = {
  watching: () => boolean;
  onStart: (photo: { photoId: string; name: string; index: number; total: number }) => void;
  onEvent: (event: FullEditProgress) => void;
  onFinish: (photoId: string, outcome: 'ready' | 'failed') => void;
};

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
    const retouch = adaptedRetouch(body.studio_portrait_auto_v1);
    if (retouch) parts.push(retouch);
    return parts.join(' · ') || undefined;
  } catch { return undefined; }
}

/** What the reference stage did to this photo, said truthfully: a match that changed nothing is
 *  not reported as a match. */
function matchSentence(look: string | null, origin: string, strength: number, matched: ApplyReport | undefined): string {
  if (matched && matched.changed === 0 && strength > 0) {
    const kept = matched.protectedFields > 0
      ? 'your own settings on this photo were kept, so the look had nothing left to change'
      : "no version of the look kept this photo's smooth gradients, skin tones and highlights while bringing it closer to the reference";
    return `${look ? `${look} saved. ` : ''}Not matched to ${origin}: ${kept}. The photo was left as it was.`;
  }
  const how = matched?.learnedFrom
    ? `The edit was learned from your own edit of ${matched.learnedFrom} and applied as the same color and tone operation.`
    : 'Tone, white balance and color fit checked at native resolution.';
  return `${look ? `${look}, then m` : 'M'}atched to ${origin} at ${Math.round(strength * 100)}% strength. ${how}`;
}

const RETOUCH_LABELS: Record<string, string> = {
  smoothing: 'smoothing', texture: 'pore texture', toneEvenness: 'tone', lightEvenness: 'light', shine: 'shine',
  microDodgeBurn: 'dodge & burn', maxSpots: 'spots up to',
};

/** The retouch amounts this photo's faces were given, which differ from photo to photo. */
function adaptedRetouch(report: unknown): string | undefined {
  if (!report || typeof report !== 'object') return undefined;
  const rows: unknown = (report as { assessments?: unknown }).assessments;
  if (!Array.isArray(rows)) return undefined;
  const faces: string[] = [];
  for (const row of rows) {
    const expert = row && typeof row === 'object' ? (row as { expert?: unknown; face?: unknown }) : null;
    const adjusted = expert?.expert && typeof expert.expert === 'object' ? (expert.expert as { adjusted?: unknown }).adjusted : null;
    if (!adjusted || typeof adjusted !== 'object' || typeof expert?.face !== 'number') continue;
    const changes = Object.entries(RETOUCH_LABELS).flatMap(([key, label]) => {
      const pair: unknown = (adjusted as Record<string, unknown>)[key];
      if (!Array.isArray(pair) || typeof pair[1] !== 'number' || !Number.isFinite(pair[1])) return [];
      return [`${label} ${key === 'maxSpots' ? Math.round(pair[1]) : `${Math.round(pair[1] * 100)}%`}`];
    });
    faces.push(`face ${expert.face}: ${changes.length ? changes.join(', ') : 'chosen settings'}`);
    if (faces.length === 3) break;
  }
  return faces.length ? `Retouch tuned per face (${faces.join('; ')})` : undefined;
}

/** The full automatic edit for every photo, one at a time: basic edit, colour correction,
 *  calibration, grade, crop and straighten, blemishes, skin, face, body and the advanced
 *  finish (ADR-0187). A chosen look decides light and colour; every other stage still runs.
 *  Every outcome is reviewable. */
export async function prepareCollection(projectId: string, stopped: () => boolean,
  onProgress: (message: string) => void, onPhoto: (photo: PreparedPhoto) => void, reference?: ReferenceSelection | null, profile?: ProfileSelection | null,
  profileName?: string, photoIds?: readonly string[], cancelId?: string, live?: LiveWatch): Promise<PreparedPhoto[]> {
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
    if (stopped()) throw new Refusal('Stopped before preview verification. Completed edits are saved; retry this photo to finish.');
  };
  // Stage-by-stage progress of the photo being edited; a window without events edits the same.
  let current = '';
  const unlisten = await fullEdit.onProgress(event => {
    if (current && event.state === 'running') onProgress(`${current} · stage ${event.number} of ${event.total}: ${event.title}`);
    if (current) live?.onEvent(event);
  }).catch(() => () => undefined);
  try {
    for (const [index, photo] of photos.entries()) {
      if (stopped()) break;
      current = `Editing photo ${index + 1} of ${photos.length}: ${photo.fileName}`;
      onProgress(current);
      const watched = live?.watching() ?? false;
      if (watched) live?.onStart({ photoId: photo.id, name: photo.fileName, index, total: photos.length });
      let result: PreparedPhoto;
      let matched: ApplyReport | undefined;
      try {
        // A profile already contains the measured correction, so it replaces the measured light
        // and colour; the retouch, straightening, calibration and finish run after it.
        const adaptations: string[] = [];
        if (profile) {
          const report = await editProfiles.apply(photo.id, profile.profileId, profile.strength);
          adaptations.push(...report.adaptations);
          checkStop();
        }
        const edited = await fullEdit.run(projectId, photo.id, profile ? 'applied' : reference ? 'coming' : 'none', watched);
        const stages = edited.stages;
        adaptations.push(`Full automatic edit, 10 stages: ${stageTally(stages)}.`);
        checkStop();
        if (reference) {
          onProgress(`Matching reference style ${index + 1} of ${photos.length}: ${photo.fileName}`);
          matched = cancelId ? await referenceStyle.apply(photo.id, reference.analysis.id, reference.strength, profile ?? null, cancelId)
            : profile ? await referenceStyle.apply(photo.id, reference.analysis.id, reference.strength, profile)
            : await referenceStyle.apply(photo.id, reference.analysis.id, reference.strength);
          checkStop();
        }
        // Drain both reads before moving on, even if one fails, to bound native render work.
        const [saved, preview] = await Promise.allSettled([
          develop.imageRecipe({ photoId: photo.id }),
          develop.renderImage({ photoId: photo.id, level: 'screen', screen: [512, 512], purpose: 'interactive' }),
        ]);
        if (saved.status === 'rejected') throw saved.reason;
        if (preview.status === 'rejected') throw preview.reason;
        // The reference look is matched after the ten stages: its result is the last picture.
        // A picture that cannot be rendered is left out; the edit is saved either way.
        if (watched && reference && matched?.changed) {
          const picture = await fullEdit.picture(projectId, photo.id, `Matched to ${reference.analysis.origin}`).catch(() => null);
          if (picture) live?.onEvent({ photoId: photo.id, number: 11, total: 10, title: 'Reference look', state: 'picture', picture });
        }
        const look = profile ? `${profileName ?? profile.profileId} profile at ${Math.round(profile.strength * 100)}%` : 'Full automatic edit';
        const detail = reference
          ? `${matchSentence(profile ? look : null, reference.analysis.origin, reference.strength, matched)} Manual settings protected. ${adaptations.join(' ')}`
          : `${look} saved. Preview rendered. Manual settings protected.${adaptations.length ? ` ${adaptations.join(' ')}` : ''}`;
        result = { photoId: photo.id, name: photo.fileName, outcome: 'ready', detail, settings: savedSettings(saved.value), stages, millis: edited.millis };
      } catch (error) {
        result = { photoId: photo.id, name: photo.fileName, outcome: 'failed', detail: reasonOf(error) };
      }
      outcomes.push(result);
      if (watched) live?.onFinish(photo.id, result.outcome);
      onPhoto(result);
    }
  } finally { current = ''; unlisten(); }
  return outcomes;
}
