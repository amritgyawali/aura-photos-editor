import { beforeEach, expect, it, vi } from 'vitest';
import { prepareCollection } from './prepareCollection';
import { api, develop, editProfiles } from '../../ipc/client';
import { referenceStyle } from '../look/referenceStyle';
vi.mock('../look/referenceStyle', () => ({ referenceStyle: { apply: vi.fn().mockResolvedValue({}) } }));
vi.mock('../../ipc/client', () => ({
  api: { listImages: vi.fn() }, develop: { enhancePhoto: vi.fn(), renderImage: vi.fn() },
  editProfiles: { apply: vi.fn().mockResolvedValue({ profileId: 'film-portra', changed: 9, protectedFields: [], adaptations: ['Warmth reduced to 60% because the light is already warm.'] }) },
  asIpcError: (error: Error) => ({ message: error.message }),
}));
beforeEach(() => {
  vi.clearAllMocks();
  vi.mocked(api.listImages).mockResolvedValue([{ id: 'a', fileName: 'a.jpg' }, { id: 'b', fileName: 'b.jpg' }] as never);
  vi.mocked(develop.enhancePhoto).mockResolvedValue({} as never);
  vi.mocked(develop.renderImage).mockResolvedValue({} as never);
});
it('edits and renders each imported photograph without another user action', async () => {
  const log = vi.fn();
  const result = await prepareCollection('project', () => false, vi.fn(), log);
  expect(result.map(row => row.outcome)).toEqual(['ready', 'ready']);
  expect(develop.enhancePhoto).toHaveBeenCalledTimes(2);
  expect(develop.renderImage).toHaveBeenCalledTimes(2);
  expect(log).toHaveBeenCalledTimes(2);
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
  await prepareCollection('project', () => stopped, vi.fn(), vi.fn());
  expect(develop.enhancePhoto).toHaveBeenCalledOnce();
  expect(develop.renderImage).not.toHaveBeenCalled();
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
