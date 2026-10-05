import { beforeEach, expect, it, vi } from 'vitest';
import { prepareCollection } from './prepareCollection';
import { api, develop, editProfiles } from '../../ipc/client';
import { referenceStyle } from '../look/referenceStyle';
import { nativeRetouch } from '../../ipc/nativeRetouch';
vi.mock('../../ipc/nativeRetouch', () => ({ nativeRetouch: { autoPortrait: vi.fn().mockResolvedValue({}) } }));
vi.mock('../look/referenceStyle', () => ({ referenceStyle: { apply: vi.fn().mockResolvedValue({}) } }));
vi.mock('../../ipc/client', () => ({
  api: { listImages: vi.fn() }, develop: { enhancePhoto: vi.fn(), renderImage: vi.fn(), imageRecipe: vi.fn() },
  editProfiles: { apply: vi.fn().mockResolvedValue({ profileId: 'film-portra', changed: 9, protectedFields: [], adaptations: ['Warmth reduced to 60% because the light is already warm.'] }) },
  asIpcError: (error: Error) => ({ message: error.message }),
}));
beforeEach(() => {
  vi.clearAllMocks();
  vi.mocked(api.listImages).mockResolvedValue([{ id: 'a', fileName: 'a.jpg' }, { id: 'b', fileName: 'b.jpg' }] as never);
  vi.mocked(develop.enhancePhoto).mockResolvedValue({} as never);
  vi.mocked(develop.renderImage).mockResolvedValue({} as never);
  vi.mocked(develop.imageRecipe).mockImplementation(async ({ photoId }) => ({ body: JSON.stringify({ global: { exposure: photoId === 'a' ? .75 : -.2, temperature: photoId === 'a' ? 4300 : 6200 } }) }) as never);
});
it('edits and renders each imported photograph without another user action', async () => {
  const log = vi.fn();
  const result = await prepareCollection('project', () => false, vi.fn(), log);
  expect(result.map(row => row.outcome)).toEqual(['ready', 'ready']);
  expect(develop.enhancePhoto).toHaveBeenCalledTimes(2);
  expect(develop.renderImage).toHaveBeenCalledTimes(2);
  expect(log).toHaveBeenCalledTimes(2);
  expect(result[0]?.settings).toContain('Exposure 0.75 EV');
  expect(result[1]?.settings).toContain('Exposure -0.20 EV');
  expect(result[0]?.settings).toContain('4300 K');
  expect(result[1]?.settings).toContain('6200 K');
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
  expect(develop.enhancePhoto).toHaveBeenCalledTimes(1);
  expect(develop.enhancePhoto).toHaveBeenCalledWith({ photoId: 'b' });
  vi.clearAllMocks();
  expect(await prepareCollection('project', () => false, vi.fn(), vi.fn(), null, null, undefined, [])).toEqual([]);
  expect(api.listImages).not.toHaveBeenCalled();
});
it('keeps a readable failed step and continues other photographs', async () => {
  vi.mocked(develop.enhancePhoto).mockRejectedValueOnce(new Error('Missing original'));
  const result = await prepareCollection('project', () => false, vi.fn(), vi.fn());
  expect(result[0]).toMatchObject({ outcome: 'failed', detail: 'Missing original' });
  expect(result[1]?.outcome).toBe('ready');
});
it('does not start another photograph after stop', async () => {
  let stopped = false;
  vi.mocked(develop.enhancePhoto).mockImplementationOnce(async () => { stopped = true; return {} as never; });
  const results = await prepareCollection('project', () => stopped, vi.fn(), vi.fn());
  expect(develop.enhancePhoto).toHaveBeenCalledOnce();
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
  vi.mocked(develop.imageRecipe).mockRejectedValueOnce(new Error('Recipe unavailable'));
  const pending = prepareCollection('project', () => false, vi.fn(), vi.fn());
  await vi.waitFor(() => expect(develop.renderImage).toHaveBeenCalledOnce());
  expect(develop.enhancePhoto).toHaveBeenCalledOnce();
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
  expect(results.every(photo => photo.outcome === 'ready' && photo.detail.includes('@reference'))).toBe(true);
});

it('applies the chosen edit profile instead of the plain enhancement and reports its adaptations', async () => {
  const profile = { profileId: 'film-portra', strength: 0.9 };
  const results = await prepareCollection('project', () => false, vi.fn(), vi.fn(), null, profile, 'Portra Film');
  expect(editProfiles.apply).toHaveBeenNthCalledWith(1, 'a', 'film-portra', 0.9);
  expect(develop.enhancePhoto).not.toHaveBeenCalled();
  expect(nativeRetouch.autoPortrait).toHaveBeenNthCalledWith(1, 'a');
  expect(nativeRetouch.autoPortrait).toHaveBeenNthCalledWith(2, 'b');
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
