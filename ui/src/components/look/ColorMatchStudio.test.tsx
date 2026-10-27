import { fireEvent, render, screen, waitFor, within } from '@testing-library/react';
import { beforeEach, expect, it, vi } from 'vitest';
import { ColorMatchStudio, describeMatch } from './ColorMatchStudio';
import { colorMatch, oneClickMatch, pickColorReferences, type ColorReference, type QuickMatchReport } from '../../ipc/colorMatch';
import { api } from '../../ipc/client';
import { useStore } from '../../state/store';

vi.mock('./ColorMatchPanel', () => ({ ColorMatchPanel: () => null }));
vi.mock('../../ipc/client', () => ({ inTauri: () => true, api: { cancelJob: vi.fn(), listImages: vi.fn() },
  asIpcError: (error: Error) => ({ message: error.message }) }));
vi.mock('../../ipc/colorMatch', async importOriginal => {
  const real = await importOriginal<typeof import('../../ipc/colorMatch')>();
  return { ...real, pickColorReferences: vi.fn(),
    colorMatch: { list: vi.fn(), status: vi.fn(), import: vi.fn(), capture: vi.fn() },
    oneClickMatch: { quick: vi.fn(), recommend: vi.fn(), preview: vi.fn(), presets: vi.fn() } };
});

const warm: ColorReference = { id: 'warm', name: 'Wedding 02.jpg', width: 800, height: 1000, convertedIcc: false, regions: ['face skin'], thumbnail: 'data:warm' };
const cool: ColorReference = { id: 'cool', name: 'Beach.jpg', width: 1000, height: 800, convertedIcc: false, regions: ['background'] };
const report = (referenceId: string, amount: number, extra: Partial<QuickMatchReport> = {}): QuickMatchReport => ({
  sessionId: `s-${referenceId}`, photoId: 'photo', reference: referenceId === 'warm' ? warm : cool,
  controls: { amount, tone: 1, color: 1, whiteBalance: 1 }, matchedRegions: [], beforeDistance: 9, afterDistance: 3,
  changed: 2, protectedFields: 0, fullResolutionChecked: true, current: true,
  requestedAmount: amount, limited: false, createdBaseline: false, attempts: 1, ...extra });
const onApply = async (action: () => Promise<unknown>) => { await action(); };

beforeEach(() => {
  vi.clearAllMocks();
  localStorage.clear();
  useStore.setState({ activeProjectId: 'project', selection: new Set<string>(), rows: [] });
  vi.mocked(api.cancelJob).mockResolvedValue(true);
  vi.mocked(colorMatch.list).mockResolvedValue([warm, cool]);
  vi.mocked(colorMatch.status).mockResolvedValue(null);
  vi.mocked(oneClickMatch.presets).mockResolvedValue([]);
  vi.mocked(oneClickMatch.recommend).mockResolvedValue([{ referenceId: 'warm', score: 0.9, reasons: ['People in both', 'Similar brightness'] }]);
  vi.mocked(oneClickMatch.quick).mockImplementation(async (photo, referenceId, controls) => ({ ...report(referenceId, controls.amount), photoId: photo }));
});

const mount = (onReferenceChange = vi.fn()) =>
  render(<ColorMatchStudio projectId="project" photoId="photo" disabled={false} onApply={onApply} onReferenceChange={onReferenceChange} />);

it('matches a recommended reference in one click at the chosen preference', async () => {
  const announce = vi.fn();
  mount(announce);
  const card = await screen.findByRole('button', { name: /Wedding 02\.jpg/ });
  expect(card.getAttribute('title')).toContain('People in both');
  fireEvent.click(card);
  await screen.findByText(/Matched to Wedding 02\.jpg · Standard\./);
  expect(oneClickMatch.quick).toHaveBeenCalledWith('photo', 'warm', { amount: 1, tone: 1, color: 1, whiteBalance: 1 }, expect.any(String), false);
  expect(announce).toHaveBeenCalledWith({ id: 'warm', name: 'Wedding 02.jpg', thumbnail: 'data:warm' }, true);
  expect(screen.getByTestId('one-click-measurement').textContent).toContain('9.0 → 3.0');

  fireEvent.change(screen.getByRole('combobox', { name: 'Color match preference' }), { target: { value: 'light' } });
  await waitFor(() => expect(oneClickMatch.quick).toHaveBeenLastCalledWith('photo', 'warm', expect.objectContaining({ amount: 0.5 }), expect.any(String), false));
});

