import { beforeEach, expect, it, vi } from 'vitest';
import { api } from '../../ipc/client';
import { colorMatch, type ColorMatchReport } from '../../ipc/colorMatch';
import { matchColorCollection } from './matchColorCollection';
vi.mock('../../ipc/client', () => ({ api: { listImages: vi.fn() }, asIpcError: (e: Error) => ({ message: e.message }) }));
vi.mock('../../ipc/colorMatch', () => ({ colorMatch: { status: vi.fn(), prepare: vi.fn(), apply: vi.fn() } }));
const controls = { amount: 0.7, tone: 0.5, color: 0.8, whiteBalance: 0.3 };
const result = (id: string): ColorMatchReport => ({ photoId: id, sessionId: `session-${id}`, reference: { id: 'ref', name: 'Reference', width: 100, height: 100, convertedIcc: false, regions: [] },
  controls, matchedRegions: ['whole image'], beforeDistance: 8, afterDistance: 5, changed: 3, protectedFields: 0, fullResolutionChecked: true, current: true });
beforeEach(() => {
  vi.resetAllMocks();
  vi.mocked(api.listImages).mockResolvedValue([{ id: 'a', fileName: 'A.jpg' }, { id: 'b', fileName: 'B.jpg' }] as never);
  vi.mocked(colorMatch.status).mockResolvedValue(null);
  vi.mocked(colorMatch.prepare).mockImplementation(async photo => result(photo));
  vi.mocked(colorMatch.apply).mockImplementation(async photo => result(photo));
});
it('fits different targets separately and reuses the stable baseline for a previous match', async () => {
  vi.mocked(colorMatch.status).mockImplementation(async photo => photo === 'a' ? result(photo) : null);
  const results = await matchColorCollection('project', 'ref', controls, 'job', () => false, vi.fn(), vi.fn());
  expect(colorMatch.prepare).toHaveBeenCalledOnce();
  expect(colorMatch.prepare).toHaveBeenCalledWith('b', 'ref', 'job');
  expect(colorMatch.apply).toHaveBeenNthCalledWith(1, 'a', 'session-a', controls, 'job');
  expect(colorMatch.apply).toHaveBeenNthCalledWith(2, 'b', 'session-b', controls, 'job');
  expect(results.map(r => r.outcome)).toEqual(['matched', 'matched']);
});
it('continues past a failed photo and refuses incomplete verification', async () => {
  vi.mocked(colorMatch.apply).mockResolvedValueOnce({ ...result('a'), fullResolutionChecked: false });
  const results = await matchColorCollection('project', 'ref', controls, 'job', () => false, vi.fn(), vi.fn());
  expect(results.map(r => r.outcome)).toEqual(['failed', 'matched']);
});
it('stops between analysis and application without saving that photo', async () => {
  let stopped = false;
  vi.mocked(colorMatch.prepare).mockImplementation(async photo => { stopped = true; return result(photo); });
  const results = await matchColorCollection('project', 'ref', controls, 'job', () => stopped, vi.fn(), vi.fn());
  expect(results).toHaveLength(0);
  expect(colorMatch.apply).not.toHaveBeenCalled();
});
it('syncs only selected collection members and forwards the real fitting quality', async () => {
  const results = await matchColorCollection('project', 'ref', controls, 'job', () => false, vi.fn(), vi.fn(),
    { photoIds: ['b', 'b', 'another-project'], quality: 'quick' });
  expect(results.map(item => item.photoId)).toEqual(['b']);
  expect(colorMatch.prepare).toHaveBeenCalledOnce();
  expect(colorMatch.prepare).toHaveBeenCalledWith('b', 'ref', 'job', 'quick');
  expect(colorMatch.apply).toHaveBeenCalledOnce();
  expect(colorMatch.apply).toHaveBeenCalledWith('b', 'session-b', controls, 'job');
});
it('does not expand an empty selection to the whole collection', async () => {
  expect(await matchColorCollection('project', 'ref', controls, 'job', () => false, vi.fn(), vi.fn(), { photoIds: [] })).toEqual([]);
  expect(api.listImages).not.toHaveBeenCalled();
  expect(colorMatch.apply).not.toHaveBeenCalled();
});
it('refits when the selected quality differs from a previous match', async () => {
  vi.mocked(colorMatch.status).mockImplementation(async photo => result(photo));
  await matchColorCollection('project', 'ref', controls, 'job', () => false, vi.fn(), vi.fn(), { quality: 'quick' });
  expect(colorMatch.prepare).toHaveBeenCalledTimes(2);
  expect(colorMatch.prepare).toHaveBeenNthCalledWith(1, 'a', 'ref', 'job', 'quick');
});
it('defaults to precise instead of reusing a saved quick fit', async () => {
  vi.mocked(colorMatch.status).mockImplementation(async photo => ({ ...result(photo), fitting: { quality: 'quick' } } as ColorMatchReport));
  await matchColorCollection('project', 'ref', controls, 'job', () => false, vi.fn(), vi.fn());
  expect(colorMatch.prepare).toHaveBeenCalledTimes(2);
  expect(colorMatch.prepare).toHaveBeenNthCalledWith(1, 'a', 'ref', 'job');
});
it('rejects a native result for a different reference', async () => {
  vi.mocked(colorMatch.apply).mockResolvedValueOnce({ ...result('a'), reference: { ...result('a').reference, id: 'other' } });
  const results = await matchColorCollection('project', 'ref', controls, 'job', () => false, vi.fn(), vi.fn());
  expect(results.map(r => r.outcome)).toEqual(['failed', 'matched']);
});
