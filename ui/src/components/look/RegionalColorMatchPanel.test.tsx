import { act, fireEvent, render, screen, waitFor } from '@testing-library/react';
import { beforeEach, expect, it, vi } from 'vitest';
import { RegionalColorMatchPanel } from './RegionalColorMatchPanel';
import { regionalColorMatch, type RegionalMatchReport } from '../../ipc/regionalColorMatch';
import { api } from '../../ipc/client';

vi.mock('../../ipc/client', () => ({ inTauri: () => true, api: { cancelJob: vi.fn() }, asIpcError: (error: Error) => ({ message: error.message }) }));
vi.mock('../../ipc/regionalColorMatch', () => ({
  REGIONAL_MATCH_LABELS: { face_skin: 'Face skin', body_skin: 'Body skin', hair: 'Hair', clothes: 'Clothing', background: 'Background' },
  regionalColorMatch: { status: vi.fn(), prepare: vi.fn(), apply: vi.fn() },
}));
const controls = { amount: 1, tone: 1, color: 1, whiteBalance: 1 };
const report: RegionalMatchReport = {
  sessionId: 'regions', photoId: 'photo', reference: { id: 'ref', name: 'Evening reference', width: 200, height: 300, convertedIcc: false, regions: [] },
  recipeHash: 'saved-recipe', current: true, fullResolutionChecked: false, changed: 0,
  regions: [{ name: 'face_skin', controls, beforeDistance: 4, afterDistance: 2, correctionAvailable: true, reason: null }, { name: 'background', controls, beforeDistance: 6, afterDistance: 3, correctionAvailable: true, reason: null }],
  unavailable: [{ name: 'hair', reason: 'No shared supported hair selection.' }],
};
const onBusyChange = vi.fn();
const onApply = async (action: () => Promise<unknown>) => { await action(); };
const props = { photoId: 'photo', referenceId: 'ref', referenceAvailable: true, disabled: false, onApply, onBusyChange };
beforeEach(() => {
  vi.clearAllMocks();
  vi.mocked(api.cancelJob).mockResolvedValue(true);
  vi.mocked(regionalColorMatch.status).mockResolvedValue(null);
  vi.mocked(regionalColorMatch.prepare).mockResolvedValue(report);
  vi.mocked(regionalColorMatch.apply).mockImplementation(async (_photo, _session, _hash, strengths) => ({
    ...report, changed: 2, fullResolutionChecked: true,
    regions: report.regions.map(region => ({ ...region, controls: strengths[region.name] ?? controls })),
  }));
});
const open = () => fireEvent.click(screen.getByText('Match skin, hair, clothing and background separately'));
it('links only the changed strength across fitted regions without copying other draft fields', async () => {
  render(<RegionalColorMatchPanel {...props}/>); open();
  fireEvent.click(screen.getByRole('button', { name: 'Analyse regional colour match' }));
  await screen.findByRole('slider', { name: 'Face skin match amount' });
  fireEvent.change(screen.getByRole('slider', { name: 'Background match white balance' }), { target: { value: '150' } });
  fireEvent.click(screen.getByRole('checkbox', { name: 'Link regional matching strengths' }));
  fireEvent.change(screen.getByRole('slider', { name: 'Face skin match amount' }), { target: { value: '60' } });
  fireEvent.click(screen.getByRole('button', { name: 'Apply regional colour strengths' }));
  await waitFor(() => expect(regionalColorMatch.apply).toHaveBeenCalledWith('photo','regions','saved-recipe',{
    face_skin: { ...controls, amount: .6 }, background: { ...controls, amount: .6, whiteBalance: 1.5 },
  }, expect.any(String)));
});
it('explains a detected region without a useful correction and disables its effect controls', async () => {
  vi.mocked(regionalColorMatch.status).mockResolvedValue({ ...report, regions: [{ ...report.regions[0]!, correctionAvailable: false, reason: 'No bounded correction improved this region.' }] });
  render(<RegionalColorMatchPanel {...props}/>); open();
  await screen.findByText('No bounded correction improved this region.');
  expect((screen.getByRole('slider', { name: 'Face skin match amount' }).closest('fieldset') as HTMLFieldSetElement).disabled).toBe(true);
  expect((screen.getByRole('button', { name: 'Apply regional colour strengths' }) as HTMLButtonElement).disabled).toBe(true);
});
it('applies independent regional amounts and white balance from the expected saved recipe', async () => {
  render(<RegionalColorMatchPanel {...props}/>); open();
  fireEvent.click(screen.getByRole('button', { name: 'Analyse regional colour match' }));
  await screen.findByRole('slider', { name: 'Face skin match amount' });
  fireEvent.change(screen.getByRole('slider', { name: 'Face skin match amount' }), { target: { value: '0' } });
  fireEvent.change(screen.getByRole('slider', { name: 'Background match white balance' }), { target: { value: '150' } });
  expect(regionalColorMatch.apply).not.toHaveBeenCalled();
  fireEvent.click(screen.getByRole('button', { name: 'Apply regional colour strengths' }));
  await waitFor(() => expect(regionalColorMatch.apply).toHaveBeenCalledWith('photo', 'regions', 'saved-recipe', {
    face_skin: { ...controls, amount: 0 }, background: { ...controls, whiteBalance: 1.5 },
  }, expect.any(String)));
  expect(onBusyChange).toHaveBeenCalledWith(true);
  expect(onBusyChange).toHaveBeenLastCalledWith(false);
  expect(screen.getByText(/Checked on native-resolution renders/)).toBeTruthy();
});
it('shows unavailable regions and refuses stale or empty analyses instead of offering fake selections', async () => {
  vi.mocked(regionalColorMatch.status).mockResolvedValue({ ...report, current: false });
  const view = render(<RegionalColorMatchPanel {...props}/>); open();
  await screen.findByText(/This photo changed/);
  expect((screen.getByRole('button', { name: 'Apply regional colour strengths' }) as HTMLButtonElement).disabled).toBe(true);
  fireEvent.click(screen.getByText('Unavailable regions (1)'));
  expect(screen.getByText(/No shared supported hair selection/)).toBeTruthy();
  expect(screen.queryByRole('slider', { name: 'Hair match amount' })).toBeNull();
  vi.mocked(regionalColorMatch.prepare).mockResolvedValue({ ...report, regions: [] });
  fireEvent.click(screen.getByRole('button', { name: 'Analyse regional colour match' }));
  await screen.findByText(/No shared supported regions were found/);
  expect((screen.getByRole('button', { name: 'Apply regional colour strengths' }) as HTMLButtonElement).disabled).toBe(true);
  view.unmount();
});
it('reports native refusal even when the editor handles the write error and allows a fresh read', async () => {
  vi.mocked(regionalColorMatch.status).mockResolvedValue(report);
  vi.mocked(regionalColorMatch.apply).mockRejectedValue(new Error('The saved selection changed. Analyse again.'));
  render(<RegionalColorMatchPanel {...props} onApply={async action => { try { await action(); } catch { /* The editor shows its own error. */ } }}/>); open();
  await screen.findByRole('slider', { name: 'Face skin match amount' });
  fireEvent.click(screen.getByRole('button', { name: 'Apply regional colour strengths' }));
  await screen.findByRole('alert');
  fireEvent.click(screen.getByRole('button', { name: 'Reload regional match' }));
  await waitFor(() => expect(regionalColorMatch.status).toHaveBeenCalledTimes(2));
});
it('cancels analysis on navigation and ignores an old-photo completion', async () => {
  let complete!: (value: RegionalMatchReport) => void;
  vi.mocked(regionalColorMatch.prepare).mockReturnValue(new Promise(resolve => { complete = resolve; }));
  const view = render(<RegionalColorMatchPanel {...props}/>); open();
  fireEvent.click(screen.getByRole('button', { name: 'Analyse regional colour match' }));
  await waitFor(() => expect(regionalColorMatch.prepare).toHaveBeenCalledOnce());
  const cancelId = vi.mocked(regionalColorMatch.prepare).mock.calls[0]?.[2];
  view.unmount();
  expect(api.cancelJob).toHaveBeenCalledWith(cancelId);
  await act(async () => complete(report));
  expect(regionalColorMatch.apply).not.toHaveBeenCalled();
});