it('says when Extreme was reduced to protect the photo', async () => {
  vi.mocked(oneClickMatch.quick).mockResolvedValue(report('warm', 1.024, { requestedAmount: 1.6, limited: true, attempts: 3 }));
  mount();
  fireEvent.change(await screen.findByRole('combobox', { name: 'Color match preference' }), { target: { value: 'extreme' } });
  fireEvent.click(await screen.findByRole('button', { name: /Wedding 02\.jpg/ }));
  await screen.findByText(/Reduced to 102% of the reference look so highlights and skin stay natural/);
  expect(oneClickMatch.quick).toHaveBeenCalledWith('photo', 'warm', expect.objectContaining({ amount: 1.6 }), expect.any(String), false);
  // The preference still reads Extreme: it is what was asked for.
  expect((screen.getByRole('combobox', { name: 'Color match preference' }) as HTMLSelectElement).value).toBe('extreme');
});

it('waits for Apply match when real-time adjustments are off', async () => {
  mount();
  fireEvent.click(screen.getByRole('switch', { name: 'Real-time color adjustments' }));
  fireEvent.click(screen.getByRole('tab', { name: 'My References' }));
  fireEvent.click(await screen.findByRole('button', { name: /Beach\.jpg/ }));
  expect(oneClickMatch.quick).not.toHaveBeenCalled();
  fireEvent.click(screen.getByRole('button', { name: 'Apply match' }));
  await waitFor(() => expect(oneClickMatch.quick).toHaveBeenCalledWith('photo', 'cool', expect.objectContaining({ amount: 1 }), expect.any(String), false));
  expect(localStorage.getItem('aura.colorMatch.realtime')).toBe('off');
});

it('saves a preset and reuses it in one click', async () => {
  const saved = { id: 'p1', name: 'Golden', referenceId: 'warm', referenceName: 'Wedding 02.jpg', level: 'strong' as const,
    controls: { amount: 1.25, tone: 1, color: 1, whiteBalance: 1 }, createdAt: 'now', available: true };
  vi.mocked(oneClickMatch.presets).mockImplementation(async action => action === 'save' ? [saved] : []);
  mount();
  fireEvent.change(await screen.findByRole('combobox', { name: 'Color match preference' }), { target: { value: 'strong' } });
  fireEvent.click(await screen.findByRole('button', { name: /Wedding 02\.jpg/ }));
  await screen.findByText(/Matched to Wedding 02\.jpg · Strong/);
  fireEvent.click(screen.getByRole('button', { name: 'Save Preset' }));
  fireEvent.change(screen.getByRole('textbox', { name: 'Preset name' }), { target: { value: 'Golden' } });
  fireEvent.click(screen.getByRole('button', { name: 'Save preset' }));
  await screen.findByText('Preset “Golden” saved.');
  expect(oneClickMatch.presets).toHaveBeenCalledWith('save', { name: 'Golden', referenceId: 'warm', level: 'strong',
    controls: { amount: 1.25, tone: 1, color: 1, whiteBalance: 1 } });
  expect(screen.getByRole('tab', { name: 'Presets' }).getAttribute('aria-selected')).toBe('true');
  vi.mocked(oneClickMatch.quick).mockClear();
  fireEvent.click(screen.getByRole('button', { name: 'Use preset Golden' }));
  await waitFor(() => expect(oneClickMatch.quick).toHaveBeenCalledWith('photo', 'warm', saved.controls, expect.any(String), false));
});

it('refuses a preset whose reference was removed', async () => {
  vi.mocked(oneClickMatch.presets).mockResolvedValue([{ id: 'p', name: 'Old', referenceId: 'gone', referenceName: 'Gone.jpg',
    level: 'standard', controls: { amount: 1, tone: 1, color: 1, whiteBalance: 1 }, createdAt: 'x', available: false }]);
  mount();
  fireEvent.click(screen.getByRole('tab', { name: 'Presets' }));
  fireEvent.click(await screen.findByRole('button', { name: 'Use preset Old' }));
  expect((await screen.findByRole('alert')).textContent).toContain('Gone.jpg');
  expect(oneClickMatch.quick).not.toHaveBeenCalled();
});

