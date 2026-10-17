import { act, render, screen } from '@testing-library/react';
import { afterEach, beforeEach, expect, it, vi } from 'vitest';
import type { ActivityStatus } from '../../ipc/activity';
import { elapsed, StatusBar } from './StatusBar';

beforeEach(() => vi.useFakeTimers());
afterEach(() => vi.useRealTimers());

const busy: ActivityStatus = {
  items: [
    { id: 0, kind: 'folder', label: 'Finishing a folder of 1000 photographs (export)', detail: 'Exporting photo 421 of 998: source_0421.jpg written and read back.', done: 421, total: 998, seconds: 3725 },
    { id: 7, kind: 'edit', label: 'Auto edit: IMG_0042.jpg', detail: 'Step 2 of 6: Skin', done: 1, total: 6, seconds: 4 },
  ],
  imageWorkersBusy: 3,
  imageWorkers: 3,
};

it('shows every piece of work with its photograph, progress and time, and how many workers are busy', async () => {
  render(<StatusBar read={() => Promise.resolve(busy)} />);
  await act(async () => undefined);
  expect(screen.getByText('Finishing a folder of 1000 photographs (export)')).toBeTruthy();
  expect(screen.getByText(/source_0421\.jpg written and read back/)).toBeTruthy();
  expect(screen.getByText('421 of 998 (42%)')).toBeTruthy();
  expect(screen.getByText('1 h 02 min')).toBeTruthy();
  expect(screen.getByText('Step 2 of 6: Skin')).toBeTruthy();
  expect(screen.getByText('Image workers: 3 of 3 busy')).toBeTruthy();
  expect(screen.getAllByRole('progressbar')).toHaveLength(2);
});

it('says idle only when nothing is running, and keeps asking', async () => {
  const read = vi.fn<() => Promise<ActivityStatus>>()
    .mockResolvedValueOnce({ items: [], imageWorkersBusy: 0, imageWorkers: 3 })
    .mockResolvedValue(busy);
  render(<StatusBar read={read} />);
  await act(async () => undefined);
  expect(screen.getByText('Idle - nothing is running.')).toBeTruthy();
  await act(async () => { vi.advanceTimersByTime(1000); });
  await act(async () => undefined);
  expect(screen.queryByText('Idle - nothing is running.')).toBeNull();
  expect(read).toHaveBeenCalledTimes(2);
});

it('does not claim idle when the status cannot be read', async () => {
  render(<StatusBar read={() => Promise.reject(new Error('gone'))} />);
  await act(async () => undefined);
  expect(screen.getByText(/Status unavailable/)).toBeTruthy();
  expect(screen.queryByText(/Idle/)).toBeNull();
});

it('formats elapsed time', () => {
  expect(elapsed(9)).toBe('9 s');
  expect(elapsed(192)).toBe('3 min 12 s');
  expect(elapsed(3840)).toBe('1 h 04 min');
});
