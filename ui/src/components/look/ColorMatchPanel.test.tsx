import { act, fireEvent, render, screen, waitFor } from '@testing-library/react';
import { beforeEach, expect, it, vi } from 'vitest';
import { ColorMatchPanel } from './ColorMatchPanel';
import { colorMatch, pickColorReference, pickColorReferences, type ColorMatchReport } from '../../ipc/colorMatch';
import { api } from '../../ipc/client';
vi.mock('./RegionalColorMatchPanel', () => ({ RegionalColorMatchPanel: () => null }));

vi.mock('../../ipc/client', () => ({ inTauri: () => true, api: { cancelJob: vi.fn() },
  asIpcError: (error: Error) => ({ message: error.message }) }));
vi.mock('../../ipc/colorMatch', () => ({
  DEFAULT_MATCH_CONTROLS: { amount: 1, tone: 1, color: 1, whiteBalance: 1 }, pickColorReference: vi.fn(), pickColorReferences: vi.fn(),
  colorMatch: { list: vi.fn(), status: vi.fn(), import: vi.fn(), prepare: vi.fn(), apply: vi.fn(), manage:vi.fn(), capture:vi.fn() },
}));
const reference = { id: 'ref', name: 'My wedding.tiff', width: 4480, height: 6720, convertedIcc: true, regions: ['whole image', 'face skin'] };
const analysis: ColorMatchReport = { sessionId: 'session', photoId: 'photo', reference,
  controls: { amount: 1, tone: 1, color: 1, whiteBalance: 1 }, matchedRegions: reference.regions,
  beforeDistance: 8, afterDistance: 3, changed: 0, protectedFields: 0, fullResolutionChecked: false, current: true };
