import { fireEvent, render, screen, waitFor } from '@testing-library/react';
import { beforeEach, expect, it, vi } from 'vitest';
import { AutopilotPanel } from './AutopilotPanel';
import { autopilot } from '../../ipc/client';

vi.mock('./prepareCollection', () => ({ prepareCollection: vi.fn().mockResolvedValue([{ photoId: 'photo', name: 'photo.jpg', outcome: 'ready' }]) }));

vi.mock('../../ipc/client', () => ({
  asIpcError: (error: Error) => ({ message: error.message, code: 'test' }),
  autopilot: {
    autopilotStatus: vi.fn(), autopilotStages: vi.fn().mockResolvedValue([]),
    autopilotSummary: vi.fn().mockResolvedValue(null), autopilotEvents: vi.fn().mockResolvedValue([]),
    autopilotProgress: vi.fn().mockResolvedValue(null), autopilotPreflight: vi.fn(), autopilotStart: vi.fn(),
  },
}));
vi.mock('./Autopilot', () => ({ Autopilot: ({ onPreflight, preflight }: { onPreflight: () => void; preflight: unknown }) => <><button onClick={onPreflight}>Start edit</button>{preflight && <p>Needs attention</p>}</> }));
beforeEach(() => {
  vi.clearAllMocks();
  vi.mocked(autopilot.autopilotStatus).mockResolvedValue({ zeroTouch: true } as never);
  vi.mocked(autopilot.autopilotStart).mockResolvedValue(null as never);
});
it('starts after a passing preflight with one user click', async () => {
  vi.mocked(autopilot.autopilotPreflight).mockResolvedValue({ permitsStart: true } as never);
  render(<AutopilotPanel projectId="p" onError={vi.fn()} />);
  fireEvent.click(screen.getByText('Start edit'));
  await waitFor(() => expect(autopilot.autopilotStart).toHaveBeenCalledOnce());
  expect(screen.queryByText('Needs attention')).toBeNull();
});
it('shows a blocked preflight without starting a job', async () => {
  vi.mocked(autopilot.autopilotPreflight).mockResolvedValue({ permitsStart: false } as never);
  render(<AutopilotPanel projectId="p" onError={vi.fn()} />);
  fireEvent.click(screen.getByText('Start edit'));
  await screen.findByText('Needs attention');
  expect(autopilot.autopilotStart).not.toHaveBeenCalled();
});
it('ignores repeated clicks while the preflight is pending', async () => {
  let finish!: (value: never) => void;
  vi.mocked(autopilot.autopilotPreflight).mockReturnValue(new Promise(resolve => { finish = resolve; }));
  render(<AutopilotPanel projectId="p" onError={vi.fn()} />);
  fireEvent.click(screen.getByText('Start edit'));
  fireEvent.click(screen.getByText('Start edit'));
  await waitFor(() => expect(autopilot.autopilotPreflight).toHaveBeenCalledOnce());
  finish({ permitsStart: true } as never);
  await waitFor(() => expect(autopilot.autopilotStart).toHaveBeenCalledOnce());
});

it('consumes an import hand-off once and prepares photos without optional model stages', async () => {
  vi.mocked(autopilot.autopilotPreflight).mockResolvedValue({ permitsStart: true } as never);
  const consumed = vi.fn();
  const { rerender } = render(<AutopilotPanel projectId="p" automaticRequest={1} onAutomaticConsumed={consumed} onError={vi.fn()} />);
  await screen.findByText('1 photos have saved edits, ready to render.');
  rerender(<AutopilotPanel projectId="p" automaticRequest={1} onAutomaticConsumed={consumed} onError={vi.fn()} />);
  expect(consumed).toHaveBeenCalledOnce();
  expect(autopilot.autopilotStart).not.toHaveBeenCalled();
  expect(autopilot.autopilotPreflight).not.toHaveBeenCalled();
});

it('offers local editing without advanced preflight and then opens export', async () => {
  const onRender = vi.fn();
  render(<AutopilotPanel projectId="p" onError={vi.fn()} onRender={onRender} />);
  fireEvent.click(screen.getByRole('button', { name: 'Auto edit all photos' }));
  await screen.findByText('1 photos have saved edits, ready to render.');
  expect(autopilot.autopilotPreflight).not.toHaveBeenCalled();
  fireEvent.click(screen.getByRole('button', { name: /Render final output/ }));
  expect(onRender).toHaveBeenCalledOnce();
});
