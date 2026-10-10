import { act, renderHook } from '@testing-library/react';
import { useMemo } from 'react';
import { afterEach, beforeEach, expect, it, vi } from 'vitest';
import { nativeRetouch, freshRetouch } from '../../ipc/nativeRetouch';
import { useRetouchDraftPreview } from './useRetouchDraftPreview';
import type { RenderDto } from '../../ipc/types';

vi.mock('../../ipc/nativeRetouch', async () => ({ ...await vi.importActual('../../ipc/nativeRetouch'), nativeRetouch: { draftPreview: vi.fn(), selectionPreview: vi.fn() } }));
beforeEach(() => { vi.useFakeTimers(); vi.resetAllMocks(); });
afterEach(() => vi.useRealTimers());
it('serializes native renders and discards stale results while keeping only the latest pending draft', async () => {
  let resolve!: (image: RenderDto) => void;
  vi.mocked(nativeRetouch.draftPreview).mockReturnValueOnce(new Promise(done => { resolve = done; }))
    .mockResolvedValue({ width: 200 } as RenderDto);
  const { result, rerender } = renderHook(({ amount }) => {
    const draft = useMemo(() => ({ ...freshRetouch(), amount }), [amount]);
    return useRetouchDraftPreview('project', 'photo', draft, null, true, 0);
  }, { initialProps: { amount: .2 } });
  await act(async () => { await vi.advanceTimersByTimeAsync(400); });
  rerender({ amount: .4 });
  await act(async () => { await vi.advanceTimersByTimeAsync(400); });
  rerender({ amount: .8 });
  await act(async () => { await vi.advanceTimersByTimeAsync(400); });
  expect(nativeRetouch.draftPreview).toHaveBeenCalledTimes(1);
  await act(async () => { resolve({ width: 100 } as RenderDto); await vi.advanceTimersByTimeAsync(1); });
  expect(nativeRetouch.draftPreview).toHaveBeenCalledTimes(2);
  expect(vi.mocked(nativeRetouch.draftPreview).mock.calls[1]?.[2].amount).toBe(.8);
  expect(result.current.image?.width).toBe(200);
});
it('never displays a stale photo as a selection mask when the preview mode changes', async () => {
  let resolve!: (image: RenderDto) => void;
  vi.mocked(nativeRetouch.draftPreview).mockReturnValue(new Promise(done => { resolve = done; }));
  vi.mocked(nativeRetouch.selectionPreview).mockResolvedValue({width: 30, height: 20, rgbBase64: 'mask'});
  const draft = freshRetouch();
  const {result, rerender} = renderHook(({selection}) => useRetouchDraftPreview('p','i',draft,null,true,0,selection),
    {initialProps:{selection:false}});
  await act(async () => { await vi.advanceTimersByTimeAsync(400); });
  rerender({selection:true});
  expect(result.current.image).toBeNull();
  await act(async () => { await vi.advanceTimersByTimeAsync(400); });
  expect(nativeRetouch.selectionPreview).not.toHaveBeenCalled();
  await act(async () => { resolve({width:100} as RenderDto); await vi.advanceTimersByTimeAsync(1); });
  expect(result.current.image?.rgbBase64).toBe('mask');
  rerender({selection:false});
  expect(result.current.image).toBeNull();
});
it('surfaces a render error and cancels work before the debounce expires', async () => {
  vi.mocked(nativeRetouch.draftPreview).mockRejectedValue({ code: 'AURA-RENDER-8001', message: 'Preview unavailable', runbookUrl: '/help', retryable: true });
  const draft = freshRetouch();
  const { result, rerender } = renderHook(({ enabled }) => useRetouchDraftPreview('p','i',draft,null,enabled,0), { initialProps: { enabled: true } });
  rerender({ enabled: false });
  await act(async () => { await vi.advanceTimersByTimeAsync(400); });
  expect(nativeRetouch.draftPreview).not.toHaveBeenCalled();
  rerender({ enabled: true });
  await act(async () => { await vi.advanceTimersByTimeAsync(400); });
  expect(result.current.error).toBe('Preview unavailable');
  expect(result.current.pending).toBe(false);
});
it('shows a resting draft at full quality after the quick look, and never a superseded one', async () => {
  vi.mocked(nativeRetouch.draftPreview).mockImplementation(async (_p, _i, _e, _r, quality) => ({ width: quality === 'full' ? 400 : 100 }) as RenderDto);
  const { result, rerender } = renderHook(({ amount }) => {
    const draft = useMemo(() => ({ ...freshRetouch(), amount }), [amount]);
    return useRetouchDraftPreview('project', 'photo', draft, null, true, 0);
  }, { initialProps: { amount: .2 } });
  await act(async () => { await vi.advanceTimersByTimeAsync(400); });
  expect(result.current.image?.width).toBe(100);
  await act(async () => { await vi.advanceTimersByTimeAsync(1300); });
  expect(vi.mocked(nativeRetouch.draftPreview).mock.calls.map(call => call[4])).toEqual(['fast', 'full']);
  expect(result.current.image?.width).toBe(400);
  // A new stroke before the rest is over: no full-quality render of the old one.
  rerender({ amount: .5 });
  await act(async () => { await vi.advanceTimersByTimeAsync(400); });
  rerender({ amount: .6 });
  await act(async () => { await vi.advanceTimersByTimeAsync(400 + 1300); });
  const qualities = vi.mocked(nativeRetouch.draftPreview).mock.calls.slice(2).map(call => [call[2].amount, call[4]]);
  expect(qualities).toEqual([[.5, 'fast'], [.6, 'fast'], [.6, 'full']]);
});
