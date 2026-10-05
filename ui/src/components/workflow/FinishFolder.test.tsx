import { cleanup, fireEvent, render, screen, waitFor } from '@testing-library/react';
import { afterEach, beforeEach, expect, it, vi } from 'vitest';

import { automaticStart, pickPhotoFolder } from '../../ipc/client';
import { useAutomatic } from '../../state/automaticStore';
import { FinishFolder, lookFor } from './FinishFolder';

vi.mock('../../ipc/client', async (importOriginal) => {
  const original = await importOriginal<typeof import('../../ipc/client')>();
  return { ...original, inTauri: () => true, automaticStart: vi.fn(), pickPhotoFolder: vi.fn() };
});

const FOLDER = 'D:\\Weddings\\Asha';
const run = { projectId: 'project-1', jobId: 'oneclick-1', ingestJobId: 'ingest-1', destination: 'C:\\out' };
const reference = { analysis: { id: 'ref-1', origin: 'saved photos' }, strength: 0.6 } as never;
const finish = () => screen.getByRole('button', { name: 'Finish a whole folder' });

beforeEach(() => {
  vi.mocked(automaticStart).mockReset().mockResolvedValue(run);
  vi.mocked(pickPhotoFolder).mockReset().mockResolvedValue(FOLDER);
  localStorage.clear();
  useAutomatic.setState({ jobId: null, projectId: null, status: null, starting: false, error: null });
});
afterEach(cleanup);

it('turns the chosen look into whole percentages and nothing into no look', () => {
  expect(lookFor(null, null)).toBeNull();
  expect(lookFor({ profileId: 'warm-film', strength: 0.8 }, null)).toEqual({ profileId: 'warm-film', profileStrength: 80, referenceId: null, referenceStrength: 0 });
  expect(lookFor({ profileId: 'warm-film', strength: 9 }, reference)).toEqual({ profileId: 'warm-film', profileStrength: 150, referenceId: 'ref-1', referenceStrength: 60 });
});

it('starts the whole run from one folder and hands the job to the shell', async () => {
  const started = vi.fn();
  render(<FinishFolder disabled={false} profile={{ profileId: 'warm-film', strength: 1 }} reference={null} onStarted={started} onError={vi.fn()} />);
  fireEvent.click(finish());
  await waitFor(() => expect(started).toHaveBeenCalledWith('project-1'));
  expect(automaticStart).toHaveBeenCalledWith({
    roots: [FOLDER], projectId: null, keepEverything: false, destination: null,
    look: { profileId: 'warm-film', profileStrength: 100, referenceId: null, referenceStrength: 0 },
  });
  expect(useAutomatic.getState()).toMatchObject({ jobId: 'oneclick-1', projectId: 'project-1', starting: false });
});

it('remembers that the cull was switched off and delivers everything', async () => {
  const first = render(<FinishFolder disabled={false} profile={null} reference={null} onStarted={vi.fn()} onError={vi.fn()} />);
  fireEvent.click(screen.getByRole('checkbox'));
  first.unmount();
  render(<FinishFolder disabled={false} profile={null} reference={null} onStarted={vi.fn()} onError={vi.fn()} />);
  expect((screen.getByRole('checkbox') as HTMLInputElement).checked).toBe(false);
  fireEvent.click(finish());
  await waitFor(() => expect(automaticStart).toHaveBeenCalledWith({ roots: [FOLDER], projectId: null, look: null, keepEverything: true, destination: null }));
});

it('finishes a pasted folder without a dialog and sends the export where it was told', async () => {
  const started = vi.fn();
  render(<FinishFolder disabled={false} profile={null} reference={null} onStarted={started} onError={vi.fn()} />);
  fireEvent.change(screen.getByLabelText('Folder to finish'), { target: { value: ' E:\\Card\\DCIM ' } });
  fireEvent.change(screen.getByLabelText(/Export into/), { target: { value: 'F:\\Delivered' } });
  fireEvent.click(finish());
  await waitFor(() => expect(started).toHaveBeenCalledWith('project-1'));
  expect(pickPhotoFolder).not.toHaveBeenCalled();
  expect(automaticStart).toHaveBeenCalledWith({ roots: ['E:\\Card\\DCIM'], projectId: null, look: null, keepEverything: false, destination: 'F:\\Delivered' });
  expect(localStorage.getItem('aura.finish.export')).toBe('F:\\Delivered');
  expect((screen.getByLabelText('Folder to finish') as HTMLInputElement).value).toBe('');
});

it('does nothing when the folder dialog is cancelled and reports a refusal', async () => {
  vi.mocked(pickPhotoFolder).mockResolvedValueOnce(null);
  const failed = vi.fn();
  render(<FinishFolder disabled={false} profile={null} reference={null} onStarted={vi.fn()} onError={failed} />);
  fireEvent.click(finish());
  await waitFor(() => expect(pickPhotoFolder).toHaveBeenCalledTimes(1));
  await waitFor(() => expect(finish().hasAttribute('disabled')).toBe(false));
  expect(automaticStart).not.toHaveBeenCalled();
  expect(useAutomatic.getState().starting).toBe(false);

  const refusal = { code: 'AURA-RENDER-8001', message: 'Another automatic run is active. Stop it or wait for delivery.', runbookUrl: 'https://aura.app/e/AURA-RENDER-8001', retryable: false };
  vi.mocked(automaticStart).mockRejectedValueOnce(refusal);
  fireEvent.click(finish());
  await waitFor(() => expect(failed).toHaveBeenCalledWith({ code: refusal.code, message: refusal.message }));
  expect(useAutomatic.getState().starting).toBe(false);
});
