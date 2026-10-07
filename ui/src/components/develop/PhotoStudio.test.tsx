import { act, fireEvent, render, screen, waitFor } from '@testing-library/react';
import { beforeEach, expect, it, vi } from 'vitest';
import { PhotoStudio } from './PhotoStudio';
import { api, develop, editProfiles, pickWhiteBalance } from '../../ipc/client';
import type { RenderDto } from '../../ipc/types';
import { nativeRetouch } from '../../ipc/nativeRetouch';
import { forgetPreviews } from '../../state/previewCache';

vi.mock('../../ipc/client', () => ({
  inTauri: () => true,
  asIpcError: (error: Error) => ({ message: error.message }),
  api: { getPreview: vi.fn() },
  develop: { imageRecipe: vi.fn(), imageHistory: vi.fn(), renderImage: vi.fn(), enhancePhoto: vi.fn(), setParam: vi.fn(), historyStep: vi.fn(), snapshot: vi.fn() },
  editProfiles: { list: vi.fn() }, syncSettings: vi.fn(), pickWhiteBalance: vi.fn(),
}));

vi.mock('../../ipc/nativeRetouch', async () => ({ ...await vi.importActual('../../ipc/nativeRetouch'), nativeRetouch: { original: vi.fn() } }));

const pixels = { width: 1, height: 1, rgbBase64: btoa(String.fromCharCode(80, 100, 120)), notes: [] } as unknown as RenderDto;

beforeEach(() => {
  vi.resetAllMocks();
  forgetPreviews();
  vi.mocked(nativeRetouch.original).mockResolvedValue({ width: 1, height: 1, rgbBase64: btoa(String.fromCharCode(70, 90, 110)), notes: [] } as never);
  vi.mocked(api.getPreview).mockResolvedValue({ dataUrl: 'data:image/png;base64,original' } as never);
  vi.mocked(develop.imageRecipe).mockResolvedValue({ photoId: 'portrait', params: [] } as never);
  vi.mocked(develop.imageHistory).mockResolvedValue({ entries: [], canUndo: false, canRedo: false } as never);
  vi.mocked(develop.renderImage).mockResolvedValue(pixels);
  vi.mocked(editProfiles.list).mockResolvedValue([]);
});

it('saves named snapshots and restores them through the persisted history command', async () => {
  vi.mocked(develop.imageHistory).mockResolvedValue({ entries: [], snapshots: ['First look'], canUndo: true, canRedo: false } as never);
  render(<PhotoStudio projectId="project" photoId="portrait" disabled={false} onBusyChange={vi.fn()} />);
  await screen.findByAltText('Edited photograph');
  fireEvent.click(screen.getByText('Named snapshots (1)'));
  fireEvent.change(screen.getByLabelText('Snapshot name'), { target: { value: 'First look' } });
  expect((screen.getByText('Save snapshot') as HTMLButtonElement).disabled).toBe(true);
  fireEvent.change(screen.getByLabelText('Snapshot name'), { target: { value: '  Second look  ' } });
  fireEvent.click(screen.getByText('Save snapshot'));
  await waitFor(() => expect(develop.snapshot).toHaveBeenCalledWith({ projectId: 'project', photoId: 'portrait', action: 'take', name: 'Second look' }));
  await waitFor(() => expect((screen.getByRole('button', { name: 'Restore snapshot First look' }) as HTMLButtonElement).disabled).toBe(false));
  fireEvent.click(screen.getByRole('button', { name: 'Restore snapshot First look' }));
  await waitFor(() => expect(develop.snapshot).toHaveBeenLastCalledWith({ projectId: 'project', photoId: 'portrait', action: 'restore', name: 'First look' }));
});

it('provides a keyboard-accessible neutral picker with normalized original coordinates', async () => {
  render(<PhotoStudio projectId="project" photoId="portrait" disabled={false} onBusyChange={vi.fn()} />);
  await screen.findByAltText('Edited photograph');
  fireEvent.click(screen.getByText('White balance picker'));
  fireEvent.change(screen.getByLabelText('Horizontal position (%)'), { target: { value: '25' } });
  fireEvent.click(screen.getByText('Sample this position'));
  await waitFor(() => expect(pickWhiteBalance).toHaveBeenCalledWith('project', 'portrait', 0.25, 0.5));
});

