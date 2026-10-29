import { act, fireEvent, render, screen } from '@testing-library/react';
import { afterEach, beforeEach, expect, it, vi } from 'vitest';
import type { ActivityStatus } from '../../ipc/activity';
import { elapsed, remaining, StatusBar } from './StatusBar';
import { beginRequest, describeCommand, endRequest, inflightRequests, lastRequestActivity, recentRequests, resetInflight } from '../../state/inflight';

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

// ADR-0188: why the sections are locked, what is awaited, and a way out of a lock nobody holds.

const idle: ActivityStatus = { items: [], imageWorkersBusy: 0, imageWorkers: 3 };

async function tick(ms: number, clock: { t: number }): Promise<void> {
  await act(async () => { clock.t += ms; vi.advanceTimersByTime(ms); });
  await act(async () => undefined);
}

it('names the lock while work is running and does not offer to break it', async () => {
  resetInflight();
  const clock = { t: 1_000_000 };
  const onUnlock = vi.fn();
  render(<StatusBar read={() => Promise.resolve(busy)} now={() => clock.t} onUnlock={onUnlock}
    locks={[{ key: 'edit', label: 'Auto edit is running', active: true }, { key: 'export', label: 'Exporting', active: false }]} />);
  await act(async () => undefined);
  await tick(20_000, clock);
  expect(screen.queryByText('Unlock sections')).toBeNull();
  fireEvent.click(screen.getByText('Details'));
  expect(screen.getByText('Auto edit is running')).toBeTruthy();
  expect(screen.queryByText('Exporting')).toBeNull();
  expect(screen.getByText('locked for 20 s')).toBeTruthy();
});

it('says when the sections are locked but nothing is running, and unlocks them', async () => {
  resetInflight();
  const clock = { t: 2_000_000 };
  const onUnlock = vi.fn();
  render(<StatusBar read={() => Promise.resolve(idle)} now={() => clock.t} onUnlock={onUnlock}
    locks={[{ key: 'folder', label: 'Finish a folder is running', active: true }]} />);
  await act(async () => undefined);
  expect(screen.getByText(/Waiting: Finish a folder is running/)).toBeTruthy();
  expect(screen.queryByText(/Idle/)).toBeNull();
  await tick(16_000, clock);
  expect(screen.getByText(/but nothing is running/)).toBeTruthy();
  fireEvent.click(screen.getByText('Unlock sections'));
  expect(onUnlock).toHaveBeenCalledTimes(1);
});

it('does not call a lock stale while a request is still awaited', async () => {
  resetInflight();
  const clock = { t: 3_000_000 };
  beginRequest('full_edit_photo', false, clock.t);
  render(<StatusBar read={() => Promise.resolve(idle)} now={() => clock.t} onUnlock={() => undefined}
    locks={[{ key: 'edit', label: 'Auto edit is running', active: true }]} />);
  await act(async () => undefined);
  await tick(40_000, clock);
  expect(screen.queryByText('Unlock sections')).toBeNull();
  expect(screen.getByText('1 waiting')).toBeTruthy();
  fireEvent.click(screen.getByText('Details'));
  expect(screen.getByText('Full automatic edit of a photo')).toBeTruthy();
  expect(screen.getByText(/40 s - slow/)).toBeTruthy();
  // Locked long enough, the sections can be opened by hand even so.
  expect(screen.getByText('Unlock anyway')).toBeTruthy();
});

it('shows what is awaited when nothing is locked, and the history once it answers', async () => {
  resetInflight();
  const clock = { t: 4_000_000 };
  const id = beginRequest('analyse_reference_style', false, clock.t);
  render(<StatusBar read={() => Promise.resolve(idle)} now={() => clock.t} />);
  await act(async () => undefined);
  expect(screen.getByText('Waiting for the engine: Analysing the reference style (0 s)')).toBeTruthy();
  endRequest(id, false, 'The folder has no photos.', clock.t + 2000);
  await tick(2000, clock);
  expect(screen.getByText('Idle - nothing is running.')).toBeTruthy();
  fireEvent.click(screen.getByText('Details'));
  expect(screen.getByText(/failed: The folder has no photos\./)).toBeTruthy();
});

