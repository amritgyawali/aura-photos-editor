import { act, fireEvent, render, screen, waitFor } from '@testing-library/react';
import { beforeEach, expect, it, vi } from 'vitest';
import { api, pairedReference, pickPairedReference, type PairedReferenceReport } from '../../ipc/client';
import { PairedReference } from './PairedReference';

vi.mock('../../ipc/client', () => ({
  inTauri: () => true, asIpcError: (e: Error) => ({ message: e.message }),
  api: { cancelJob: vi.fn().mockResolvedValue(undefined) },
  pairedReference: { match: vi.fn() }, pickPairedReference: vi.fn(),
}));
const result: PairedReferenceReport = { changed: 3, probes: 54, beforeError: 5.3, afterError: 4.8,
  exactMatch: false, matchingPixels: 10, totalPixels: 100, protectedFields: 2,
  referenceHash: 'hash', width: 10, height: 10, subjectErrorsBefore: [5.3], subjectErrorsAfter: [4.8] };
beforeEach(() => {
  vi.clearAllMocks();
  vi.mocked(pickPairedReference).mockResolvedValue('C:/reference.png');
  vi.mocked(pairedReference.match).mockResolvedValue(result);
});
const onApply = async (action: () => Promise<unknown>) => { await action(); };

it('matches only the selected photo and reports remaining differences accurately', async () => {
  render(<PairedReference photoId="photo" disabled={false} onApply={onApply} />);
  fireEvent.click(screen.getByRole('button', { name: 'Choose finished reference' }));
  await screen.findByText(/Pixels still differ/);
  expect(pairedReference.match).toHaveBeenCalledWith('photo', 'C:/reference.png', expect.any(String));
  expect(screen.getByText(/5.300 → 4.800/)).toBeTruthy();
  expect(screen.getByText(/Use Undo to compare/)).toBeTruthy();
  expect(screen.queryByText(/Pixels match exactly/)).toBeNull();
});

it('prevents duplicate work and stops an active native job without claiming a saved edit', async () => {
  let fail!: (error: Error) => void;
  vi.mocked(pairedReference.match).mockReturnValue(new Promise((_resolve, reject) => { fail = reject; }));
  const view = render(<PairedReference photoId="photo" disabled={false} onApply={onApply} />);
  fireEvent.click(screen.getByRole('button', { name: 'Choose finished reference' }));
  await waitFor(() => expect(pairedReference.match).toHaveBeenCalledOnce());
  expect((screen.getByRole('button', { name: 'Matching reference…' }) as HTMLButtonElement).disabled).toBe(true);
  fireEvent.click(screen.getByRole('button', { name: 'Stop reference matching' }));
  expect(api.cancelJob).toHaveBeenCalledWith(vi.mocked(pairedReference.match).mock.calls[0]?.[2]);
  await act(async () => fail(new Error('Stopped. No edit saved.')));
  expect(screen.getByRole('alert').textContent).toContain('No edit saved');
  expect(screen.queryByText(/Saved 3/)).toBeNull();
  view.unmount();
});

it('a cancelled picker does not start fitting or change the photo', async () => {
  vi.mocked(pickPairedReference).mockResolvedValue(null);
  render(<PairedReference photoId="photo" disabled={false} onApply={onApply} />);
  fireEvent.click(screen.getByRole('button', { name: 'Choose finished reference' }));
  await waitFor(() => expect((screen.getByRole('button', { name: 'Choose finished reference' }) as HTMLButtonElement).disabled).toBe(false));
  expect(pairedReference.match).not.toHaveBeenCalled();
});
