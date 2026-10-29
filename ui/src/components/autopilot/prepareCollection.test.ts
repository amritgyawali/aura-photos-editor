import { beforeEach, expect, it, vi } from 'vitest';
import { prepareCollection } from './prepareCollection';
import { api, develop, editProfiles } from '../../ipc/client';
import { referenceStyle } from '../look/referenceStyle';
import { fullEdit, type FullEditProgress } from '../../ipc/fullEdit';
vi.mock('../../ipc/fullEdit', async (original) => ({
  ...(await original<typeof import('../../ipc/fullEdit')>()),
  fullEdit: { run: vi.fn(), onProgress: vi.fn(), picture: vi.fn() },
}));
import { photoPass } from '../../ipc/photoPass';
vi.mock('../../ipc/photoPass', () => ({ photoPass: { run: vi.fn() } }));
vi.mock('../look/referenceStyle', () => ({ referenceStyle: { apply: vi.fn().mockResolvedValue({}) } }));
vi.mock('../../ipc/client', () => ({
  api: { listImages: vi.fn() }, develop: { renderImage: vi.fn(), imageRecipe: vi.fn() },
  editProfiles: { apply: vi.fn().mockResolvedValue({ profileId: 'film-portra', changed: 9, protectedFields: [], adaptations: ['Warmth reduced to 60% because the light is already warm.'] }) },
  asIpcError: (error: unknown) => (typeof error === 'object' && error !== null && 'runbookUrl' in error ? error : { message: 'AURA hit an unexpected problem and stopped safely.' }),
}));
// A full edit as the backend reports it: ten stages, each with its outcome.
const stage = (number: number, key: string, outcome: string) => ({ number, key, title: key, group: 'Edit', outcome, changes: [], checks: [] });
const edited = { photoId: 'a', recipe: {}, faces: 1, qualityPassed: true, summary: 'ok', millis: 4200,
  stages: [stage(1, 'basic', 'applied'), stage(2, 'colour_correction', 'applied'), stage(3, 'calibration', 'unchanged'), stage(4, 'colour_grading', 'applied'),
    stage(5, 'crop_straighten', 'unchanged'), stage(6, 'blemishes', 'applied'), stage(7, 'skin_smoothing', 'applied'), stage(8, 'face', 'applied'),
    stage(9, 'body', 'not_applicable'), stage(10, 'advanced', 'applied')] };
// A failure as the backend sends it: a typed error, not a JavaScript exception.
const backendError = (message: string) => ({ code: 'AURA-DB-3001', message, runbookUrl: 'https://aura.app/e/AURA-DB-3001', retryable: false });
beforeEach(() => {
  vi.clearAllMocks();
  vi.mocked(api.listImages).mockResolvedValue([{ id: 'a', fileName: 'a.jpg' }, { id: 'b', fileName: 'b.jpg' }] as never);
  vi.mocked(fullEdit.run).mockResolvedValue(edited as never);
  vi.mocked(fullEdit.onProgress).mockResolvedValue(() => undefined);
  vi.mocked(develop.renderImage).mockResolvedValue({} as never);
  vi.mocked(photoPass.run).mockResolvedValue({ recipe: {}, report: { summary: '3 of 25 steps changed the photograph.' } } as never);
  vi.mocked(develop.imageRecipe).mockImplementation(async ({ photoId }) => ({ body: JSON.stringify({ global: { exposure: photoId === 'a' ? .75 : -.2, temperature: photoId === 'a' ? 4300 : 6200 } }) }) as never);
});
it('edits and renders each imported photograph without another user action', async () => {
  const log = vi.fn();
  const result = await prepareCollection('project', () => false, vi.fn(), log);
  expect(result.map(row => row.outcome)).toEqual(['ready', 'ready']);
  expect(fullEdit.run).toHaveBeenCalledTimes(2);
  expect(fullEdit.run).toHaveBeenNthCalledWith(1, 'project', 'a', 'none', false);
  expect(develop.renderImage).toHaveBeenCalledTimes(2);
  expect(result[0]?.stages).toHaveLength(10);
  expect(result[0]?.detail).toContain('Full automatic edit, 10 stages: 7 applied · 2 nothing needed · 1 does not apply.');
  expect(result[0]?.millis).toBe(4200);
  expect(log).toHaveBeenCalledTimes(2);
  expect(result[0]?.settings).toContain('Exposure 0.75 EV');
  expect(result[1]?.settings).toContain('Exposure -0.20 EV');
  expect(result[0]?.settings).toContain('4300 K');
  expect(result[1]?.settings).toContain('6200 K');
});