it('keeps the last photo visible and blocks edits until the updated preview arrives', async () => {
  const busy = vi.fn();
  const pending: ((value: RenderDto) => void)[] = [];
  render(<PhotoStudio projectId="project" photoId="portrait" disabled={false} onBusyChange={busy} />);
  const preview = await screen.findByAltText('Edited photograph');
  await waitFor(() => expect(busy).toHaveBeenLastCalledWith(false));
  const before = preview.getAttribute('src');
  const calls = vi.mocked(develop.renderImage).mock.calls.length;
  vi.mocked(develop.renderImage).mockImplementation(() => new Promise(resolve => { pending.push(resolve); }));
  fireEvent.click(screen.getByRole('button', { name: 'Auto enhance photo' }));
  // The quick first look of the new version is asked for first, the full-quality one after it.
  await waitFor(() => expect(develop.renderImage).toHaveBeenCalledTimes(calls + 1));
  expect(vi.mocked(develop.renderImage).mock.calls[calls]?.[0].level).toBe('screen');
  expect(screen.getByAltText('Edited photograph').getAttribute('src')).toBe(before);
  expect((screen.getByRole('button', { name: 'Auto enhance photo' }) as HTMLButtonElement).disabled).toBe(true);
  await act(async () => { pending.shift()?.({ ...pixels, rgbBase64: btoa(String.fromCharCode(140, 160, 180)) }); });
  expect(screen.getByAltText('Edited photograph').getAttribute('src')).not.toBe(before);
  await waitFor(() => expect(develop.renderImage).toHaveBeenCalledTimes(calls + 2));
  expect(vi.mocked(develop.renderImage).mock.calls[calls + 1]?.[0].level).toBe('full');
  await act(async () => { pending.shift()?.({ ...pixels, rgbBase64: btoa(String.fromCharCode(150, 170, 190)) }); });
  await waitFor(() => expect(busy).toHaveBeenLastCalledWith(false));
});

it('surfaces a failed save and releases the parent lock on unmount', async () => {
  const busy = vi.fn();
  const view = render(<PhotoStudio projectId="project" photoId="portrait" disabled={false} onBusyChange={busy} />);
  await screen.findByAltText('Edited photograph');
  await waitFor(() => expect(busy).toHaveBeenLastCalledWith(false));
  vi.mocked(develop.enhancePhoto).mockRejectedValueOnce(new Error('Disk is full'));
  fireEvent.click(screen.getByRole('button', { name: 'Auto enhance photo' }));
  expect((await screen.findByRole('alert')).textContent).toContain('Disk is full');
  expect(screen.getByAltText('Edited photograph')).toBeTruthy();
  view.unmount();
  expect(busy).toHaveBeenLastCalledWith(false);
});

it('offers recovery when a renderer payload cannot be displayed', async () => {
  vi.mocked(develop.renderImage).mockResolvedValue({ ...pixels, rgbBase64: 'broken!' });
  render(<PhotoStudio projectId="project" photoId="portrait" disabled={false} onBusyChange={vi.fn()} />);
  expect((await screen.findByRole('alert')).textContent).toContain('incomplete image data');
  vi.mocked(develop.renderImage).mockResolvedValue(pixels);
  fireEvent.click(screen.getByRole('button', { name: 'Retry preview' }));
  expect(await screen.findByAltText('Edited photograph')).toBeTruthy();
});

it('jumps back to any saved automatic step and marks where the photo now is', async () => {
  vi.mocked(develop.imageHistory).mockResolvedValue({ entries: [
    { seq: 1, atMs: 0, source: 'ai', changed: ['global.exposure'], label: 'Auto edit 1/3 · Light & colour: portrait' },
    { seq: 2, atMs: 0, source: 'ai', changed: ['studio_retouch_v1'], label: 'Auto edit 2/3 · Skin: 1 face' },
    { seq: 3, atMs: 0, source: 'ai', changed: ['studio_retouch_v1'], label: 'Auto edit 3/3 · Eyes: measured' },
  ], snapshots: [], canUndo: true, canRedo: false } as never);
  vi.mocked(develop.historyStep).mockResolvedValue({} as never);
  render(<PhotoStudio projectId="project" photoId="portrait" disabled={false} onBusyChange={vi.fn()} />);
  await screen.findByAltText('Edited photograph');
  const head = await screen.findByText('Auto edit 3/3 · Eyes: measured');
  expect(head.closest('li')?.getAttribute('aria-current')).toBe('step');
  const back = screen.getByRole('button', { name: 'Go back to step 2: Auto edit 2/3 · Skin: 1 face' });
  await waitFor(() => expect((back as HTMLButtonElement).disabled).toBe(false));
  fireEvent.click(back);
  await waitFor(() => expect(develop.historyStep).toHaveBeenCalledWith({ projectId: 'project', photoId: 'portrait', action: 'goto:2' }));
  await waitFor(() => expect(screen.getByText('Auto edit 2/3 · Skin: 1 face').closest('li')?.getAttribute('aria-current')).toBe('step'));
});
