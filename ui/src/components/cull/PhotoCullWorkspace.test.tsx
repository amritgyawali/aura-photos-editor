import { cleanup, fireEvent, render, screen, waitFor, within } from '@testing-library/react';
import { afterEach, beforeEach, expect, it, vi } from 'vitest';

import { oneClickFinish, photoCullChecks, photoCullReview, photoCullStatus, setPhotoCullChoice, setPhotoCullStrictness, startPhotoCull } from '../../ipc/client';
import type { PhotoCullPhotoDto, PhotoCullReviewDto } from '../../ipc/types';
import { useAutomatic } from '../../state/automaticStore';
import { actionFor, choiceLabel, cullSummary, groupChecks, PhotoCullWorkspace, photosFor, reasonCounts, siblingsOf, stepFrom } from './PhotoCullWorkspace';

vi.mock('../../ipc/client', async (importOriginal) => {
  const original = await importOriginal<typeof import('../../ipc/client')>();
  return {
    ...original,
    inTauri: () => true,
    photoCullReview: vi.fn(),
    startPhotoCull: vi.fn(),
    photoCullStatus: vi.fn(),
    photoCullCancel: vi.fn(),
    photoCullChecks: vi.fn(),
    setPhotoCullChoice: vi.fn(),
    setPhotoCullStrictness: vi.fn(),
    oneClickFinish: vi.fn(),
    pickImportPaths: vi.fn(),
    getThumbnail: vi.fn(() => new Promise(() => undefined)),
  };
});

function photo(id: string, keep: boolean, score: number, extra: Partial<PhotoCullPhotoDto> = {}): PhotoCullPhotoDto {
  return {
    photoId: id, fileName: `${id}.jpg`, keep, cullKeep: keep, choice: null, topPick: false,
    reason: keep ? 'keep' : 'burst_duplicate', reasonLabel: keep ? 'Good photo' : 'Duplicate in a burst',
    detail: keep ? 'Sharp and usable.' : 'Near-identical to a.jpg, which is delivered instead.',
    score, categories: [90, 80, null, null, null, null, null, null, null, null], keptInstead: keep ? null : 'a',
    burst: null, moment: null, ...extra,
  };
}

function review(photos: PhotoCullPhotoDto[]): PhotoCullReviewDto {
  const best = photos.filter((p) => p.keep).length;
  return {
    projectId: 'p1', measuredAt: '2026-10-28T10:00:00Z', summary: '', frames: photos.length, best,
    leftOut: photos.length - best, topPicks: photos.filter((p) => p.topPick).length,
    keptByYou: photos.filter((p) => p.choice === 'keep').length,
    leftOutByYou: photos.filter((p) => p.choice === 'leave_out').length, notCulled: 0,
    groups: ['Exposure', 'Focus', 'Motion and shake', 'Noise and quality', 'Colour', 'Composition', 'Faces', 'Eyes', 'Light', 'Among similar photos'],
    strictness: 'balanced',
    levels: [
      { strictness: 'gentle', label: 'Gentle', description: 'Leaves out only mistakes.', best: 3, leftOut: 1 },
      { strictness: 'balanced', label: 'Balanced', description: 'Also weaker shots.', best, leftOut: photos.length - best },
      { strictness: 'strict', label: 'Strict', description: 'Only the strongest.', best: 1, leftOut: 3 },
    ],
    photos,
  };
}

const base = review([
  photo('a', true, 91, { topPick: true }),
  photo('b', false, 70),
  photo('c', true, 60),
  photo('d', false, 30, { reason: 'out_of_focus', reasonLabel: 'Out of focus', keptInstead: null }),
]);

beforeEach(() => {
  vi.mocked(photoCullReview).mockReset().mockResolvedValue(base);
  vi.mocked(setPhotoCullChoice).mockReset();
  vi.mocked(setPhotoCullStrictness).mockReset();
  vi.mocked(startPhotoCull).mockReset();
  vi.mocked(photoCullStatus).mockReset();
  vi.mocked(photoCullChecks).mockReset().mockResolvedValue([]);
  vi.mocked(oneClickFinish).mockReset().mockResolvedValue({ jobId: 'oneclick-1' });
  useAutomatic.setState({ jobId: null, projectId: null, status: null, starting: false, error: null });
});
afterEach(cleanup);

