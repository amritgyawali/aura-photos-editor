import { fireEvent, render, screen, waitFor } from '@testing-library/react';
import { beforeEach, expect, it, vi } from 'vitest';
import { InstagramStyle } from './InstagramStyle';
import { referenceStyle, type ReferenceAnalysis } from './referenceStyle';
import { pickPhotoFolder } from '../../ipc/client';

vi.mock('./referenceStyle', () => ({ referenceStyle: { analyse: vi.fn() } }));
vi.mock('../../ipc/client', () => ({ inTauri: () => true, asIpcError: (e: Error) => ({ message: e.message }),
  pickPhotoFolder: vi.fn(), api: { cancelJob: vi.fn().mockResolvedValue(null) } }));
const analysis: ReferenceAnalysis = { id: 'reference', origin: 'Asha', measured: 24, skipped: 0,
  colors: ['#a08060'], brightness: 0.5, contrast: 0.6, warmth: 4, saturation: 10 };
const props = () => ({ selection: null, disabled: false, onChange: vi.fn(), onBusyChange: vi.fn(), onAddPhotos: vi.fn() });
beforeEach(() => {
  vi.clearAllMocks();
  vi.mocked(referenceStyle.analyse).mockResolvedValue(analysis);
});

it('learns a look from a folder of reference photos, with a label and nothing downloaded', async () => {
  vi.mocked(pickPhotoFolder).mockResolvedValue('D:/saved references');
  const events = props();
  render(<InstagramStyle {...events} />);
  fireEvent.change(screen.getByLabelText(/Whose look is this/), { target: { value: 'Asha' } });
  fireEvent.click(screen.getByRole('button', { name: 'Choose reference photos' }));
  await waitFor(() => expect(events.onChange).toHaveBeenCalledWith({ analysis, strength: 0.8 }));
  expect(referenceStyle.analyse).toHaveBeenCalledWith('Asha', 'D:/saved references', expect.any(String), false);
});

it('reads an Instagram data export by its own layout', async () => {
  vi.mocked(pickPhotoFolder).mockResolvedValue('D:/instagram-export');
  const events = props();
  render(<InstagramStyle {...events} />);
  fireEvent.click(screen.getByRole('button', { name: 'Use my Instagram data export' }));
  await waitFor(() => expect(events.onChange).toHaveBeenCalled());
  expect(referenceStyle.analyse).toHaveBeenCalledWith('', 'D:/instagram-export', expect.any(String), true);
});

it('keeps the previous style when the folder is refused', async () => {
  vi.mocked(pickPhotoFolder).mockResolvedValue('D:/three photos');
  vi.mocked(referenceStyle.analyse).mockRejectedValue(new Error('That folder holds 3 photographs AURA can read, against a minimum of 8.'));
  const events = props();
  render(<InstagramStyle {...events} selection={{ analysis, strength: 0.8 }} />);
  fireEvent.click(screen.getByRole('button', { name: 'Choose reference photos' }));
  expect((await screen.findByRole('alert')).textContent).toContain('minimum of 8');
  expect(events.onChange).not.toHaveBeenCalled();
  expect(screen.getByText('Asha')).toBeTruthy();
});

it('does nothing when the folder picker is cancelled', async () => {
  vi.mocked(pickPhotoFolder).mockResolvedValue(null);
  const events = props();
  render(<InstagramStyle {...events} />);
  fireEvent.click(screen.getByRole('button', { name: 'Choose reference photos' }));
  await screen.findByText('Stopped. Your previous style is unchanged.');
  expect(referenceStyle.analyse).not.toHaveBeenCalled();
});

it('does not adopt a completed analysis after Stop', async () => {
  vi.mocked(pickPhotoFolder).mockResolvedValue('D:/references');
  let finish!: (value: ReferenceAnalysis) => void;
  vi.mocked(referenceStyle.analyse).mockReturnValue(new Promise(resolve => { finish = resolve; }));
  const events = props();
  render(<InstagramStyle {...events} />);
  fireEvent.click(screen.getByRole('button', { name: 'Choose reference photos' }));
  await waitFor(() => expect(referenceStyle.analyse).toHaveBeenCalledOnce());
  fireEvent.click(screen.getByRole('button', { name: 'Stop analysis' }));
  finish(analysis);
  await screen.findByText('Stopped. Your previous style is unchanged.');
  expect(events.onChange).not.toHaveBeenCalled();
});