const onApply = async (action: () => Promise<unknown>) => { await action(); };
beforeEach(() => {
  vi.clearAllMocks();
  vi.mocked(api.cancelJob).mockResolvedValue(true);
  vi.mocked(colorMatch.list).mockResolvedValue([reference]);
  vi.mocked(colorMatch.status).mockResolvedValue(null);
  vi.mocked(colorMatch.prepare).mockResolvedValue(analysis);
  vi.mocked(colorMatch.apply).mockImplementation(async (_photo, _session, controls) => ({ ...analysis, controls, changed: 4, fullResolutionChecked: true }));
  vi.mocked(pickColorReference).mockResolvedValue(null);
  vi.mocked(pickColorReferences).mockResolvedValue([]);
  vi.mocked(colorMatch.import).mockResolvedValue(reference);
  vi.mocked(colorMatch.capture).mockResolvedValue({ ...reference, id:'captured', name:'Studio finish' });
  vi.mocked(colorMatch.manage).mockImplementation(async (_id, metadata, action) => action === 'remove' ? null : { ...reference, ...metadata });
});
it('keeps a saved match adjustable after its reference leaves the reusable library', async () => {
  vi.mocked(colorMatch.list).mockResolvedValue([]);
  vi.mocked(colorMatch.status).mockResolvedValue(analysis);
  render(<ColorMatchPanel projectId="project" photoId="photo" disabled={false} onApply={onApply}/>);
  await screen.findByRole('option', { name: `${reference.name} (saved match only)` });
  expect((screen.getByRole('button', { name: 'Analyse reference match' }) as HTMLButtonElement).disabled).toBe(true);
  fireEvent.change(screen.getByRole('slider', { name: 'Match amount' }), { target: { value: '40' } });
  fireEvent.click(screen.getByRole('button', { name: 'Apply colour strengths' }));
  await waitFor(() => expect(colorMatch.apply).toHaveBeenCalledWith('photo', 'session', expect.objectContaining({ amount: 0.4 }), expect.any(String)));
  fireEvent.click(screen.getByText('Match this collection'));
  expect((screen.getByRole('button', { name: 'Match colour across this collection' }) as HTMLButtonElement).disabled).toBe(true);
});
it('sends independent slider strengths and uses an explicit native application', async () => {
  render(<ColorMatchPanel photoId="photo" disabled={false} onApply={onApply}/>);
  await screen.findByRole('option', { name: reference.name });
  fireEvent.click(screen.getByRole('button', { name: 'Analyse reference match' }));
  await screen.findByTestId('colour-match-measurement');
  fireEvent.change(screen.getByRole('slider', { name: 'Match amount' }), { target: { value: '70' } });
  fireEvent.change(screen.getByRole('slider', { name: 'Tone match' }), { target: { value: '20' } });
  fireEvent.change(screen.getByRole('slider', { name: 'Colour match' }), { target: { value: '80' } });
  fireEvent.change(screen.getByRole('slider', { name: 'Warmth match' }), { target: { value: '0' } });
  expect(colorMatch.apply).not.toHaveBeenCalled();
  fireEvent.click(screen.getByRole('button', { name: 'Apply colour strengths' }));
  await screen.findByText(/Colour strengths saved/);
  expect(colorMatch.apply).toHaveBeenCalledWith('photo', 'session', { amount: 0.7, tone: 0.2, color: 0.8, whiteBalance: 0 }, expect.any(String));
  expect(screen.getByTestId('colour-match-measurement').textContent).toContain('native-resolution');
  fireEvent.click(screen.getByRole('button', { name: 'Remove matched treatment' }));
  await waitFor(() => expect(colorMatch.apply).toHaveBeenLastCalledWith('photo', 'session', { amount: 0, tone: 0.2, color: 0.8, whiteBalance: 0 }, expect.any(String)));
});
it('refuses to apply a stale analysis and restores saved strengths', async () => {
  vi.mocked(colorMatch.status).mockResolvedValue({ ...analysis, current: false, controls: { ...analysis.controls, amount: 0.4 } });
  render(<ColorMatchPanel photoId="photo" disabled={false} onApply={onApply}/>);
  await screen.findByText(/photo changed after analysis/);
  expect((screen.getByRole('slider', { name: 'Match amount' }) as HTMLInputElement).value).toBe('40');
  expect((screen.getByRole('button', { name: 'Apply colour strengths' }).closest('fieldset') as HTMLFieldSetElement).disabled).toBe(true);
  expect(colorMatch.apply).not.toHaveBeenCalled();
});
it('cancels active analysis on navigation and does not attach its result to another photo', async () => {
  let finish!: (value: ColorMatchReport) => void;
  vi.mocked(colorMatch.prepare).mockReturnValue(new Promise(resolve => { finish = resolve; }));
  const view = render(<ColorMatchPanel photoId="photo" disabled={false} onApply={onApply}/>);
  await screen.findByRole('option', { name: reference.name });
  fireEvent.click(screen.getByRole('button', { name: 'Analyse reference match' }));
  await waitFor(() => expect(colorMatch.prepare).toHaveBeenCalledOnce());
  view.unmount();
  expect(api.cancelJob).toHaveBeenCalledWith(vi.mocked(colorMatch.prepare).mock.calls[0]?.[2]);
  await act(async () => finish(analysis));
  expect(colorMatch.apply).not.toHaveBeenCalled();
});
it('does not import or fit when the reference picker is cancelled', async () => {
  render(<ColorMatchPanel photoId="photo" disabled={false} onApply={onApply}/>);
  await screen.findByRole('option', { name: reference.name });
  fireEvent.click(screen.getByRole('button', { name: 'Upload colour reference' }));
  await waitFor(() => expect(pickColorReference).toHaveBeenCalledOnce());
  expect(colorMatch.import).not.toHaveBeenCalled();
  expect(colorMatch.prepare).not.toHaveBeenCalled();
});

it('applies fine exposure, temperature and tint strengths independently', async () => {
  render(<ColorMatchPanel photoId="photo" disabled={false} onApply={onApply}/>);
  await screen.findByRole('option', { name:reference.name });
  fireEvent.click(screen.getByRole('button', { name:'Analyse reference match' }));
  await screen.findByTestId('colour-match-measurement');
  fireEvent.click(screen.getByText('Fine matching controls'));
  fireEvent.change(screen.getByRole('slider', { name:'Exposure match' }), { target:{ value:'20' } });
  fireEvent.change(screen.getByRole('slider', { name:'Temperature match' }), { target:{ value:'140' } });
  fireEvent.change(screen.getByRole('slider', { name:'Tint match' }), { target:{ value:'0' } });
  fireEvent.click(screen.getByRole('button', { name:'Apply colour strengths' }));
  await waitFor(() => expect(colorMatch.apply).toHaveBeenCalledWith('photo','session',expect.objectContaining({
    detail:{ exposure:0.2, temperature:1.4, tint:0 },
  }),expect.any(String)));
});