it('syncs the selected photos, each measured on its own', async () => {
  useStore.setState({ selection: new Set(['photo', 'a', 'b']),
    rows: [{ id: 'a', fileName: 'A.CR3' }, { id: 'b', fileName: 'B.CR3' }] as never });
  vi.mocked(oneClickMatch.quick).mockImplementation(async (photo, referenceId, controls) =>
    photo === 'b' ? { ...report(referenceId, 0.8, { requestedAmount: 1, limited: true }), photoId: photo }
      : { ...report(referenceId, controls.amount), photoId: photo });
  mount();
  fireEvent.click(await screen.findByRole('button', { name: /Wedding 02\.jpg/ }));
  await screen.findByText(/Matched to Wedding 02\.jpg/);
  vi.mocked(oneClickMatch.quick).mockClear();
  fireEvent.click(screen.getByRole('button', { name: 'Sync (2)' }));
  await screen.findByText(/Synced 2 of 2 \(1 reduced\)/);
  expect(vi.mocked(oneClickMatch.quick).mock.calls.map(call => call[0])).toEqual(['a', 'b']);
  const results = within(screen.getByRole('list', { name: 'Sync results' }));
  expect(results.getByText('B.CR3').parentElement?.textContent).toContain('matched, reduced');
});

it('asks for a selection before syncing nothing', async () => {
  mount();
  fireEvent.click(await screen.findByRole('button', { name: /Wedding 02\.jpg/ }));
  await screen.findByText(/Matched to/);
  fireEvent.click(screen.getByRole('button', { name: 'Sync (0)' }));
  await screen.findByText(/Select photos in the library first/);
  fireEvent.click(screen.getByRole('button', { name: 'Sync options' }));
  fireEvent.click(screen.getByRole('radio', { name: 'Whole collection' }));
  vi.mocked(api.listImages).mockResolvedValue([{ id: 'photo', fileName: 'This.CR3' }, { id: 'c', fileName: 'C.CR3' }] as never);
  vi.mocked(oneClickMatch.quick).mockClear();
  fireEvent.click(screen.getByRole('button', { name: 'Sync all' }));
  await screen.findByText(/Synced 1 of 1/);
  expect(vi.mocked(oneClickMatch.quick).mock.calls.map(call => call[0])).toEqual(['c']);
});

it('uploads several references and matches the first', async () => {
  const added = { ...cool, id: 'new', name: 'Studio.png' };
  vi.mocked(pickColorReferences).mockResolvedValue(['C:/a.png', 'C:/b.png']);
  vi.mocked(colorMatch.import).mockImplementationOnce(async () => added).mockRejectedValueOnce(new Error('Too small.'));
  mount();
  await screen.findByRole('button', { name: /Wedding 02\.jpg/ });
  fireEvent.click(screen.getByRole('button', { name: /Upload References/ }));
  await waitFor(() => expect(oneClickMatch.quick).toHaveBeenCalledWith('photo', 'new', expect.objectContaining({ amount: 1 }), expect.any(String), false));
  expect((await screen.findByRole('alert')).textContent).toContain('1 could not be saved: Too small.');
  expect(screen.getByRole('tab', { name: 'My References' }).getAttribute('aria-selected')).toBe('true');
});

it('restores a saved match and announces it without switching views', async () => {
  vi.mocked(colorMatch.status).mockResolvedValue(report('cool', 0.75));
  const announce = vi.fn();
  mount(announce);
  await waitFor(() => expect(announce).toHaveBeenCalledWith({ id: 'cool', name: 'Beach.jpg', thumbnail: undefined }, false));
  expect((screen.getByRole('combobox', { name: 'Color match preference' }) as HTMLSelectElement).value).toBe('natural');
  fireEvent.click(screen.getByRole('button', { name: 'Remove matched look' }));
  await waitFor(() => expect(oneClickMatch.quick).toHaveBeenCalledWith('photo', 'cool', expect.objectContaining({ amount: 0 }), expect.any(String), false));
});