it('flags a job whose step has not changed for a minute, and estimates the time left', async () => {
  resetInflight();
  const clock = { t: 5_000_000 };
  let done = 10;
  const job = (): ActivityStatus => ({ items: [{ id: 3, kind: 'cull', label: 'Culling 100 photos', detail: 'Checking', done, total: 100, seconds: 1 }], imageWorkersBusy: 1, imageWorkers: 3 });
  render(<StatusBar read={() => Promise.resolve(job())} now={() => clock.t} />);
  await act(async () => undefined);
  done = 20;
  await tick(10_000, clock);
  // 10 photos in 10 s: 80 left takes about 80 s.
  expect(screen.getByText('about 1 min 20 s left')).toBeTruthy();
  for (let i = 0; i < 61; i += 1) await tick(1000, clock);
  expect(screen.getByText(/^No change for 1 min 1 s/)).toBeTruthy();
});

it('moves a job that ended into the history', async () => {
  resetInflight();
  const clock = { t: 6_000_000 };
  let status: ActivityStatus = { items: [{ id: 9, kind: 'edit', label: 'Full edit: IMG_0001.CR2', detail: 'Stage 3 of 10', done: 2, total: 10, seconds: 12 }], imageWorkersBusy: 1, imageWorkers: 3 };
  render(<StatusBar read={() => Promise.resolve(status)} now={() => clock.t} />);
  await act(async () => undefined);
  status = idle;
  await tick(1000, clock);
  fireEvent.click(screen.getByText('Details'));
  expect(screen.getByText(/Auto edit: Full edit: IMG_0001\.CR2/)).toBeTruthy();
  expect(screen.getByText('ended after 13 s')).toBeTruthy();
});

it('estimates nothing until it has seen progress for a few seconds', () => {
  expect(remaining({ firstDone: 0, firstAt: 0 }, 0, 10, 60_000)).toBeNull();
  expect(remaining({ firstDone: 0, firstAt: 0 }, 5, 10, 1000)).toBeNull();
  expect(remaining({ firstDone: 0, firstAt: 0 }, 5, 10, 10_000)).toBe(10);
  expect(remaining({ firstDone: 0, firstAt: 0 }, 10, 10, 10_000)).toBeNull();
});

it('reads command names as words', () => {
  expect(describeCommand('full_edit_photo')).toBe('Full automatic edit of a photo');
  expect(describeCommand('save_local_masks')).toBe('Save local masks');
  expect(describeCommand('plugin:dialog|open')).toBe('Waiting for you to choose in the file dialog');
});

it('keeps quick previews out of the history and slow ones in it', () => {
  resetInflight();
  endRequest(beginRequest('get_preview', true, 0), true, null, 200);
  expect(recentRequests()).toHaveLength(0);
  expect(lastRequestActivity()).toBe(0);
  endRequest(beginRequest('get_preview', true, 0), true, null, 4000);
  expect(recentRequests()).toHaveLength(1);
  beginRequest('activity_status', false, 0);
  expect(inflightRequests()).toHaveLength(0);
});

it('does not say idle while a slow preview is still awaited', async () => {
  resetInflight();
  const clock = { t: 7_000_000 };
  beginRequest('render_image', true, clock.t);
  render(<StatusBar read={() => Promise.resolve(idle)} now={() => clock.t} />);
  await act(async () => undefined);
  // A quick preview is not worth a line.
  expect(screen.getByText('Idle - nothing is running.')).toBeTruthy();
  await tick(4000, clock);
  expect(screen.getByText('Waiting for the engine: Rendering the photo (4 s)')).toBeTruthy();
});