it('runs the photo pass after the ten stages on every photograph and reports what it did', async () => {
  const order: string[] = [];
  vi.mocked(fullEdit.run).mockImplementation(async (_project, photoId) => { order.push(`edit ${photoId}`); return edited as never; });
  vi.mocked(photoPass.run).mockImplementation(async (_project, photoId) => {
    order.push(`pass ${photoId}`);
    return { recipe: {}, report: { summary: '3 of 25 steps changed the photograph.' } } as never;
  });
  const result = await prepareCollection('project', () => false, vi.fn(), vi.fn());
  expect(order).toEqual(['edit a', 'pass a', 'edit b', 'pass b']);
  expect(photoPass.run).toHaveBeenCalledWith('project', 'a');
  expect(result[0]?.detail).toContain('Photo pass: 3 of 25 steps changed the photograph.');
});

it('a failed photo pass fails that photograph only', async () => {
  vi.mocked(photoPass.run).mockRejectedValueOnce(backendError('Preview missing'));
  const result = await prepareCollection('project', () => false, vi.fn(), vi.fn());
  expect(result[0]).toMatchObject({ outcome: 'failed', detail: 'Preview missing' });
  expect(result[1]?.outcome).toBe('ready');
});

it('reports the retouch amounts each photo was given, which differ between photos', async () => {
  const face = (smoothing: number, extra: Record<string, [number, number]> = {}) => ({ face: 1, expert: { adjusted: { smoothing: [.5, smoothing], ...extra } } });
  vi.mocked(develop.imageRecipe).mockImplementation(async ({ photoId }) => ({ body: JSON.stringify({ global: { exposure: 0 },
    studio_portrait_auto_v1: { message: 'ok', assessments: photoId === 'a'
      ? [face(.33)] : [face(.71, { maxSpots: [12, 80], unknownControl: [0, 1] }), { face: 2, expert: { adjusted: {} } }, { face: 3 }] } }) }) as never);
  const result = await prepareCollection('project', () => false, vi.fn(), vi.fn());
  expect(result[0]?.settings).toContain('Retouch tuned per face (face 1: smoothing 33%)');
  expect(result[1]?.settings).toContain('face 1: smoothing 71%, spots up to 80; face 2: chosen settings)');
  expect(result[1]?.settings).not.toContain('unknownControl');
});

it('edits only requested photos and treats an empty selection as no work', async () => {
  const results = await prepareCollection('project', () => false, vi.fn(), vi.fn(), null, null, undefined, ['b']);
  expect(results.map(photo => photo.photoId)).toEqual(['b']);
  expect(fullEdit.run).toHaveBeenCalledTimes(1);
  expect(fullEdit.run).toHaveBeenCalledWith('project', 'b', 'none', false);
  vi.clearAllMocks();
  expect(await prepareCollection('project', () => false, vi.fn(), vi.fn(), null, null, undefined, [])).toEqual([]);
  expect(api.listImages).not.toHaveBeenCalled();
});
it('keeps a readable failed step and continues other photographs', async () => {
  vi.mocked(fullEdit.run).mockRejectedValueOnce(backendError('Missing original'));
  const result = await prepareCollection('project', () => false, vi.fn(), vi.fn());
  expect(result[0]).toMatchObject({ outcome: 'failed', detail: 'Missing original' });
  expect(result[1]?.outcome).toBe('ready');
});
it('does not start another photograph after stop', async () => {
  let stopped = false;
  vi.mocked(fullEdit.run).mockImplementationOnce(async () => { stopped = true; return edited as never; });
  const results = await prepareCollection('project', () => stopped, vi.fn(), vi.fn());
  expect(fullEdit.run).toHaveBeenCalledOnce();
  expect(develop.renderImage).not.toHaveBeenCalled();
  expect(results).toEqual([expect.objectContaining({ photoId: 'a', outcome: 'failed', detail: expect.stringContaining('retry this photo') })]);
});

it('finds selected photos beyond the first page without editing repeated IDs twice', async () => {
  const first = Array.from({ length: 240 }, (_, i) => ({ id: String(i), fileName: `${i}.jpg` }));
  vi.mocked(api.listImages).mockResolvedValueOnce(first as never)
    .mockResolvedValueOnce([first[239], { id: 'last', fileName: 'last.jpg' }] as never);
  const results = await prepareCollection('project', () => false, vi.fn(), vi.fn(), null, null, undefined, ['239', 'last']);
  expect(results.map(row => row.photoId)).toEqual(['239', 'last']);
  expect(api.listImages).toHaveBeenLastCalledWith({ projectId: 'project', offset: 240, limit: 240, orderBy: 'timeline' });
});