it('opens on Recommended when the remembered Presets tab is empty', async () => {
  localStorage.setItem('aura.colorMatch.tab', 'presets');
  mount();
  await screen.findByRole('button', { name: /Wedding 02\.jpg/ });
  expect(screen.getByRole('tab', { name: 'Recommended' }).getAttribute('aria-selected')).toBe('true');
});

it('names the protection that reduced a match', () => {
  const reduced = (limitedBy?: QuickMatchReport['limitedBy']) =>
    describeMatch(report('warm', 0.4, { requestedAmount: 1, limited: true, limitedBy }));
  expect(reduced('highlights')).toContain('Reduced to 40% of the reference look so highlights keep their detail.');
  expect(reduced('skin')).toContain('so skin stays natural.');
  expect(reduced('distance')).toContain('because more would move the photo away from the reference.');
  expect(reduced()).toContain('so highlights and skin stay natural.');
});

it('says when the cautious version of a look was used', () => {
  expect(describeMatch(report('warm', 1, { cautious: true }))).toContain('lighter, cautious version');
});

it('describes removal, a baseline and a monochrome reference', () => {
  expect(describeMatch(report('warm', 0, { requestedAmount: 0 }))).toContain('Matched look removed');
  const text = describeMatch(report('warm', 1, { createdBaseline: true, referenceMonochrome: true }));
  expect(text).toContain('black and white');
  expect(text).toContain('Undo returns to it');
});

it('keeps showing the requested level when the editor reloads after a reduced match', async () => {
  const reduced = report('warm', 1.024, { requestedAmount: 1.6, limited: true, attempts: 3 });
  vi.mocked(oneClickMatch.quick).mockResolvedValue(reduced);
  const view = render(<ColorMatchStudio projectId="project" photoId="photo" revision="a" disabled={false} onApply={onApply} />);
  fireEvent.change(await screen.findByRole('combobox', { name: 'Color match preference' }), { target: { value: 'extreme' } });
  fireEvent.click(await screen.findByRole('button', { name: /Wedding 02\.jpg/ }));
  await screen.findByText(/Reduced to 102%/);
  // The editor saves, then reloads the panel with the saved (reduced) amount.
  const { requestedAmount: _r, limited: _l, createdBaseline: _c, attempts: _a, ...saved } = reduced;
  vi.mocked(colorMatch.status).mockResolvedValue(saved);
  view.rerender(<ColorMatchStudio projectId="project" photoId="photo" revision="b" disabled={false} onApply={onApply} />);
  await waitFor(() => expect(colorMatch.status).toHaveBeenCalledTimes(2));
  expect((screen.getByRole('combobox', { name: 'Color match preference' }) as HTMLSelectElement).value).toBe('extreme');
  expect(screen.getByTestId('one-click-measurement').textContent).toContain('Limited to protect');
  // Removing the match keeps Extreme ready for the next reference.
  vi.mocked(oneClickMatch.quick).mockResolvedValue(report('warm', 0, { requestedAmount: 0 }));
  fireEvent.click(screen.getByRole('button', { name: 'Remove matched look' }));
  await screen.findByText(/Matched look removed/);
  expect((screen.getByRole('combobox', { name: 'Color match preference' }) as HTMLSelectElement).value).toBe('extreme');
});

it('skips the reference photo itself during Sync instead of failing it', async () => {
  useStore.setState({ selection: new Set(['photo', 'a', 'b']),
    rows: [{ id: 'a', fileName: 'A.CR3' }, { id: 'b', fileName: 'Reference.CR3' }] as never });
  mount();
  fireEvent.click(await screen.findByRole('button', { name: /Wedding 02\.jpg/ }));
  await screen.findByText(/Matched to Wedding 02\.jpg/);
  vi.mocked(oneClickMatch.quick).mockImplementation(async (photo, referenceId, controls) => {
    if (photo === 'b') throw new Error('This reference is the same photo you are matching. Choose a different photo as the reference; nothing was changed.');
    return { ...report(referenceId, controls.amount), photoId: photo };
  });
  fireEvent.click(screen.getByRole('button', { name: 'Sync (2)' }));
  await screen.findByText(/Synced 1 of 2.*1 skipped \(the reference itself\), 0 failed/);
  const results = within(screen.getByRole('list', { name: 'Sync results' }));
  expect(results.getByText('Reference.CR3').parentElement?.textContent).toContain('skipped');
});
