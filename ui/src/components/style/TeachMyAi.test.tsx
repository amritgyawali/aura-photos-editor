import { act, cleanup, fireEvent, render, screen, waitFor } from '@testing-library/react';
import { afterEach, beforeEach, expect, it, vi } from 'vitest';
import { api, style } from '../../ipc/client';
import { TeachMyAi } from './TeachMyAi';
import type { TrainProfileDto } from '../../ipc/types';

vi.mock('../../ipc/client', () => ({
  api: { cancelJob: vi.fn().mockResolvedValue(true) },
  asIpcError: (error: unknown) => ({ code: 'TEST', message: String(error) }),
  style: { scanArchive: vi.fn(), trainProfile: vi.fn() },
}));
beforeEach(() => {
  vi.clearAllMocks();
  vi.mocked(style.scanArchive).mockResolvedValue({ originals: 120, finals: 120, matched: 120,
    unmatchedOriginals: 0, unmatchedFinals: 0, byMethod: [], weakestMethod: 'filename_stem', enough: true });
});
afterEach(cleanup);
async function scan() {
  fireEvent.change(screen.getByLabelText('Folders, one per line'), { target: { value: 'C:/archive' } });
  fireEvent.click(screen.getByRole('button', { name: 'Look at these folders' }));
  await waitFor(() => expect((screen.getByRole('button', { name: 'Learn my style' }) as HTMLButtonElement).disabled).toBe(false));
}
it('invalidates an earlier successful scan before a failed rescan', async () => {
  render(<TeachMyAi projectId="project" onError={vi.fn()} />);
  await scan();
  vi.mocked(style.scanArchive).mockRejectedValueOnce(new Error('Folder disconnected'));
  fireEvent.click(screen.getByRole('button', { name: 'Look at these folders' }));
  await waitFor(() => expect(screen.queryByRole('button', { name: 'Learn my style' })).toBeNull());
  expect(style.trainProfile).not.toHaveBeenCalled();
});
it('cancels the active unique job and does not publish an older profile returned by cancellation', async () => {
  let finish!: (value: TrainProfileDto) => void;
  vi.mocked(style.trainProfile).mockReturnValue(new Promise(resolve => { finish = resolve; }));
  const trained = vi.fn();
  render(<TeachMyAi projectId="project" onTrained={trained} />);
  await scan(); fireEvent.click(screen.getByRole('button', { name: 'Learn my style' }));
  fireEvent.click(await screen.findByRole('button', { name: 'Stop learning' }));
  const id = vi.mocked(style.trainProfile).mock.calls[0]?.[0].cancelId;
  expect(id).toMatch(/^[0-9a-f-]{36}$/);
  expect(api.cancelJob).toHaveBeenCalledWith(id);
  await act(async () => { finish({ cancelled: true, profile: { profileId: 'older' } } as TrainProfileDto); });
  expect(trained).not.toHaveBeenCalled();
  expect(screen.getByText(/Stopped\. Everything/)).toBeTruthy();
});
it('requests cancellation if a training workspace is unmounted', async () => {
  vi.mocked(style.trainProfile).mockReturnValue(new Promise(() => undefined));
  const view = render(<TeachMyAi projectId="project" />);
  await scan(); fireEvent.click(screen.getByRole('button', { name: 'Learn my style' }));
  const id = vi.mocked(style.trainProfile).mock.calls[0]?.[0].cancelId;
  view.unmount();
  expect(api.cancelJob).toHaveBeenCalledWith(id);
});