it('says what was kept, left out and chosen, in one sentence', () => {
  expect(cullSummary(base)).toBe('4 photos checked: 2 best · 2 left out · 1 top picks.');
  const chosen = review([photo('a', true, 90, { choice: 'keep', cullKeep: false }), photo('b', false, 20, { choice: 'leave_out', cullKeep: true })]);
  expect(cullSummary(chosen)).toContain('1 brought back by you · 1 left out by you');
});

it('orders each list best first or in shooting order, and can show only top picks', () => {
  expect(photosFor(base, 'best', 'score').map((p) => p.photoId)).toEqual(['a', 'c']);
  expect(photosFor(base, 'left', 'score').map((p) => p.photoId)).toEqual(['b', 'd']);
  expect(photosFor(review([photo('x', true, 50), photo('y', true, 80)]), 'best', 'time').map((p) => p.photoId)).toEqual(['x', 'y']);
  expect(photosFor(base, 'best', 'score', true).map((p) => p.photoId)).toEqual(['a']);
});

it('offers bring back, leave out or undo, and names who decided', () => {
  expect(actionFor(photo('b', false, 70))).toEqual({ label: 'Bring back', choice: 'keep' });
  expect(actionFor(photo('a', true, 90))).toEqual({ label: 'Leave out', choice: 'leave_out' });
  const back = photo('b', true, 70, { choice: 'keep', cullKeep: false });
  expect(actionFor(back)).toEqual({ label: 'Undo my choice', choice: 'auto' });
  expect(choiceLabel(back)).toBe('Brought back by you');
  expect(choiceLabel(photo('a', false, 90, { choice: 'leave_out', cullKeep: true }))).toBe('Left out by you');
  expect(choiceLabel(photo('a', true, 90))).toBeNull();
});

it('groups the checks and scores each group by weight, ignoring checks that did not apply', () => {
  const groups = groupChecks([
    { id: 'a', group: 'Focus', name: 'A', score: 100, value: 1, weight: 3 },
    { id: 'b', group: 'Focus', name: 'B', score: 0, value: 0, weight: 1 },
    { id: 'c', group: 'Eyes', name: 'C', score: null, value: null, weight: 1 },
  ]);
  expect(groups.map((g) => [g.group, g.score])).toEqual([['Focus', 75], ['Eyes', null]]);
});

it('shows a collection that was never culled and starts a cull that refreshes the lists', async () => {
  vi.mocked(photoCullReview).mockResolvedValueOnce(null).mockResolvedValue(base);
  vi.mocked(startPhotoCull).mockResolvedValue({ jobId: 'cull-1' });
  vi.mocked(photoCullStatus).mockResolvedValue({ jobId: 'cull-1', projectId: 'p1', status: 'completed', done: 4, total: 4, label: 'Culled.' });
  render(<PhotoCullWorkspace projectId="p1" disabled={false} onError={vi.fn()} />);
  expect(await screen.findByText('This collection has not been culled yet.')).toBeTruthy();
  fireEvent.click(screen.getByRole('button', { name: 'Cull all photos' }));
  expect(await screen.findByRole('tab', { name: 'Best (2)' })).toBeTruthy();
  expect(startPhotoCull).toHaveBeenCalledWith({ projectId: 'p1' });
});

it('brings a left-out photo back with one click', async () => {
  vi.mocked(setPhotoCullChoice).mockResolvedValue(review([photo('a', true, 91), photo('b', true, 70, { choice: 'keep', cullKeep: false }), photo('c', true, 60), photo('d', false, 30)]));
  render(<PhotoCullWorkspace projectId="p1" disabled={false} onError={vi.fn()} />);
  fireEvent.click(await screen.findByRole('tab', { name: 'Left out (2)' }));
  const card = screen.getByRole('article', { name: 'b.jpg' });
  fireEvent.click(within(card).getByRole('button', { name: 'Bring back' }));
  await waitFor(() => expect(setPhotoCullChoice).toHaveBeenCalledWith({ projectId: 'p1', photoIds: ['b'], choice: 'keep' }));
  expect(await screen.findByRole('tab', { name: 'Left out (1)' })).toBeTruthy();
});

