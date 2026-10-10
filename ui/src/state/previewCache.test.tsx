import { act, renderHook, waitFor } from '@testing-library/react';
import { beforeEach, expect, it, vi } from 'vitest';
import type { RenderDto } from '../ipc/types';
import { cachedPreview, forgetPreviews, rememberPreview, useProgressivePreview, type PreviewQuality } from './previewCache';

vi.mock('../ipc/client', () => ({ asIpcError: (error: Error) => ({ message: error.message }) }));

const image = (width: number, value: number) => ({ width, height: 1, rgbBase64: btoa(String.fromCharCode(...Array(width * 3).fill(value))), notes: [] }) as unknown as RenderDto;
beforeEach(() => forgetPreviews());

it('shows the quick look first, then full quality, and keeps it for the next visit', async () => {
  let finishFull: (value: RenderDto) => void = () => undefined;
  const load = vi.fn((quality: PreviewQuality) => quality === 'fast' ? Promise.resolve(image(1, 10))
    : new Promise<RenderDto>(resolve => { finishFull = resolve; }));
  const first = renderHook(() => useProgressivePreview('p:edited:h1', 'p', load));
  await waitFor(() => expect(first.result.current.quality).toBe('fast'));
  expect(first.result.current.upgrading).toBe(true);
  await act(async () => finishFull(image(4, 20)));
  expect(first.result.current).toMatchObject({ quality: 'full', upgrading: false, current: true });
  expect(first.result.current.image?.width).toBe(4);
  first.unmount();
  // Coming back to the section: the full-quality picture is there at once, nothing is asked for.
  const again = vi.fn();
  const second = renderHook(() => useProgressivePreview('p:edited:h1', 'p', again));
  expect(second.result.current).toMatchObject({ quality: 'full', current: true, upgrading: false });
  expect(again).not.toHaveBeenCalled();
});

it('keeps the last picture of the same photograph while a new version loads, never another photograph', async () => {
  rememberPreview('a:edited:1', image(2, 30), 'full');
  const load = vi.fn(() => new Promise<RenderDto>(() => undefined));
  const view = renderHook(({ key, scope }) => useProgressivePreview(key, scope, load), { initialProps: { key: 'a:edited:1', scope: 'a' } });
  view.rerender({ key: 'a:edited:2', scope: 'a' });
  await waitFor(() => expect(view.result.current.current).toBe(false));
  expect(view.result.current.image?.width).toBe(2);
  view.rerender({ key: 'b:edited:1', scope: 'b' });
  await waitFor(() => expect(view.result.current.image).toBeNull());
});

it('never replaces full quality with a quick look and never keeps a broken payload', () => {
  rememberPreview('k', image(4, 1), 'full');
  rememberPreview('k', image(1, 2), 'fast');
  expect(cachedPreview('k')?.quality).toBe('full');
  rememberPreview('broken', { ...image(2, 1), rgbBase64: 'nope' }, 'full');
  expect(cachedPreview('broken')).toBeUndefined();
});

it('reports a failed render and asks again on retry', async () => {
  const load = vi.fn((_quality: PreviewQuality): Promise<RenderDto> => Promise.reject(new Error('Disk is full')));
  const view = renderHook(({ attempt }) => useProgressivePreview('x', 'x', load, attempt), { initialProps: { attempt: 0 } });
  await waitFor(() => expect(view.result.current.error).toBe('Disk is full'));
  load.mockImplementation(() => Promise.resolve(image(1, 5)));
  view.rerender({ attempt: 1 });
  await waitFor(() => expect(view.result.current.quality).toBe('full'));
  expect(view.result.current.error).toBeNull();
});

it('asks for full quality only after the quick look, and not at all for a version already left', async () => {
  const pending: ((value: RenderDto) => void)[] = [];
  const load = vi.fn((_quality: PreviewQuality) => new Promise<RenderDto>(resolve => { pending.push(resolve); }));
  const view = renderHook(({ key }) => useProgressivePreview(key, 'p', load), { initialProps: { key: 'p:v1' } });
  expect(load.mock.calls.map(call => call[0])).toEqual(['fast']);
  // The slider moves on before the quick look of v1 arrives.
  view.rerender({ key: 'p:v2' });
  await act(async () => pending[0]?.(image(1, 1)));
  expect(load.mock.calls.map(call => call[0])).toEqual(['fast', 'fast']);
  await act(async () => pending[1]?.(image(1, 2)));
  // Only v2's full-quality picture is asked for.
  expect(load.mock.calls.map(call => call[0])).toEqual(['fast', 'fast', 'full']);
});

it('shows the live estimate first, then the exact quick look, then full quality, and never keeps the estimate', async () => {
  const order: string[] = [];
  const load = vi.fn(async (quality: PreviewQuality) => { order.push(quality); return image(quality === 'full' ? 4 : 2, 9); });
  const live = vi.fn(async () => { order.push('live'); return image(1, 9); });
  const view = renderHook(() => useProgressivePreview('p:live', 'p', load, 0, live));
  await waitFor(() => expect(view.result.current.quality).toBe('full'));
  expect(order).toEqual(['live', 'fast', 'full']);
  expect(cachedPreview('p:live')?.quality).toBe('full');
  // No estimate available yet: the quick look is asked for anyway.
  forgetPreviews();
  const none = vi.fn(async () => null);
  const second = renderHook(() => useProgressivePreview('p:none', 'p', load, 0, none));
  await waitFor(() => expect(second.result.current.quality).toBe('full'));
  expect(none).toHaveBeenCalledTimes(1);
});
