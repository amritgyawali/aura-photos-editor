import { act, fireEvent, render, screen } from '@testing-library/react';
import { afterEach, beforeEach, expect, it, vi } from 'vitest';
import { oneClickCancel, oneClickStatus } from '../../ipc/client';
import type { OneClickStatusDto } from '../../ipc/types';
import { automaticBusy, useAutomatic } from '../../state/automaticStore';
import { AutomaticProgress, summary } from './AutomaticProgress';

vi.mock('../../ipc/client', async importOriginal => ({
  ...await importOriginal<typeof import('../../ipc/client')>(),
  oneClickStatus: vi.fn(), oneClickCancel: vi.fn(),
}));

const row = (status: string): OneClickStatusDto => ({
  jobId: 'job', status, phase: 'edit', phaseLabel: 'Editing portraits', itemsDone: 2,
  itemsTotal: 4, frames: 4, aiEdited: 0, localEdited: 2, selected: 4, written: 0,
  verified: 0, destination: 'D:/photos', model: '', notes: [],
});

beforeEach(() => {
  vi.useFakeTimers();
  vi.resetAllMocks();
  useAutomatic.setState({ jobId: 'job', projectId: 'project', status: row('running'), starting: false, error: null });
});
afterEach(() => vi.useRealTimers());

it('keeps the running handle, cancellation and busy lock after a failed status read, then reconnects', async () => {
  vi.mocked(oneClickStatus).mockRejectedValueOnce(new Error('Status unavailable')).mockResolvedValue(row('completed'));
  const finished = vi.fn();
  render(<AutomaticProgress onFinished={finished} />);
  await act(async () => undefined);
  expect(useAutomatic.getState().jobId).toBe('job');
  expect(automaticBusy(useAutomatic.getState())).toBe(true);
  expect(screen.getByRole('button', { name: 'Stop automatic processing' })).toBeDefined();
  expect(screen.getByText(/2 of 4 edited/)).toBeDefined();
  await act(async () => { await vi.advanceTimersByTimeAsync(1600); });
  expect(finished).toHaveBeenCalledTimes(1);
  expect(finished).toHaveBeenCalledWith('project');
  expect(automaticBusy(useAutomatic.getState())).toBe(false);
  expect(useAutomatic.getState().error).toBeNull();
});

it('bounds retries and lets the user retry or stop the original native run', async () => {
  vi.mocked(oneClickStatus).mockRejectedValue(new Error('Disconnected'));
  vi.mocked(oneClickCancel).mockResolvedValue(true);
  const finished = vi.fn();
  render(<AutomaticProgress onFinished={finished} />);
  await act(async () => { await vi.advanceTimersByTimeAsync(20000); });
  expect(oneClickStatus).toHaveBeenCalledTimes(3);
  expect(screen.getByText(/Status checks are paused/)).toBeDefined();
  fireEvent.click(screen.getByRole('button', { name: 'Stop automatic processing' }));
  expect(oneClickCancel).toHaveBeenCalledWith('job');
  vi.mocked(oneClickStatus).mockResolvedValue(row('cancelled'));
  fireEvent.click(screen.getByRole('button', { name: 'Retry status' }));
  await act(async () => undefined);
  expect(finished).toHaveBeenCalledTimes(1);
  expect(finished).toHaveBeenCalledWith('project');
  expect(automaticBusy(useAutomatic.getState())).toBe(false);
});

it('waits for a pending read and ignores its result after the run changes', async () => {
  let resolve!: (value: OneClickStatusDto) => void;
  vi.mocked(oneClickStatus).mockImplementationOnce(() => new Promise(done => { resolve = done; }))
    .mockResolvedValue({ ...row('running'), jobId: 'new-job' });
  const finished = vi.fn();
  render(<AutomaticProgress onFinished={finished} />);
  await act(async () => { await vi.advanceTimersByTimeAsync(10000); });
  expect(oneClickStatus).toHaveBeenCalledTimes(1);
  act(() => useAutomatic.getState().adopt('new-job', 'new-project'));
  await act(async () => { resolve(row('completed')); });
  expect(useAutomatic.getState().jobId).toBe('new-job');
  expect(useAutomatic.getState().status?.status).toBe('running');
  expect(finished).not.toHaveBeenCalled();
});

it('does not poll the previous terminal run while waiting for the next handle', async () => {
  useAutomatic.setState({ starting: true, status: row('completed') });
  vi.mocked(oneClickStatus).mockResolvedValue({ ...row('running'), jobId: 'next' });
  render(<AutomaticProgress onFinished={vi.fn()} />);
  await act(async () => { await vi.advanceTimersByTimeAsync(5000); });
  expect(oneClickStatus).not.toHaveBeenCalled();
  act(() => useAutomatic.getState().adopt('next', 'project'));
  await act(async () => undefined);
  expect(oneClickStatus).toHaveBeenCalledTimes(1);
  expect(oneClickStatus).toHaveBeenCalledWith('next');
});

it('names photos that could not be opened, instead of calling them culled or failed edits', () => {
  const exporting: OneClickStatusDto = { ...row('running'), phase: 'export', frames: 1000, analyzed: 998, selected: 998,
    localEdited: 996, failedEdits: 4, written: 420, verified: 420 };
  const line = summary(exporting);
  expect(line).toContain('2 could not be opened');
  expect(line).not.toContain('left out by the cull');
  expect(line).toContain('2 failed edits');
  expect(line).toContain('420 written');
});