it('keeps every photo when asked to remove none', async () => {
  vi.mocked(setPhotoCullChoice).mockResolvedValue(review(base.photos.map((p) => ({ ...p, keep: true, choice: p.keep ? null : 'keep' }))));
  render(<PhotoCullWorkspace projectId="p1" disabled={false} onError={vi.fn()} />);
  fireEvent.click(await screen.findByRole('tab', { name: 'Left out (2)' }));
  fireEvent.click(screen.getByRole('button', { name: 'Keep every photo (remove none)' }));
  await waitFor(() => expect(setPhotoCullChoice).toHaveBeenCalledWith({ projectId: 'p1', photoIds: ['b', 'd'], choice: 'keep' }));
  expect(await screen.findByText('Nothing was left out: every photo is delivered.')).toBeTruthy();
  // And every choice can be handed back to the cull in one press.
  expect(screen.getByRole('button', { name: 'Undo all my choices (2)' })).toBeTruthy();
});

it('opens a photo to show why, and what was delivered instead', async () => {
  render(<PhotoCullWorkspace projectId="p1" disabled={false} onError={vi.fn()} />);
  fireEvent.click(await screen.findByRole('tab', { name: 'Left out (2)' }));
  fireEvent.click(within(screen.getByRole('article', { name: 'b.jpg' })).getByTitle('Show its hundred checks'));
  const detail = await screen.findByRole('complementary', { name: 'Checks for b.jpg' });
  expect(within(detail).getByText('Delivered instead: a.jpg.')).toBeTruthy();
  expect(within(detail).getByText('Score 70 of 100')).toBeTruthy();
  await waitFor(() => expect(photoCullChecks).toHaveBeenCalledWith({ projectId: 'p1', photoId: 'b' }));
});

it('edits and exports the best photos, or only the ones brought back', async () => {
  vi.mocked(photoCullReview).mockResolvedValue(review([photo('a', true, 91), photo('b', true, 70, { choice: 'keep', cullKeep: false })]));
  render(<PhotoCullWorkspace projectId="p1" disabled={false} onError={vi.fn()} />);
  fireEvent.click(await screen.findByRole('button', { name: 'Edit & export only the 1 brought back' }));
  await waitFor(() => expect(oneClickFinish).toHaveBeenCalledWith({ projectId: 'p1', destination: '', ingestJobId: null, onlyPhotos: ['b'] }));
  expect(useAutomatic.getState()).toMatchObject({ jobId: 'oneclick-1', projectId: 'p1' });
});

const moment = review([
  photo('a', true, 91, { moment: 1 }),
  photo('b', false, 70, { moment: 1, reason: 'weaker_of_moment', reasonLabel: 'Weaker shot of the moment' }),
  photo('c', true, 60),
  photo('d', false, 30, { reason: 'out_of_focus', reasonLabel: 'Out of focus', keptInstead: null }),
  photo('e', false, 40, { reason: 'out_of_focus', reasonLabel: 'Out of focus', keptInstead: null }),
]);

it('counts each reason, finds a photo\'s moment and walks the list', () => {
  expect(reasonCounts(moment, 'left')).toEqual([
    { reason: 'out_of_focus', label: 'Out of focus', count: 2 },
    { reason: 'weaker_of_moment', label: 'Weaker shot of the moment', count: 1 },
  ]);
  expect(photosFor(moment, 'left', 'score', false, 'out_of_focus').map((p) => p.photoId)).toEqual(['e', 'd']);
  expect(siblingsOf(moment, moment.photos[1]!).map((p) => p.photoId)).toEqual(['a', 'b']);
  expect(siblingsOf(moment, moment.photos[2]!)).toEqual([]);
  const shown = photosFor(moment, 'left', 'time');
  expect(stepFrom(shown, 'b', 1)).toBe('d');
  expect(stepFrom(shown, 'b', -1)).toBeNull();
  expect(stepFrom(shown, 'gone', 1)).toBe('b');
});

it('switches strictness without culling again, showing what each level delivers', async () => {
  vi.mocked(setPhotoCullStrictness).mockResolvedValue({ ...base, strictness: 'strict' });
  render(<PhotoCullWorkspace projectId="p1" disabled={false} onError={vi.fn()} />);
  const strict = await screen.findByRole('radio', { name: /Strict/ });
  expect(strict.textContent).toContain('1 best · 3 left out');
  expect(screen.getByRole('radio', { name: /Balanced/ }).getAttribute('aria-checked')).toBe('true');
  fireEvent.click(strict);
  await waitFor(() => expect(setPhotoCullStrictness).toHaveBeenCalledWith({ projectId: 'p1', strictness: 'strict' }));
  await waitFor(() => expect(screen.getByRole('radio', { name: /Strict/ }).getAttribute('aria-checked')).toBe('true'));
  expect(startPhotoCull).not.toHaveBeenCalled();
});

