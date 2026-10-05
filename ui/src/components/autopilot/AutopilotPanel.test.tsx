import { fireEvent, render, screen, waitFor } from '@testing-library/react';
import { beforeEach, expect, it, vi } from 'vitest';
import { AutopilotPanel } from './AutopilotPanel';
import { api, autopilot } from '../../ipc/client';
import { prepareCollection } from './prepareCollection';

vi.mock('./prepareCollection', () => ({ prepareCollection: vi.fn().mockResolvedValue([{ photoId: 'photo', name: 'photo.jpg', outcome: 'ready' }]) }));

vi.mock('../../ipc/client', () => ({
  api: { listImages: vi.fn().mockResolvedValue([{ id: 'a', fileName: 'portrait.jpg' }, { id: 'b', fileName: 'landscape.jpg' }]) },
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

it('retries only failed photos while keeping completed results', async () => {
  vi.mocked(prepareCollection).mockResolvedValueOnce([
    { photoId: 'a', name: 'a.jpg', outcome: 'ready', detail: 'Saved' },
    { photoId: 'b', name: 'b.jpg', outcome: 'failed', detail: 'Decode failed' },
  ]);
  render(<AutopilotPanel projectId="p" onError={vi.fn()} />);
  fireEvent.click(screen.getByRole('button', { name: 'Auto edit all photos' }));
  const retry = await screen.findByRole('button', { name: 'Retry failed photos only' });
  vi.mocked(prepareCollection).mockImplementationOnce(async (_p, _stopped, _progress, onPhoto) => {
    const result = { photoId: 'b', name: 'b.jpg', outcome: 'ready' as const, detail: 'Saved after retry' };
    onPhoto(result);
    return [result];
  });
  fireEvent.click(retry);
  await screen.findByText('2 photos have saved edits, ready to render.');
  expect(vi.mocked(prepareCollection).mock.calls[1]?.[7]).toEqual(['b']);
  expect(screen.getByText('a.jpg · Prepared')).toBeTruthy();
  expect(screen.queryByRole('button', { name: 'Retry failed photos only' })).toBeNull();
});

it('sends the selected photo IDs to editing and prevents an empty batch', async () => {
  render(<AutopilotPanel projectId="p" onError={vi.fn()} />);
  const details = screen.getByText(/Choose photos/).closest('details')!;
  details.open = true;
  fireEvent(details, new Event('toggle'));
  await screen.findByLabelText('portrait.jpg');
  expect(api.listImages).toHaveBeenCalledWith(expect.objectContaining({ projectId: 'p' }));
  fireEvent.click(screen.getByLabelText('portrait.jpg'));
  fireEvent.click(screen.getByRole('button', { name: 'Auto edit 1 selected photos' }));
  await waitFor(() => expect(prepareCollection).toHaveBeenCalledOnce());
  expect(vi.mocked(prepareCollection).mock.calls[0]?.[7]).toEqual(['b']);
  await screen.findByText('1 photos have saved edits, ready to render.');
  fireEvent.click(screen.getByText('Clear selection'));
  expect((screen.getByRole('button', { name: 'Auto edit 0 selected photos' }) as HTMLButtonElement).disabled).toBe(true);
});