it('waits for the current render to drain when reading its recipe fails', async () => {
  let finish!: (value: never) => void;
  vi.mocked(develop.renderImage).mockReturnValueOnce(new Promise(resolve => { finish = resolve; }));
  vi.mocked(develop.imageRecipe).mockRejectedValueOnce(backendError('Recipe unavailable'));
  const pending = prepareCollection('project', () => false, vi.fn(), vi.fn());
  await vi.waitFor(() => expect(develop.renderImage).toHaveBeenCalledOnce());
  expect(fullEdit.run).toHaveBeenCalledOnce();
  finish({} as never);
  const results = await pending;
  expect(results.map(row => row.outcome)).toEqual(['failed', 'ready']);
  expect(results[0]?.detail).toBe('Recipe unavailable');
});

it('applies the selected reference to each new photo before rendering it', async () => {
  const selection = { analysis: { id: 'style', origin: '@reference' }, strength: 0.8 };
  const results = await prepareCollection('project', () => false, vi.fn(), vi.fn(), selection as never);
  expect(referenceStyle.apply).toHaveBeenNthCalledWith(1, 'a', 'style', 0.8);
  expect(referenceStyle.apply).toHaveBeenNthCalledWith(2, 'b', 'style', 0.8);
  // The reference decides the colour, so the edit leaves the grade to it.
  expect(fullEdit.run).toHaveBeenNthCalledWith(1, 'project', 'a', 'coming', false);
  expect(results.every(photo => photo.outcome === 'ready' && photo.detail.includes('@reference'))).toBe(true);
});

it('says when the look was learned from the reference photo\'s own original', async () => {
  vi.mocked(referenceStyle.apply).mockResolvedValue({ changed: 3, learnedFrom: '989A2698.CR3' } as never);
  const selection = { analysis: { id: 'style', origin: 'Studio look' }, strength: 1 };
  const results = await prepareCollection('project', () => false, vi.fn(), vi.fn(), selection as never);
  expect(results.every(photo => photo.detail.includes('learned from your own edit of 989A2698.CR3'))).toBe(true);
  vi.mocked(referenceStyle.apply).mockResolvedValue({} as never);
  const estimated = await prepareCollection('project', () => false, vi.fn(), vi.fn(), selection as never);
  expect(estimated.some(photo => photo.detail.includes('learned from'))).toBe(false);
});

it('never reports a reference match that changed nothing as a match', async () => {
  vi.mocked(referenceStyle.apply).mockResolvedValue({ changed: 0, beforeDistance: 11.3, afterDistance: 11.3, protectedFields: 0 } as never);
  const selection = { analysis: { id: 'style', origin: '@reference' }, strength: 0.8 };
  const results = await prepareCollection('project', () => false, vi.fn(), vi.fn(), selection as never);
  expect(results.every(photo => photo.detail.startsWith('Not matched to @reference:'))).toBe(true);
  expect(results.some(photo => photo.detail.includes('Matched to'))).toBe(false);
  expect(results[0]?.detail).toContain('left as it was');
  vi.mocked(referenceStyle.apply).mockResolvedValue({ changed: 0, beforeDistance: 4, afterDistance: 4, protectedFields: 5 } as never);
  const kept = await prepareCollection('project', () => false, vi.fn(), vi.fn(), selection as never);
  expect(kept[0]?.detail).toContain('your own settings on this photo were kept');
});

it('passes the active cancellation job into each native reference fit', async () => {
  const selection = { analysis: { id: 'style', origin: '@reference' }, strength: 0.8 };
  await prepareCollection('project', () => false, vi.fn(), vi.fn(), selection as never,
    null, undefined, undefined, 'reference-job');
  expect(referenceStyle.apply).toHaveBeenNthCalledWith(1, 'a', 'style', 0.8, null, 'reference-job');
  expect(referenceStyle.apply).toHaveBeenNthCalledWith(2, 'b', 'style', 0.8, null, 'reference-job');
});

