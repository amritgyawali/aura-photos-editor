import { fireEvent, render, screen, waitFor } from '@testing-library/react';
import { beforeEach, expect, it, vi } from 'vitest';
import { ImportWizard } from './ImportWizard';
import { pickPhotos, pickPhotoFolder } from '../ipc/client';

vi.mock('../ipc/client', () => ({
  inTauri: () => true, pickPhotos: vi.fn(), pickPhotoFolder: vi.fn(),
  asIpcError: (error: Error) => ({ message: error.message }),
}));
beforeEach(() => vi.clearAllMocks());
const props = { disabled: false, running: false, done: 0, total: 0, onCancel: vi.fn() };

it('imports just the selected photos and leaves a cancelled picker alone', async () => {
  const start = vi.fn();
  vi.mocked(pickPhotos).mockResolvedValueOnce(['D:/one.jpg', 'D:/two.png']).mockResolvedValueOnce([]);
  render(<ImportWizard {...props} onStart={start} />);
  fireEvent.click(screen.getByRole('button', { name: 'Choose photos' }));
  await waitFor(() => expect(start).toHaveBeenCalledWith(['D:/one.jpg', 'D:/two.png']));
  await waitFor(() => expect(screen.getByRole('button', { name: 'Choose photos' }).hasAttribute('disabled')).toBe(false));
  fireEvent.click(screen.getByRole('button', { name: 'Choose photos' }));
  await waitFor(() => expect(pickPhotos).toHaveBeenCalledTimes(2));
  expect(start).toHaveBeenCalledOnce();
});

it('shows picker errors and lets the user retry', async () => {
  vi.mocked(pickPhotoFolder).mockRejectedValueOnce(new Error('Folder picker unavailable'));
  render(<ImportWizard {...props} onStart={vi.fn()} />);
  fireEvent.click(screen.getByRole('button', { name: 'Choose photo folder' }));
  expect(await screen.findByRole('alert')).toHaveProperty('textContent', 'Folder picker unavailable');
  expect(screen.getByRole('button', { name: 'Choose photo folder' }).hasAttribute('disabled')).toBe(false);
});
