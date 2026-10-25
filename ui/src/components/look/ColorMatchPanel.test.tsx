import { act, fireEvent, render, screen, waitFor } from '@testing-library/react';
import { beforeEach, expect, it, vi } from 'vitest';
import { ColorMatchPanel } from './ColorMatchPanel';
import { colorMatch, pickColorReference, pickColorReferences, type ColorMatchReport } from '../../ipc/colorMatch';
import { api } from '../../ipc/client';
vi.mock('./RegionalColorMatchPanel', () => ({ RegionalColorMatchPanel: () => null }));
vi.mock('./ReferenceLibraryManager', () => ({ ReferenceLibraryManager: () => null }));

vi.mock('../../ipc/client', () => ({ inTauri: () => true, api: { cancelJob: vi.fn(), listImages: vi.fn() },
  // The real fallback: only a backend error or a Refusal reaches the photographer as written.
  asIpcError: (error: unknown) => (typeof error === 'object' && error !== null && 'runbookUrl' in error ? error : { message: 'AURA hit an unexpected problem and stopped safely.' }) }));
vi.mock('../../ipc/colorMatch', () => ({
  DEFAULT_MATCH_CONTROLS: { amount: 1, tone: 1, color: 1, whiteBalance: 1 }, pickColorReference: vi.fn(), pickColorReferences: vi.fn(),
  colorMatch: { list: vi.fn(), status: vi.fn(), import: vi.fn(), prepare: vi.fn(), apply: vi.fn(), manage:vi.fn(), capture:vi.fn(), generatePreset:vi.fn() },
}));
// A refusal as the backend sends it: a typed error, not a JavaScript exception.
const backendError = (message: string) => ({ code: 'AURA-ML-5070', message, runbookUrl: 'https://aura.app/e/AURA-ML-5070', retryable: false });
const reference = { id: 'ref', name: 'My wedding.tiff', width: 4480, height: 6720, convertedIcc: true, regions: ['whole image', 'face skin'] };
const analysis: ColorMatchReport = { sessionId: 'session', photoId: 'photo', reference,
  controls: { amount: 1, tone: 1, color: 1, whiteBalance: 1 }, matchedRegions: reference.regions,
  beforeDistance: 8, afterDistance: 3, changed: 0, protectedFields: 0, fullResolutionChecked: false, current: true };
