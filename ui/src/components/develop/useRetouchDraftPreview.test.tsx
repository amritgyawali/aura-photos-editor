import { act, renderHook } from '@testing-library/react';
import { useMemo } from 'react';
import { afterEach, beforeEach, expect, it, vi } from 'vitest';
import { nativeRetouch, freshRetouch } from '../../ipc/nativeRetouch';
import { useRetouchDraftPreview } from './useRetouchDraftPreview';
import type { RenderDto } from '../../ipc/types';

vi.mock('../../ipc/nativeRetouch', async () => ({ ...await vi.importActual('../../ipc/nativeRetouch'), nativeRetouch: { draftPreview: vi.fn() } }));
beforeEach(() => { vi.useFakeTimers(); vi.resetAllMocks(); });
afterEach(() => vi.useRealTimers());
it('serializes native renders and discards stale results while keeping only the latest pending draft', async () => {
  let resolve!: (image: RenderDto) => void;
  vi.mocked(nativeRetouch.draftPreview).mockReturnValueOnce(new Promise(done => { resolve = done; }))
    .mockResolvedValue({ renderHash: 'latest' } as RenderDto);
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
  await act(async () => { resolve({ renderHash: 'stale' } as RenderDto); await vi.advanceTimersByTimeAsync(1); });
  expect(nativeRetouch.draftPreview).toHaveBeenCalledTimes(2);
  expect(vi.mocked(nativeRetouch.draftPreview).mock.calls[1]?.[2].amount).toBe(.8);
  expect(result.current.image?.renderHash).toBe('latest');
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