it('applies the chosen edit profile, then every other stage keeping its light and colour, and reports its adaptations', async () => {
  const profile = { profileId: 'film-portra', strength: 0.9 };
  const results = await prepareCollection('project', () => false, vi.fn(), vi.fn(), null, profile, 'Portra Film');
  expect(editProfiles.apply).toHaveBeenNthCalledWith(1, 'a', 'film-portra', 0.9);
  expect(fullEdit.run).toHaveBeenNthCalledWith(1, 'project', 'a', 'applied', false);
  expect(fullEdit.run).toHaveBeenNthCalledWith(2, 'project', 'b', 'applied', false);
  expect(vi.mocked(editProfiles.apply).mock.invocationCallOrder[0]).toBeLessThan(vi.mocked(fullEdit.run).mock.invocationCallOrder[0] ?? 0);
  expect(develop.renderImage).toHaveBeenCalledTimes(2);
  expect(results[0]?.detail).toContain('Portra Film profile at 90%');
  expect(results[0]?.detail).toContain('already warm');
});

it('fits a reference on top of the chosen profile', async () => {
  const selection = { analysis: { id: 'style', origin: '@reference' }, strength: 0.7 };
  const profile = { profileId: 'dark-moody', strength: 1 };
  await prepareCollection('project', () => false, vi.fn(), vi.fn(), selection as never, profile, 'Dark & Moody');
  expect(referenceStyle.apply).toHaveBeenNthCalledWith(1, 'a', 'style', 0.7, profile);
});

it('says which stage of which photo is running while it edits', async () => {
  let emit: ((event: never) => void) | undefined;
  const stop = vi.fn();
  vi.mocked(fullEdit.onProgress).mockImplementation(async handler => { emit = handler as never; return stop; });
  vi.mocked(fullEdit.run).mockImplementation(async (_project, photoId) => {
    emit?.({ photoId, number: 6, total: 10, title: 'Blemishes', state: 'running' } as never);
    return edited as never;
  });
  const progress = vi.fn();
  await prepareCollection('project', () => false, progress, vi.fn());
  expect(progress).toHaveBeenCalledWith('Editing photo 2 of 2: b.jpg · stage 6 of 10: Blemishes');
  expect(stop).toHaveBeenCalledOnce();
});

it('asks for pictures only while someone watches, and hands every picture to the viewer', async () => {
  let emit: ((event: never) => void) | undefined;
  vi.mocked(fullEdit.onProgress).mockImplementation(async handler => { emit = handler as never; return () => undefined; });
  vi.mocked(fullEdit.run).mockImplementation(async (_project, photoId) => {
    emit?.({ photoId, number: 0, total: 10, title: 'Original', state: 'picture', picture: { step: 'Original', image: 'data:image/jpeg;base64,AA', width: 3, height: 2 } } as never);
    return edited as never;
  });
  let watching = true;
  const seen: string[] = [];
  const live = {
    watching: () => watching,
    onStart: vi.fn((photo: { photoId: string }) => { seen.push(`start ${photo.photoId}`); watching = false; }),
    onEvent: vi.fn((event: { photoId: string; state: string }) => seen.push(`${event.state} ${event.photoId}`)),
    onFinish: vi.fn((photoId: string, outcome: string) => seen.push(`${outcome} ${photoId}`)),
  };
  await prepareCollection('project', () => false, vi.fn(), vi.fn(), null, null, undefined, undefined, undefined, live);
  // Watched for the first photo; the screen was left before the second, which renders no pictures.
  expect(fullEdit.run).toHaveBeenNthCalledWith(1, 'project', 'a', 'none', true);
  expect(fullEdit.run).toHaveBeenNthCalledWith(2, 'project', 'b', 'none', false);
  expect(seen.slice(0, 3)).toEqual(['start a', 'picture a', 'ready a']);
  expect(live.onStart).toHaveBeenCalledOnce();
  expect(live.onFinish).toHaveBeenCalledOnce();
});

it('shows the matched reference look as the last picture of a watched edit', async () => {
  vi.mocked(referenceStyle.apply).mockResolvedValueOnce({ changed: 4 } as never);
  vi.mocked(fullEdit.picture).mockResolvedValueOnce({ step: 'Matched to @reference', image: 'data:image/jpeg;base64,AA', width: 3, height: 2 });
  const events: FullEditProgress[] = [];
  const live = { watching: () => true, onStart: vi.fn(), onEvent: (event: FullEditProgress) => { events.push(event); }, onFinish: vi.fn() };
  const selection = { analysis: { id: 'style', origin: '@reference' }, strength: 0.8 };
  await prepareCollection('project', () => false, vi.fn(), vi.fn(), selection as never, null, undefined, ['a'], undefined, live);
  expect(fullEdit.picture).toHaveBeenCalledWith('project', 'a', 'Matched to @reference');
  expect(events.at(-1)?.title).toBe('Reference look');
  expect(events.at(-1)?.picture?.step).toBe('Matched to @reference');
});