const onApply = async (action: () => Promise<unknown>) => { await action(); };
it('puts Lightroom export last and disables generation for unsaved strengths', async () => {
  vi.mocked(colorMatch.status).mockResolvedValue({ ...analysis, fullResolutionChecked: true });
  render(<ColorMatchPanel photoId="photo" disabled={false} onApply={onApply}/>);
  await waitFor(() => expect(screen.getByRole('button', { name: 'Generate .xmp for Lightroom' }).hasAttribute('disabled')).toBe(false));
  expect(screen.getByRole('region', { name: 'Reference colour matching' }).lastElementChild?.getAttribute('aria-label')).toBe('Lightroom preset export');
  fireEvent.change(screen.getByRole('slider', { name: 'Match amount' }), { target: { value: '75' } });
  expect(screen.getByRole('button', { name: 'Generate .xmp for Lightroom' }).hasAttribute('disabled')).toBe(true);
});
beforeEach(() => {
  vi.clearAllMocks();
  vi.mocked(api.cancelJob).mockResolvedValue(true);
  vi.mocked(api.listImages).mockResolvedValue([]);
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

it('shows separately verified tone and palette differences for the saved strengths', async () => {
  const distance = (tone: number, palette: number) => ({ tone, palette, cast: 1, chroma: 1, hue: 1, total: 3 });
  vi.mocked(colorMatch.status).mockResolvedValue({ ...analysis, fullResolutionChecked: true, fitting: {
    engine: 'reference-palette-transport-v8', intent: 'palette', refinementCandidates: 4, uniqueRenders: 3, cachedProposals: 0,
    transfer: { renderedBefore: distance(8, 6), renderedAfter: distance(4, 2) },
  } });
  render(<ColorMatchPanel photoId="photo" disabled={false} onApply={onApply}/>);
  const components = await screen.findByTestId('colour-match-components');
  expect(components.textContent).toContain('Tone difference: 8.00 → 4.00');
  expect(components.textContent).toContain('Colour palette difference: 6.00 → 2.00');
});
it('says what tone the reference look implies and when the photo was finished first', async () => {
  const distance = { tone: 5, palette: 0, cast: 1, chroma: 1, hue: 1, total: 2 };
  vi.mocked(colorMatch.status).mockResolvedValue({ ...analysis, fullResolutionChecked: true, fitting: {
    engine: 'reference-grade-transfer-v4-scene-tone', intent: 'natural', refinementCandidates: 4, uniqueRenders: 3,
    cachedProposals: 0, nativeCandidateChecked: true, nativeBacktrackFactor: 0.75,
    transfer: { renderedBefore: distance, renderedAfter: distance,
      toneEstimate: { lookBlack: 0.12, lookWhite: 0.94, ownBlack: 0.14, ownWhite: 0.9 } },
  } });
  render(<ColorMatchPanel photoId="photo" disabled={false} onApply={onApply}/>);
  const tone = await screen.findByTestId('colour-match-tone');
  expect(tone.textContent).toContain('lifts blacks by about 12%');
  expect(tone.textContent).toContain('given a finished range first');
  expect(screen.getByText(/limited the fitted look to 75%/)).toBeTruthy();
});
it('matches the newly selected reference with a fresh analysis and native apply in one click', async () => {
  const warm = { ...reference, id: 'warm', name: 'Warm portrait.jpg' };
  vi.mocked(colorMatch.list).mockResolvedValue([reference, warm]);
  vi.mocked(colorMatch.status).mockResolvedValue({ ...analysis, fullResolutionChecked: true });
  vi.mocked(colorMatch.prepare).mockResolvedValue({ ...analysis, sessionId: 'warm-session', reference: warm });
  vi.mocked(colorMatch.apply).mockImplementation(async (_photo,_session,controls) => ({
    ...analysis, reference: warm, sessionId: 'warm-session', controls, fullResolutionChecked: true, changed: 1,
  }));
  render(<ColorMatchPanel photoId="photo" disabled={false} onApply={onApply}/>);
  await screen.findByRole('option', { name: warm.name });
  fireEvent.change(screen.getByLabelText('Saved colour reference'), { target: { value: 'warm' } });
  expect(screen.getByText(/selected reference has not been applied/)).toBeTruthy();
  expect(colorMatch.apply).not.toHaveBeenCalled();
  fireEvent.click(screen.getByRole('button', { name: 'Match reference to this photo' }));
  await screen.findByText('Matched Warm portrait.jpg to this photo. Use Undo to compare.');
  expect(colorMatch.prepare).toHaveBeenCalledWith('photo','warm',expect.any(String),'precise','natural');
  expect(colorMatch.apply).toHaveBeenCalledWith('photo','warm-session',analysis.controls,expect.any(String));
  expect(screen.queryByText(/selected reference has not been applied/)).toBeNull();
});
it('never applies a one-click analysis returned for another reference', async () => {
  vi.mocked(colorMatch.prepare).mockResolvedValue({ ...analysis, reference: { ...reference, id: 'wrong' } });
  render(<ColorMatchPanel photoId="photo" disabled={false} onApply={onApply}/>);
  await screen.findByRole('option', { name: reference.name });
  fireEvent.click(screen.getByRole('button', { name: 'Match reference to this photo' }));
  await screen.findByText('The reference or photo changed. Match this photo again.');
  expect(colorMatch.apply).not.toHaveBeenCalled();
});
it('ends the matching status when the selected reference is refused', async () => {
  vi.mocked(colorMatch.prepare).mockRejectedValue(backendError('No safe colour fit. Your edit was kept.'));
  render(<ColorMatchPanel photoId="photo" disabled={false} onApply={onApply}/>);
  await screen.findByRole('option', { name: reference.name });
  fireEvent.click(screen.getByRole('button', { name: 'Match reference to this photo' }));
  expect(await screen.findByRole('alert')).toHaveProperty('textContent', 'No safe colour fit. Your edit was kept.');
  expect(screen.queryByText(/Matching the selected reference/)).toBeNull();
  expect((screen.getByRole('button', { name: 'Match reference to this photo' }) as HTMLButtonElement).disabled).toBe(false);
  expect(colorMatch.apply).not.toHaveBeenCalled();
});
it('ends the matching status when the editor refuses the save (deep assessment, target 11)', async () => {
  // The editor's write wrapper catches a native refusal, shows its own alert and resolves to
  // the reason. The panel must end in a refusal of its own, not keep "Matching..." on screen.
  const refusing = async (action: () => Promise<unknown>) => {
    try { await action(); return null; } catch (cause) { return (cause as Error).message; }
  };
  vi.mocked(colorMatch.apply).mockRejectedValue(backendError('This strength failed the full-resolution check. No edit was saved.'));
  render(<ColorMatchPanel photoId="photo" disabled={false} onApply={refusing}/>);
  await screen.findByRole('option', { name: reference.name });
  fireEvent.click(screen.getByRole('button', { name: 'Match reference to this photo' }));
  expect(await screen.findByRole('alert')).toHaveProperty('textContent', 'This strength failed the full-resolution check. No edit was saved.');
  expect(screen.queryByText(/Matching the selected reference/)).toBeNull();
  expect(screen.queryByText(/Matched .* to this photo/)).toBeNull();
  expect((screen.getByRole('button', { name: 'Match reference to this photo' }) as HTMLButtonElement).disabled).toBe(false);
});
it('says the look was learned from the original and leaves only Amount, up to 100%', async () => {
  const distance = { tone: 2, palette: 0, cast: 1, chroma: 1, hue: 1, total: 2 };
  vi.mocked(colorMatch.status).mockResolvedValue({ ...analysis, fullResolutionChecked: true, fitting: {
    engine: 'reference-original-calibrated-v1', intent: 'natural', refinementCandidates: 0, uniqueRenders: 2,
    cachedProposals: 0, nativeCandidateChecked: true, nativeBacktrackFactor: 1,
    transfer: { renderedBefore: distance, renderedAfter: distance, learnedFrom: '989A2698.CR3' },
  } });
  render(<ColorMatchPanel photoId="photo" disabled={false} onApply={onApply}/>);
  expect((await screen.findByTestId('colour-match-learned')).textContent).toContain('your own edit of 989A2698.CR3');
  const amount = screen.getByRole('slider', { name: 'Match amount' }) as HTMLInputElement;
  expect(amount.disabled).toBe(false);
  expect(amount.max).toBe('100');
  for (const name of ['Tone match', 'Colour match', 'Warmth match']) {
    expect((screen.getByRole('slider', { name }) as HTMLInputElement).disabled).toBe(true);
  }
});
it('explains when a photo looked like the original but could not be used', async () => {
  const distance = { tone: 2, palette: 0, cast: 1, chroma: 1, hue: 1, total: 2 };
  vi.mocked(colorMatch.status).mockResolvedValue({ ...analysis, fullResolutionChecked: true, fitting: {
    engine: 'reference-grade-transfer-v4-scene-tone', intent: 'natural', refinementCandidates: 1, uniqueRenders: 2, cachedProposals: 0,
    transfer: { renderedBefore: distance, renderedAfter: distance,
      originalRefused: 'DSC0001.png looks like the original, but the edited photo is cropped or rotated, so its pixels do not line up.' },
  } });
  render(<ColorMatchPanel photoId="photo" disabled={false} onApply={onApply}/>);
  expect((await screen.findByTestId('colour-match-original-refused')).textContent).toContain('estimated from the edited photo alone');
  expect((screen.getByRole('slider', { name: 'Tone match' }) as HTMLInputElement).disabled).toBe(false);
});
it('never calls a result a match when the photo looks the same as before', async () => {
  vi.mocked(colorMatch.apply).mockImplementation(async (_photo, _session, controls) => ({
    ...analysis, controls, changed: 1, fullResolutionChecked: true, afterDistance: 2, visibleChange: 0.4,
  }));
  render(<ColorMatchPanel photoId="photo" disabled={false} onApply={onApply}/>);
  await screen.findByRole('option', { name: reference.name });
  fireEvent.click(screen.getByRole('button', { name: 'Match reference to this photo' }));
  expect(await screen.findByText(/looks almost the same as before \(difference 0\.4/)).toBeTruthy();
  expect(screen.queryByText(/Matched .* to this photo/)).toBeNull();
  expect(screen.getByTestId('colour-match-visible').textContent).toContain('0.4');
});
it('reports a visible match as a match, with how much it changed', async () => {
  vi.mocked(colorMatch.apply).mockImplementation(async (_photo, _session, controls) => ({
    ...analysis, controls, changed: 1, fullResolutionChecked: true, afterDistance: 2, visibleChange: 6.2,
  }));
  render(<ColorMatchPanel photoId="photo" disabled={false} onApply={onApply}/>);
  await screen.findByRole('option', { name: reference.name });
  fireEvent.click(screen.getByRole('button', { name: 'Match reference to this photo' }));
  await screen.findByText(`Matched ${reference.name} to this photo. Use Undo to compare.`);
  expect(screen.getByTestId('colour-match-visible').textContent).toContain('6.2');
});
it('treats an edit that is still saving as nothing saved', async () => {
  const busy = async () => 'Another edit is still being saved. No edit was saved; try again when it finishes.';
  render(<ColorMatchPanel photoId="photo" disabled={false} onApply={busy}/>);
  await screen.findByRole('option', { name: reference.name });
  fireEvent.click(screen.getByRole('button', { name: 'Match reference to this photo' }));
  expect((await screen.findByRole('alert')).textContent).toContain('Another edit is still being saved');
  expect(screen.queryByText(/Matching the selected reference/)).toBeNull();
});
it('does not report a closer match when native safety retained an unchanged endpoint', async () => {
  vi.mocked(colorMatch.apply).mockResolvedValue({ ...analysis, afterDistance: analysis.beforeDistance, changed: 0, fullResolutionChecked: true });
  render(<ColorMatchPanel photoId="photo" disabled={false} onApply={onApply}/>);
  await screen.findByRole('option', { name: reference.name });
  fireEvent.click(screen.getByRole('button', { name: 'Match reference to this photo' }));
  await screen.findByText('No closer colour match passed this photo’s safety checks. No stronger match was saved.');
  expect(screen.queryByText(/Matched My wedding/)).toBeNull();
});
it('refuses to apply a stale analysis and restores saved strengths', async () => {
  vi.mocked(colorMatch.status).mockResolvedValue({ ...analysis, current: false, controls: { ...analysis.controls, amount: 0.4 } });
  render(<ColorMatchPanel photoId="photo" disabled={false} onApply={onApply}/>);
  await screen.findByText(/analysis is no longer current/);
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

it('requires a new analysis when changing from natural look to reference palette and tone', async () => {
  vi.mocked(colorMatch.status).mockResolvedValue(analysis);
  vi.mocked(colorMatch.prepare).mockResolvedValue({ ...analysis, fitting: {
    engine: 'reference-palette-transport-v8', quality: 'precise', intent: 'palette',
    refinementCandidates: 4, uniqueRenders: 3, cachedProposals: 0,
  } });
  render(<ColorMatchPanel photoId="photo" disabled={false} onApply={onApply}/>);
  await screen.findByTestId('colour-match-measurement');
  fireEvent.change(screen.getByRole('combobox', { name: 'Matching goal' }), { target: { value: 'palette' } });
  expect((screen.getByRole('button', { name: 'Apply colour strengths' }) as HTMLButtonElement).disabled).toBe(true);
  expect(colorMatch.apply).not.toHaveBeenCalled();
  fireEvent.click(screen.getByRole('button', { name: 'Analyse reference match' }));
  await waitFor(() => expect(colorMatch.prepare).toHaveBeenCalledWith('photo', 'ref', expect.any(String), 'precise', 'palette'));
  await waitFor(() => expect((screen.getByRole('button', { name: 'Apply colour strengths' }) as HTMLButtonElement).disabled).toBe(false));
});

it.each(['reference-palette-transport-v4', 'reference-palette-transport-v5', 'reference-palette-transport-v6', 'reference-palette-transport-v7'])('refits saved %s sessions with continuous palette transport', async engine => {
  const legacy = { ...analysis, fitting: {
    engine, quality: 'precise' as const, intent: 'palette' as const,
    refinementCandidates: 4, uniqueRenders: 3, cachedProposals: 0,
  } };
  vi.mocked(colorMatch.status).mockResolvedValue(legacy);
  vi.mocked(colorMatch.prepare).mockResolvedValue({ ...legacy, sessionId: 'guarded',
    fitting: { ...legacy.fitting, engine: 'reference-palette-transport-v8' } });
  render(<ColorMatchPanel photoId="photo" disabled={false} onApply={onApply}/>);
  await screen.findByTestId('colour-match-measurement');
  fireEvent.click(screen.getByRole('checkbox', { name: 'Real-Time Color Adjustments' }));
  await waitFor(() => expect(colorMatch.apply).toHaveBeenCalledWith('photo', 'guarded', analysis.controls, expect.any(String)));
  expect(colorMatch.prepare).toHaveBeenCalledWith('photo', 'ref', expect.any(String), 'precise', 'palette');
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
    .mockRejectedValueOnce(backendError('Cannot decode this reference.')).mockResolvedValueOnce({ ...reference,id:'b',name:'B.png' });
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

it('opens with live matching off and never edits just because a reference is displayed', async () => {
  const selected = vi.fn();
  render(<ColorMatchPanel photoId="photo" disabled={false} onApply={onApply} onReferenceChange={selected}/>);
  await screen.findByRole('option', { name: reference.name });
  expect((screen.getByRole('checkbox', { name: 'Real-Time Color Adjustments' }) as HTMLInputElement).checked).toBe(false);
  expect(screen.getByText('Recommended from your library: shared measured regions, then favorites.')).toBeTruthy();
  expect(selected).toHaveBeenLastCalledWith(reference);
  await act(async () => { await new Promise(resolve => setTimeout(resolve, 420)); });
  expect(colorMatch.prepare).not.toHaveBeenCalled();
  expect(colorMatch.apply).not.toHaveBeenCalled();
});

it('debounces live changes and saves the newest strengths from one immutable analysis', async () => {
  render(<ColorMatchPanel photoId="photo" disabled={false} onApply={onApply}/>);
  await screen.findByRole('option', { name: reference.name });
  fireEvent.click(screen.getByRole('checkbox', { name: 'Real-Time Color Adjustments' }));
  fireEvent.change(screen.getByRole('slider', { name: 'Match amount' }), { target: { value: '20' } });
  fireEvent.change(screen.getByRole('slider', { name: 'Match amount' }), { target: { value: '80' } });
  await waitFor(() => expect(colorMatch.apply).toHaveBeenCalledOnce());
  expect(colorMatch.prepare).toHaveBeenCalledOnce();
  expect(colorMatch.prepare).toHaveBeenCalledWith('photo', 'ref', expect.any(String), 'precise');
  expect(colorMatch.apply).toHaveBeenLastCalledWith('photo', 'session', expect.objectContaining({ amount: 0.8 }), expect.any(String));
  fireEvent.change(screen.getByRole('slider', { name: 'Match amount' }), { target: { value: '40' } });
  await waitFor(() => expect(colorMatch.apply).toHaveBeenCalledTimes(2));
  expect(colorMatch.prepare).toHaveBeenCalledOnce();
  expect(colorMatch.apply).toHaveBeenLastCalledWith('photo', 'session', expect.objectContaining({ amount: 0.4 }), expect.any(String));
});

it('queues strengths during analysis and never applies the superseded request', async () => {
  let finish!: (value: ColorMatchReport) => void;
  vi.mocked(colorMatch.prepare).mockReturnValueOnce(new Promise(resolve => { finish = resolve; }));
  render(<ColorMatchPanel photoId="photo" disabled={false} onApply={onApply}/>);
  await screen.findByRole('option', { name: reference.name });
  fireEvent.click(screen.getByRole('checkbox', { name: 'Real-Time Color Adjustments' }));
  await waitFor(() => expect(colorMatch.prepare).toHaveBeenCalledOnce());
  fireEvent.change(screen.getByRole('slider', { name: 'Match amount' }), { target: { value: '35' } });
  expect(colorMatch.apply).not.toHaveBeenCalled();
  await act(async () => finish(analysis));
  await waitFor(() => expect(colorMatch.apply).toHaveBeenCalledOnce());
  expect(colorMatch.apply).toHaveBeenCalledWith('photo', 'session', expect.objectContaining({ amount: 0.35 }), expect.any(String));
  expect(colorMatch.prepare).toHaveBeenCalledOnce();
});

it('serializes writes while the parent locks its own live save and leaves Stop usable', async () => {
  let finish!: (value: ColorMatchReport) => void;
  vi.mocked(colorMatch.status).mockResolvedValue(analysis);
  vi.mocked(colorMatch.apply).mockReturnValueOnce(new Promise(resolve => { finish = resolve; }));
  const props = { photoId: 'photo', disabled: false, onApply };
  const view = render(<ColorMatchPanel {...props}/>);
  await screen.findByTestId('colour-match-measurement');
  fireEvent.click(screen.getByRole('checkbox', { name: 'Real-Time Color Adjustments' }));
  await waitFor(() => expect(colorMatch.apply).toHaveBeenCalledOnce());
  view.rerender(<ColorMatchPanel {...props} disabled/>);
  const stop = screen.getByRole('button', { name: 'Stop colour matching' }) as HTMLButtonElement;
  expect(stop.disabled).toBe(false);
  expect(stop.closest('fieldset')).toBeNull();
  fireEvent.change(screen.getByRole('slider', { name: 'Match amount' }), { target: { value: '25' } });
  await act(async () => { await new Promise(resolve => setTimeout(resolve, 420)); });
  expect(colorMatch.apply).toHaveBeenCalledOnce();
  expect(api.cancelJob).not.toHaveBeenCalled();
  await act(async () => finish({ ...analysis, fullResolutionChecked: true }));
  view.rerender(<ColorMatchPanel {...props}/>);
  await waitFor(() => expect(colorMatch.apply).toHaveBeenCalledTimes(2));
  expect(colorMatch.apply).toHaveBeenLastCalledWith('photo', 'session', expect.objectContaining({ amount: 0.25 }), expect.any(String));
});

it('cancels hidden live analysis and ignores its late completion', async () => {
  let finish!: (value: ColorMatchReport) => void;
  vi.mocked(colorMatch.prepare).mockReturnValueOnce(new Promise(resolve => { finish = resolve; }));
  const props = { photoId: 'photo', disabled: false, onApply };
  const view = render(<ColorMatchPanel {...props}/>);
  await screen.findByRole('option', { name: reference.name });
  fireEvent.click(screen.getByRole('checkbox', { name: 'Real-Time Color Adjustments' }));
  await waitFor(() => expect(colorMatch.prepare).toHaveBeenCalledOnce());
  view.rerender(<ColorMatchPanel {...props} active={false}/>);
  expect(api.cancelJob).toHaveBeenCalledWith(vi.mocked(colorMatch.prepare).mock.calls[0]?.[2]);
  await act(async () => finish(analysis));
  view.rerender(<ColorMatchPanel {...props}/>);
  await act(async () => { await new Promise(resolve => setTimeout(resolve, 420)); });
  expect(colorMatch.apply).not.toHaveBeenCalled();
  expect((screen.getByRole('checkbox', { name: 'Real-Time Color Adjustments' }) as HTMLInputElement).checked).toBe(false);
});

it('surfaces failed live analysis without writing and allows a later strength change to retry', async () => {
  vi.mocked(colorMatch.prepare).mockRejectedValueOnce(backendError('Native analysis failed'));
  render(<ColorMatchPanel photoId="photo" disabled={false} onApply={onApply}/>);
  await screen.findByRole('option', { name: reference.name });
  fireEvent.click(screen.getByRole('checkbox', { name: 'Real-Time Color Adjustments' }));
  expect((await screen.findByRole('alert')).textContent).toContain('Native analysis failed');
  expect(colorMatch.apply).not.toHaveBeenCalled();
  fireEvent.change(screen.getByRole('slider', { name: 'Match amount' }), { target: { value: '30' } });
  await waitFor(() => expect(colorMatch.apply).toHaveBeenCalledOnce());
});

it('refuses stale sessions in real-time mode instead of silently replacing manual edits', async () => {
  vi.mocked(colorMatch.status).mockResolvedValue({ ...analysis, current: false });
  render(<ColorMatchPanel photoId="photo" disabled={false} onApply={onApply}/>);
  await screen.findByText(/analysis is no longer current/);
  fireEvent.click(screen.getByRole('checkbox', { name: 'Real-Time Color Adjustments' }));
  expect((await screen.findByRole('alert')).textContent).toContain('Analyse the reference again');
  expect(colorMatch.prepare).not.toHaveBeenCalled();
  expect(colorMatch.apply).not.toHaveBeenCalled();
});

it('uses real Quick analysis and persists the authored Extreme strengths as a reference preset', async () => {
  render(<ColorMatchPanel photoId="photo" disabled={false} onApply={onApply}/>);
  await screen.findByRole('option', { name: reference.name });
  fireEvent.change(screen.getByRole('combobox', { name: 'Matching mode' }), { target: { value: 'quick' } });
  fireEvent.click(screen.getByRole('button', { name: 'Analyse reference match' }));
  await screen.findByTestId('colour-match-measurement');
  expect(colorMatch.prepare).toHaveBeenCalledWith('photo', 'ref', expect.any(String), 'quick');
  fireEvent.change(screen.getByRole('combobox', { name: 'Color Match Preferences' }), { target: { value: 'extreme' } });
  expect((screen.getByRole('slider', { name: 'Match amount' }) as HTMLInputElement).value).toBe('200');
  fireEvent.click(screen.getByRole('button', { name: 'Save Preset' }));
  await waitFor(() => expect(colorMatch.manage).toHaveBeenCalledWith('ref', { defaultControls: { amount: 2, tone: 1, color: 1, whiteBalance: 1 } }, 'update'));
  expect(colorMatch.apply).not.toHaveBeenCalled();
});

it('syncs selected other photos with independent fitting and quality forwarding', async () => {
  vi.mocked(api.listImages).mockResolvedValue([{ id: 'photo', fileName: 'Active.jpg' }, { id: 'target', fileName: 'Target.jpg' }, { id: 'unselected', fileName: 'Other.jpg' }] as never);
  vi.mocked(colorMatch.prepare).mockImplementation(async photoId => ({ ...analysis, photoId }));
  vi.mocked(colorMatch.apply).mockImplementation(async (photoId, _session, controls) => ({ ...analysis, photoId, controls, fullResolutionChecked: true }));
  render(<ColorMatchPanel projectId="project" photoId="photo" disabled={false} onApply={onApply} selectedPhotoIds={['photo', 'target']}/>);
  await screen.findByRole('option', { name: reference.name });
  fireEvent.click(screen.getByRole('button', { name: 'Sync' }));
  await waitFor(() => expect(colorMatch.apply).toHaveBeenCalledOnce());
  expect(colorMatch.prepare).toHaveBeenCalledWith('target', 'ref', expect.any(String), 'precise');
  expect(colorMatch.apply).toHaveBeenCalledWith('target', 'session', expect.any(Object), expect.any(String));
  expect(await screen.findByRole('list', { name: 'Collection colour matching results' })).toBeTruthy();
});

it('does not trigger another automatic edit when its saved recipe revision refreshes', async () => {
  const saved = { ...analysis, fullResolutionChecked: true };
  vi.mocked(colorMatch.status).mockResolvedValue(saved);
  const props = { photoId: 'photo', disabled: false, onApply };
  const view = render(<ColorMatchPanel {...props} revision="before"/>);
  await screen.findByTestId('colour-match-measurement');
  fireEvent.click(screen.getByRole('checkbox', { name: 'Real-Time Color Adjustments' }));
  await screen.findByText('Real-time colour strengths saved. Use Undo to compare.');
  view.rerender(<ColorMatchPanel {...props} revision="after"/>);
  await waitFor(() => expect(colorMatch.status).toHaveBeenCalledTimes(2));
  await act(async () => { await new Promise(resolve => setTimeout(resolve, 420)); });
  expect(colorMatch.apply).toHaveBeenCalledOnce();
  expect(colorMatch.prepare).not.toHaveBeenCalled();
});

it('Stop cancels the live job and discards queued strengths and late results', async () => {
  let finish!: (value: ColorMatchReport) => void;
  vi.mocked(colorMatch.status).mockResolvedValue(analysis);
  vi.mocked(colorMatch.apply).mockReturnValueOnce(new Promise(resolve => { finish = resolve; }));
  render(<ColorMatchPanel photoId="photo" disabled={false} onApply={onApply}/>);
  await screen.findByTestId('colour-match-measurement');
  fireEvent.click(screen.getByRole('checkbox', { name: 'Real-Time Color Adjustments' }));
  await waitFor(() => expect(colorMatch.apply).toHaveBeenCalledOnce());
  fireEvent.change(screen.getByRole('slider', { name: 'Match amount' }), { target: { value: '15' } });
  fireEvent.click(screen.getByRole('button', { name: 'Stop colour matching' }));
  expect(api.cancelJob).toHaveBeenCalledWith(vi.mocked(colorMatch.apply).mock.calls[0]?.[3]);
  await act(async () => finish({ ...analysis, controls: { ...analysis.controls, amount: 1.8 }, fullResolutionChecked: true }));
  await act(async () => { await new Promise(resolve => setTimeout(resolve, 420)); });
  expect(colorMatch.apply).toHaveBeenCalledOnce();
  expect((screen.getByRole('slider', { name: 'Match amount' }) as HTMLInputElement).value).toBe('15');
  expect((screen.getByRole('checkbox', { name: 'Real-Time Color Adjustments' }) as HTMLInputElement).checked).toBe(false);
});

it('refuses incomplete native verification instead of reporting a saved live match', async () => {
  vi.mocked(colorMatch.status).mockResolvedValue(analysis);
  vi.mocked(colorMatch.apply).mockResolvedValue({ ...analysis, fullResolutionChecked: false });
  render(<ColorMatchPanel photoId="photo" disabled={false} onApply={onApply}/>);
  await screen.findByTestId('colour-match-measurement');
  fireEvent.click(screen.getByRole('checkbox', { name: 'Real-Time Color Adjustments' }));
  expect((await screen.findByRole('alert')).textContent).toContain('no current native-resolution colour check');
  expect(screen.queryByText('Real-time colour strengths saved. Use Undo to compare.')).toBeNull();
});

it('waits for an external editor lock before applying completed live analysis', async () => {
  let finish!: (value: ColorMatchReport) => void;
  vi.mocked(colorMatch.prepare).mockReturnValueOnce(new Promise(resolve => { finish = resolve; }));
  const write = vi.fn(async (action: () => Promise<unknown>) => { await action(); });
  const props = { photoId: 'photo', disabled: false, onApply: write };
  const view = render(<ColorMatchPanel {...props}/>);
  await screen.findByRole('option', { name: reference.name });
  fireEvent.click(screen.getByRole('checkbox', { name: 'Real-Time Color Adjustments' }));
  await waitFor(() => expect(colorMatch.prepare).toHaveBeenCalledOnce());
  view.rerender(<ColorMatchPanel {...props} disabled/>);
  expect((screen.getByRole('slider', { name: 'Match amount' }).closest('fieldset') as HTMLFieldSetElement).disabled).toBe(true);
  await act(async () => finish(analysis));
  await act(async () => { await new Promise(resolve => setTimeout(resolve, 420)); });
  expect(write).not.toHaveBeenCalled();
  expect(colorMatch.apply).not.toHaveBeenCalled();
  view.rerender(<ColorMatchPanel {...props}/>);
  await waitFor(() => expect(colorMatch.apply).toHaveBeenCalledOnce());
  expect(write).toHaveBeenCalledOnce();
  expect(colorMatch.prepare).toHaveBeenCalledOnce();
  expect(colorMatch.apply).toHaveBeenCalledWith('photo', 'session', analysis.controls, expect.any(String));
});
