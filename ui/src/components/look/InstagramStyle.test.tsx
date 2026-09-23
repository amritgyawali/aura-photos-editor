import { fireEvent, render, screen, waitFor } from '@testing-library/react';
import { beforeEach, expect, it, vi } from 'vitest';
import { InstagramStyle } from './InstagramStyle';
import { referenceStyle, type ReferenceAnalysis } from './referenceStyle';
import { pickPhotoFolder } from '../../ipc/client';

vi.mock('./referenceStyle', () => ({ referenceStyle: { fetch: vi.fn(), analyse: vi.fn() } }));
vi.mock('../../ipc/client', () => ({ inTauri: () => true, asIpcError: (e: Error) => ({ message: e.message }),
  pickPhotoFolder: vi.fn(), api: { cancelJob: vi.fn().mockResolvedValue(null) } }));
const analysis: ReferenceAnalysis = { id: 'reference', origin: '@photographer', measured: 24, skipped: 0,
  colors: ['#a08060'], brightness: 0.5, contrast: 0.6, warmth: 4, saturation: 10 };
const props = () => ({ selection: null, disabled: false, onChange: vi.fn(), onBusyChange: vi.fn(), onAddPhotos: vi.fn() });
beforeEach(() => {
  vi.clearAllMocks();
  vi.mocked(referenceStyle.analyse).mockResolvedValue(analysis);
});

it('retrieves and analyses a profile before any target collection exists', async () => {
  vi.mocked(referenceStyle.fetch).mockResolvedValue({ folder: 'cache/photos', fetched: 24, skipped: 2, complete: false, message: 'Limited sample' });
  const events = props();
  render(<InstagramStyle {...events} />);
  fireEvent.change(screen.getByLabelText('Photographer’s Instagram profile'), { target: { value: 'https://instagram.com/photographer/' } });
  fireEvent.click(screen.getByRole('button', { name: 'Analyze Instagram style' }));
  await waitFor(() => expect(events.onChange).toHaveBeenCalledWith({ analysis, strength: 0.8 }));
  expect(referenceStyle.fetch).toHaveBeenCalledWith('https://instagram.com/photographer/', 240, expect.any(String));
  expect(referenceStyle.analyse).toHaveBeenCalledWith('https://instagram.com/photographer/', 'cache/photos', expect.any(String));
  expect(screen.getByText(/Partial profile coverage/)).toBeTruthy();
});

it('reports blocked access without replacing the previous style or pretending to analyse photos', async () => {
  vi.mocked(referenceStyle.fetch).mockResolvedValue({ folder: 'empty', fetched: 0, skipped: 0, complete: false, message: 'Instagram requires login.' });
  const events = props();
  render(<InstagramStyle {...events} selection={{ analysis, strength: 0.8 }} />);
  fireEvent.change(screen.getByLabelText('Photographer’s Instagram profile'), { target: { value: '@photographer' } });
  fireEvent.click(screen.getByRole('button', { name: 'Analyze Instagram style' }));
  expect((await screen.findByRole('alert')).textContent).toContain('Instagram requires login');
  expect(referenceStyle.analyse).not.toHaveBeenCalled();
  expect(events.onChange).not.toHaveBeenCalled();
  expect(screen.getByText('@photographer')).toBeTruthy();
});

it('can learn from saved photos without network access', async () => {
  vi.mocked(pickPhotoFolder).mockResolvedValue('D:/saved references');
  const events = props();
  render(<InstagramStyle {...events} />);
  fireEvent.click(screen.getByRole('button', { name: 'Use saved reference photos' }));
  await waitFor(() => expect(events.onChange).toHaveBeenCalledWith({ analysis, strength: 0.8 }));
  expect(referenceStyle.fetch).not.toHaveBeenCalled();
});

it('does not adopt a completed analysis after Stop', async () => {
  vi.mocked(pickPhotoFolder).mockResolvedValue('D:/references');
  let finish!: (value: ReferenceAnalysis) => void;
  vi.mocked(referenceStyle.analyse).mockReturnValue(new Promise(resolve => { finish = resolve; }));
  const events = props();
  render(<InstagramStyle {...events} />);
  fireEvent.click(screen.getByRole('button', { name: 'Use saved reference photos' }));
  await waitFor(() => expect(referenceStyle.analyse).toHaveBeenCalledOnce());
  fireEvent.click(screen.getByRole('button', { name: 'Stop analysis' }));
  finish(analysis);
  await screen.findByText('Stopped. Your previous style is unchanged.');
  expect(events.onChange).not.toHaveBeenCalled();
});
