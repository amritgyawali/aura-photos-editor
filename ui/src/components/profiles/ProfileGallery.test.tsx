import { fireEvent, render, screen, waitFor } from '@testing-library/react';
import { beforeEach, expect, it, vi } from 'vitest';
import { ProfileGallery } from './ProfileGallery';
import { editProfiles, readProfileSelection, saveProfileSelection, swatchGradient, type EditProfile } from './profileSelection';

vi.mock('../../ipc/client', () => ({
  inTauri: () => true,
  asIpcError: (e: Error) => ({ message: e.message }),
  editProfiles: { list: vi.fn(), preview: vi.fn(), apply: vi.fn() },
}));

const profile = (id: string, name: string, category: string, origin: 'researched' | 'learned' = 'researched'): EditProfile => ({
  id, name, category, tagline: `${name} tagline`, description: `${name} description`, bestFor: ['Portraits'],
  technique: ['Lift the shadows'], origin, sources: origin === 'researched' ? [{ title: 'Guide', url: 'https://example.com/guide' }] : [],
  evidence: origin === 'learned' ? { dataset: 'FiveK', trainingPairs: 30, heldOutPairs: 10, autoDe00: 9.4, profileDe00: 7.1 } : null,
  swatch: ['#ffffff', '#000000'], adjust: {},
});

beforeEach(() => {
  vi.clearAllMocks();
  vi.mocked(editProfiles.list).mockResolvedValue([
    profile('light-airy', 'Light & Airy', 'Bright'),
    profile('dark-moody', 'Dark & Moody', 'Moody'),
    profile('fivek-expert-c', 'Retoucher C', 'Learned', 'learned'),
  ]);
  vi.mocked(editProfiles.preview).mockImplementation(async (id: string) => ({ profileId: id, before: 'data:before', after: `data:${id}`, adaptations: id === 'dark-moody' ? ['The photograph is already dark, so the look darkens it less.'] : [] }));
});

it('lists every profile, renders real previews and lets a photographer choose one', async () => {
  const onChange = vi.fn();
  render(<ProfileGallery selection={null} disabled={false} onChange={onChange} />);
  const card = await screen.findByRole('radio', { name: /Dark & Moody/ });
  await waitFor(() => expect(editProfiles.preview).toHaveBeenCalledWith('light-airy', 1, null, 240));
  fireEvent.click(card);
  expect(onChange).toHaveBeenCalledWith({ profileId: 'dark-moody', strength: 1 });
  fireEvent.click(screen.getByRole('radio', { name: /Auto only/ }));
  expect(onChange).toHaveBeenLastCalledWith(null);
});

it('filters by category', async () => {
  render(<ProfileGallery selection={null} disabled={false} onChange={vi.fn()} />);
  await screen.findByRole('radio', { name: /Light & Airy/ });
  fireEvent.click(screen.getByRole('button', { name: 'Moody' }));
  expect(screen.queryByRole('radio', { name: /Light & Airy/ })).toBeNull();
  expect(screen.getByRole('radio', { name: /Dark & Moody/ })).toBeTruthy();
});

it('shows a before/after, the adaptations and the evidence for the chosen profile on the photo in view', async () => {
  const { rerender } = render(<ProfileGallery selection={{ profileId: 'dark-moody', strength: 0.8 }} disabled={false} onChange={vi.fn()} previewPhotoId="photo-1" />);
  expect(await screen.findByAltText('After Dark & Moody')).toBeTruthy();
  expect(editProfiles.preview).toHaveBeenCalledWith('dark-moody', 0.8, 'photo-1', 560);
  expect(screen.getByText(/already dark/)).toBeTruthy();
  rerender(<ProfileGallery selection={{ profileId: 'fivek-expert-c', strength: 1 }} disabled={false} onChange={vi.fn()} />);
  expect(await screen.findByText(/9.4 → 7.1/)).toBeTruthy();
});

it('remembers only a well-formed selection', () => {
  saveProfileSelection({ profileId: 'film-portra', strength: 0.9 });
  expect(readProfileSelection()).toEqual({ profileId: 'film-portra', strength: 0.9 });
  localStorage.setItem('aura.edit-profile.v1', JSON.stringify({ profileId: '../evil', strength: 1 }));
  expect(readProfileSelection()).toBeNull();
  localStorage.setItem('aura.edit-profile.v1', JSON.stringify({ profileId: 'x', strength: 9 }));
  expect(readProfileSelection()).toBeNull();
  saveProfileSelection(null);
  expect(readProfileSelection()).toBeNull();
  expect(swatchGradient(['#111111', '#eeeeee'])).toContain('linear-gradient');
});