it('creates a reference from the actual selected edited photo', async () => {
  render(<ColorMatchPanel projectId="project" photoId="photo" disabled={false} onApply={onApply}/>);
  await screen.findByRole('option', { name:reference.name });
  fireEvent.click(screen.getByText('Use this edited photo as a reference'));
  fireEvent.change(screen.getByLabelText('Edited reference name'), { target:{ value:'Studio finish' } });
  fireEvent.click(screen.getByRole('button', { name:'Create colour reference from this edit' }));
  await screen.findByRole('option', { name:'Studio finish' });
  expect(colorMatch.capture).toHaveBeenCalledWith('project','photo','Studio finish',expect.any(String));
  expect(colorMatch.apply).not.toHaveBeenCalled();
});

it('renames and groups a reference and removes only its library entry', async () => {
  render(<ColorMatchPanel photoId="photo" disabled={false} onApply={onApply}/>);
  await screen.findByRole('option', { name:reference.name });
  fireEvent.click(screen.getByText('Reference library (1)'));
  fireEvent.change(screen.getByLabelText('Reference name'), { target:{ value:'Warm reception' } });
  fireEvent.change(screen.getByLabelText('Save in group'), { target:{ value:'Wedding' } });
  fireEvent.click(screen.getByRole('button', { name:'Save reference details' }));
  await screen.findByRole('option', { name:'Warm reception' });
  expect(colorMatch.manage).toHaveBeenCalledWith('ref',{ name:'Warm reception', group:'Wedding' },'update');
  fireEvent.click(screen.getByRole('button', { name:'Remove reference from library' }));
  await waitFor(() => expect(screen.queryByRole('option', { name:'Warm reception' })).toBeNull());
  expect(colorMatch.manage).toHaveBeenLastCalledWith('ref',{},'remove');
  expect(colorMatch.apply).not.toHaveBeenCalled();
});

it('keeps valid references when one file in a multiple upload is refused', async () => {
  vi.mocked(pickColorReferences).mockResolvedValue(['a.tiff','broken.jpg','b.png']);
  vi.mocked(colorMatch.import).mockResolvedValueOnce({ ...reference,id:'a',name:'A.tiff' })
    .mockRejectedValueOnce(new Error('Cannot decode this reference.')).mockResolvedValueOnce({ ...reference,id:'b',name:'B.png' });
  render(<ColorMatchPanel photoId="photo" disabled={false} onApply={onApply}/>);
  await screen.findByRole('option', { name:reference.name });
  fireEvent.click(screen.getByRole('button', { name:'Upload several colour references' }));
  await screen.findByText('2 references saved; 1 could not be imported.');
  expect(screen.getByRole('option', { name:'A.tiff' })).toBeTruthy();
  expect(screen.getByRole('option', { name:'B.png' })).toBeTruthy();
  expect(screen.getByRole('alert').textContent).toContain('Cannot decode');
});

it('cancels analysis on photo navigation while the component stays mounted', async () => {
  let finish!: (value:ColorMatchReport) => void;
  vi.mocked(colorMatch.prepare).mockReturnValueOnce(new Promise(resolve => { finish=resolve; }));
  const view=render(<ColorMatchPanel photoId="photo" disabled={false} onApply={onApply}/>);
  await screen.findByRole('option', { name:reference.name });
  fireEvent.click(screen.getByRole('button', { name:'Analyse reference match' }));
  await waitFor(() => expect(colorMatch.prepare).toHaveBeenCalledOnce());
  view.rerender(<ColorMatchPanel photoId="other" disabled={false} onApply={onApply}/>);
  await act(async () => finish(analysis));
  expect(api.cancelJob).toHaveBeenCalled();
  expect(screen.queryByTestId('colour-match-measurement')).toBeNull();
});
