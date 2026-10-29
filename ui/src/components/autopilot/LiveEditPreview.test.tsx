import { fireEvent, render, screen, within } from '@testing-library/react';
import { act } from 'react';
import { afterEach, expect, it, vi } from 'vitest';

import { KEPT_PHOTOS, LiveEditPreview, finishPhoto, photoEvent, startPhoto, type LivePhoto } from './LiveEditPreview';

const picture = (photoId: string, number: number, step: string) => ({
  photoId, number, total: 10, title: step, state: 'picture' as const,
  picture: { step, image: `data:image/jpeg;base64,${step.replace(/\W/g, '')}`, width: 3, height: 2 },
});

/** A photograph started, its original and two steps arrived, and stage 8 running. */
function editing(): LivePhoto[] {
  let photos = startPhoto([], { photoId: 'a', name: 'a.CR3', index: 25, total: 87 });
  photos = photoEvent(photos, picture('a', 0, 'Original'));
  photos = photoEvent(photos, picture('a', 1, 'Basic edit'));
  photos = photoEvent(photos, picture('a', 7, 'Skin smoothing · Frequency separation'));
  photos = photoEvent(photos, { photoId: 'a', number: 8, total: 10, title: 'Face', state: 'running' });
  return photos;
}

afterEach(() => vi.useRealTimers());

it('collects every picture of the photograph being edited, in order, and ignores other photographs', () => {
  const photos = photoEvent(editing(), picture('elsewhere', 2, 'Colour correction'));
  expect(photos).toHaveLength(1);
  expect(photos[0]?.pictures.map(p => p.step)).toEqual(['Original', 'Basic edit', 'Skin smoothing · Frequency separation']);
  expect(photos[0]?.working).toBe('Stage 8 of 10: Face');
  const done = finishPhoto(photos, 'a', 'ready');
  expect(done[0]?.outcome).toBe('ready');
  // A finished photograph takes no more pictures.
  expect(photoEvent(done, picture('a', 9, 'Body'))[0]?.pictures).toHaveLength(3);
});

it('keeps the pictures of the last few photographs only', () => {
  let photos: LivePhoto[] = [];
  for (let i = 0; i < KEPT_PHOTOS + 3; i += 1) photos = startPhoto(photos, { photoId: `p${i}`, name: `p${i}`, index: i, total: 99 });
  expect(photos).toHaveLength(KEPT_PHOTOS);
  expect(photos[photos.length - 1]?.photoId).toBe(`p${KEPT_PHOTOS + 2}`);
});

it('shows the photograph being edited after its newest step, and every step beneath it', () => {
  render(<LiveEditPreview photos={editing()} focus={null} onFocus={vi.fn()} />);
  expect(screen.getByRole('heading', { name: 'Editing photo 26 of 87: a.CR3' })).toBeTruthy();
  expect(screen.getByRole('status').textContent).toBe('Stage 8 of 10: Face');
  expect(screen.getByRole('img', { name: 'a.CR3: Skin smoothing · Frequency separation' })).toBeTruthy();
  expect(screen.getByText('Step 2: Skin smoothing · Frequency separation')).toBeTruthy();
  const steps = within(screen.getByRole('list', { name: 'Every step of a.CR3' })).getAllByRole('button');
  expect(steps.map(step => step.textContent)).toEqual(['Original', '1. Basic edit', '2. Skin smoothing · Frequency separation']);

  // Picking the original holds it until "Follow live".
  fireEvent.click(steps[0] as HTMLElement);
  expect(screen.getByRole('img', { name: 'a.CR3: Original' })).toBeTruthy();
  fireEvent.click(screen.getByRole('button', { name: 'Follow live' }));
  expect(screen.getByRole('img', { name: 'a.CR3: Skin smoothing · Frequency separation' })).toBeTruthy();

  // Before and after, side by side.
  fireEvent.click(screen.getByRole('button', { name: 'Before | After' }));
  expect(screen.getByRole('img', { name: 'a.CR3 before the edit' })).toBeTruthy();
  expect(screen.getByText('So far: Skin smoothing · Frequency separation')).toBeTruthy();
});

it('replays a finished edit step by step and opens an earlier photograph from the recent strip', () => {
  vi.useFakeTimers();
  let photos = finishPhoto(editing(), 'a', 'ready');
  photos = startPhoto(photos, { photoId: 'b', name: 'b.CR3', index: 26, total: 87 });
  const onFocus = vi.fn();
  const { rerender } = render(<LiveEditPreview photos={photos} focus={null} onFocus={onFocus} />);
  expect(screen.getByText('Rendering the original…')).toBeTruthy();
  fireEvent.click(screen.getByRole('button', { name: 'a.CR3' }));
  expect(onFocus).toHaveBeenCalledWith('a');

  rerender(<LiveEditPreview photos={photos} focus="a" onFocus={onFocus} />);
  expect(screen.getByRole('heading', { name: 'a.CR3 · edited' })).toBeTruthy();
  fireEvent.click(screen.getByRole('button', { name: 'Replay the edit' }));
  expect(screen.getByRole('img', { name: 'a.CR3: Original' })).toBeTruthy();
  act(() => { vi.advanceTimersByTime(900); });
  expect(screen.getByRole('img', { name: 'a.CR3: Basic edit' })).toBeTruthy();
  act(() => { vi.advanceTimersByTime(900); });
  expect(screen.getByRole('img', { name: 'a.CR3: Skin smoothing · Frequency separation' })).toBeTruthy();
  fireEvent.click(screen.getByRole('button', { name: 'Back to the photo being edited' }));
  expect(onFocus).toHaveBeenLastCalledWith(null);
});