it('asks for one more cull when a review predates strictness', async () => {
  vi.mocked(photoCullReview).mockResolvedValue({ ...base, levels: [] });
  render(<PhotoCullWorkspace projectId="p1" disabled={false} onError={vi.fn()} />);
  expect(await screen.findByText('Cull again once to choose how strict the cull is.')).toBeTruthy();
  expect(screen.queryByRole('radiogroup', { name: 'Strictness' })).toBeNull();
});

it('filters the left-out list by reason', async () => {
  vi.mocked(photoCullReview).mockResolvedValue(moment);
  render(<PhotoCullWorkspace projectId="p1" disabled={false} onError={vi.fn()} />);
  fireEvent.click(await screen.findByRole('tab', { name: 'Left out (3)' }));
  fireEvent.click(screen.getByRole('button', { name: 'Out of focus (2)' }));
  expect(screen.queryByRole('article', { name: 'b.jpg' })).toBeNull();
  expect(screen.getByRole('article', { name: 'd.jpg' })).toBeTruthy();
  fireEvent.click(screen.getByRole('button', { name: 'All' }));
  expect(screen.getByRole('article', { name: 'b.jpg' })).toBeTruthy();
});

it('shows the rest of the moment and swaps the delivered frame in one click', async () => {
  vi.mocked(photoCullReview).mockResolvedValue(moment);
  vi.mocked(setPhotoCullChoice).mockResolvedValue(moment);
  render(<PhotoCullWorkspace projectId="p1" disabled={false} onError={vi.fn()} />);
  fireEvent.click(await screen.findByRole('tab', { name: 'Left out (3)' }));
  fireEvent.click(within(screen.getByRole('article', { name: 'b.jpg' })).getByTitle('Show its hundred checks'));
  const detail = await screen.findByRole('complementary', { name: 'Checks for b.jpg' });
  const strip = within(detail).getByRole('region', { name: 'Same moment' });
  expect(within(strip).getAllByRole('button').map((b) => b.getAttribute('aria-label'))).toEqual(['a.jpg', 'b.jpg']);
  fireEvent.click(within(detail).getByRole('button', { name: 'Deliver this instead of a.jpg' }));
  await waitFor(() => expect(setPhotoCullChoice).toHaveBeenCalledTimes(2));
  expect(vi.mocked(setPhotoCullChoice).mock.calls.map((c) => [c[0].photoIds, c[0].choice])).toEqual([[['b'], 'keep'], [['a'], 'leave_out']]);
  // The strip opens a sibling.
  fireEvent.click(within(strip).getByRole('button', { name: 'a.jpg' }));
  expect(await screen.findByRole('complementary', { name: 'Checks for a.jpg' })).toBeTruthy();
});

it('walks the list with the arrow keys and decides with K, X and U', async () => {
  vi.mocked(photoCullReview).mockResolvedValue(moment);
  vi.mocked(setPhotoCullChoice).mockResolvedValue(moment);
  render(<PhotoCullWorkspace projectId="p1" disabled={false} onError={vi.fn()} />);
  fireEvent.click(await screen.findByRole('tab', { name: 'Left out (3)' }));
  fireEvent.click(within(screen.getByRole('article', { name: 'b.jpg' })).getByTitle('Show its hundred checks'));
  await screen.findByRole('complementary', { name: 'Checks for b.jpg' });
  fireEvent.keyDown(window, { key: 'ArrowRight' });
  expect(await screen.findByRole('complementary', { name: 'Checks for e.jpg' })).toBeTruthy();
  fireEvent.keyDown(window, { key: 'k' });
  await waitFor(() => expect(setPhotoCullChoice).toHaveBeenCalledWith({ projectId: 'p1', photoIds: ['e'], choice: 'keep' }));
  // X does nothing to a photo already left out; Esc closes.
  fireEvent.keyDown(window, { key: 'x' });
  expect(setPhotoCullChoice).toHaveBeenCalledTimes(1);
  fireEvent.keyDown(window, { key: 'Escape' });
  await waitFor(() => expect(screen.queryByRole('complementary')).toBeNull());
});
