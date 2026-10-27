import { act, fireEvent, render, screen, waitFor } from '@testing-library/react';
import { afterEach, beforeEach, expect, it, vi } from 'vitest';
import { LightroomPresetExport } from './LightroomPresetExport';
import { colorMatch, type LightroomPreset } from '../../ipc/colorMatch';
import { api } from '../../ipc/client';

vi.mock('../../ipc/client', () => ({ inTauri: () => true, api: { cancelJob: vi.fn() },
  asIpcError: (error: Error) => ({ message: error.message }) }));
vi.mock('../../ipc/colorMatch', () => ({ colorMatch: { generatePreset: vi.fn() } }));
const preset: LightroomPreset = { xml: '<x:xmpmeta/>', fileName: 'AURA-Warm.xmp', name: 'Warm', recipeHash: 'hash',
  tableFingerprint: 'fingerprint', tableEdge: 32, meanTableError: .1, p95TableError: .3, maxTableError: .8,
  warnings: ['Lightroom uses different raw processing.'] };
const props = { photoId: 'photo', sessionId: 'session', referenceName: 'Warm.png', eligible: true, disabled: false };
const createUrl = vi.fn((_blob: Blob) => 'blob:preset');
const revokeUrl = vi.fn();

beforeEach(() => {
  vi.clearAllMocks();
  const OriginalURL = URL;
  vi.stubGlobal('URL', class extends OriginalURL { static override createObjectURL = createUrl; static override revokeObjectURL = revokeUrl; });
  vi.mocked(colorMatch.generatePreset).mockResolvedValue(preset);
  vi.mocked(api.cancelJob).mockResolvedValue(true);
});
afterEach(() => vi.unstubAllGlobals());

it('requires a saved checked match and does not export an analysis preview', () => {
  render(<LightroomPresetExport {...props} eligible={false}/>);
  expect(screen.getByRole('button', { name: 'Generate .xmp for Lightroom' }).hasAttribute('disabled')).toBe(true);
  expect(screen.queryByRole('link', { name: 'Download .xmp' })).toBeNull();
  expect(colorMatch.generatePreset).not.toHaveBeenCalled();
});

it('generates a named preset and keeps its download alive until the section is released', async () => {
  const view = render(<LightroomPresetExport {...props}/>);
  fireEvent.change(screen.getByLabelText('Lightroom preset name'), { target: { value: 'My warm look' } });
  fireEvent.click(screen.getByRole('button', { name: 'Generate .xmp for Lightroom' }));
  const link = await screen.findByRole('link', { name: 'Download .xmp' });
  expect(colorMatch.generatePreset).toHaveBeenCalledWith('photo', 'session', 'My warm look', expect.any(String));
  expect(link.getAttribute('href')).toBe('blob:preset');
  expect(link.getAttribute('download')).toBe('AURA-Warm.xmp');
  expect(createUrl.mock.calls[0]?.[0]).toBeInstanceOf(Blob);
  expect(revokeUrl).not.toHaveBeenCalled();
  view.unmount();
  expect(revokeUrl).toHaveBeenCalledWith('blob:preset');
});

it('shows native refusal without exposing an empty or previous download', async () => {
  vi.mocked(colorMatch.generatePreset).mockRejectedValue(new Error('The saved edit changed. Match again.'));
  render(<LightroomPresetExport {...props}/>);
  fireEvent.click(screen.getByRole('button', { name: 'Generate .xmp for Lightroom' }));
  expect((await screen.findByRole('alert')).textContent).toContain('saved edit changed');
  expect(screen.queryByRole('link', { name: 'Download .xmp' })).toBeNull();
  expect(screen.getByRole('button', { name: 'Generate .xmp for Lightroom' }).hasAttribute('disabled')).toBe(false);
});

it('cancels on unmount and never exposes the late response for another photo', async () => {
  let finish: (value: LightroomPreset) => void = () => undefined;
  vi.mocked(colorMatch.generatePreset).mockImplementation(() => new Promise(resolve => { finish = resolve; }));
  const view = render(<LightroomPresetExport {...props}/>);
  fireEvent.click(screen.getByRole('button', { name: 'Generate .xmp for Lightroom' }));
  await waitFor(() => expect(colorMatch.generatePreset).toHaveBeenCalledTimes(1));
  view.unmount();
  expect(api.cancelJob).toHaveBeenCalledWith(vi.mocked(colorMatch.generatePreset).mock.calls[0]?.[3]);
  await act(async () => { finish(preset); });
  expect(createUrl).not.toHaveBeenCalled();
});
