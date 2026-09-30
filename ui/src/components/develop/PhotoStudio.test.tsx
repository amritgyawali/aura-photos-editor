import { act, fireEvent, render, screen, waitFor } from '@testing-library/react';
import { beforeEach, expect, it, vi } from 'vitest';
import { PhotoStudio } from './PhotoStudio';
import { api, develop, editProfiles, pickWhiteBalance } from '../../ipc/client';
import type { RenderDto } from '../../ipc/types';

vi.mock('../../ipc/client', () => ({
  inTauri: () => true,
  asIpcError: (error: Error) => ({ message: error.message }),
  api: { getPreview: vi.fn() },
  develop: { imageRecipe: vi.fn(), imageHistory: vi.fn(), renderImage: vi.fn(), enhancePhoto: vi.fn(), setParam: vi.fn(), historyStep: vi.fn(), snapshot: vi.fn() },
  editProfiles: { list: vi.fn() }, syncSettings: vi.fn(), pickWhiteBalance: vi.fn(),
}));

const pixels = { width: 1, height: 1, rgbBase64: btoa(String.fromCharCode(80, 100, 120)), notes: [] } as unknown as RenderDto;

beforeEach(() => {
  vi.resetAllMocks();
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
  let complete: (value: RenderDto) => void = () => { throw new Error('No pending render'); };
  render(<PhotoStudio projectId="project" photoId="portrait" disabled={false} onBusyChange={busy} />);
  const preview = await screen.findByAltText('Edited photograph');
  await waitFor(() => expect(busy).toHaveBeenLastCalledWith(false));
  const before = preview.getAttribute('src');
  vi.mocked(develop.renderImage).mockImplementationOnce(() => new Promise(resolve => { complete = resolve; }));
  fireEvent.click(screen.getByRole('button', { name: 'Auto enhance photo' }));
  await waitFor(() => expect(develop.renderImage).toHaveBeenCalledTimes(2));
  expect(screen.getByAltText('Edited photograph').getAttribute('src')).toBe(before);
  expect((screen.getByRole('button', { name: 'Auto enhance photo' }) as HTMLButtonElement).disabled).toBe(true);
  await act(async () => complete({ ...pixels, rgbBase64: btoa(String.fromCharCode(140, 160, 180)) }));
  expect(screen.getByAltText('Edited photograph').getAttribute('src')).not.toBe(before);
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
  vi.mocked(develop.renderImage).mockResolvedValueOnce({ ...pixels, rgbBase64: 'broken!' });
  render(<PhotoStudio projectId="project" photoId="portrait" disabled={false} onBusyChange={vi.fn()} />);
  expect((await screen.findByRole('alert')).textContent).toContain('incomplete image data');
  fireEvent.click(screen.getByRole('button', { name: 'Retry preview' }));
  expect(await screen.findByAltText('Edited photograph')).toBeTruthy();
});
