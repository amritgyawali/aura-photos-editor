import { expect, it } from 'vitest';
import type { BrushPoint } from '../../ipc/nativeRetouch';
import { appendBrushPoint } from './brushPath';

it('keeps the endpoints of a long straight path without exhausting the point budget', () => {
  let points: BrushPoint[] = [[0, .5, .4]];
  for (let x = 1; x <= 5000; x++) points = appendBrushPoint(points, [x / 5000, .5, .4], 6000, 4000)!;
  expect(points).toEqual([[0, .5, .4], [1, .5, .4]]);
});
it('preserves corners, reversals and non-linear pen pressure', () => {
  const corners: BrushPoint[] = [[.1, .1, .2], [.2, .1, .2]];
  expect(appendBrushPoint(corners, [.2, .2, .2], 6000, 4000)).toHaveLength(3);
  expect(appendBrushPoint(corners, [.15, .1, .2], 6000, 4000)).toHaveLength(3);
  expect(appendBrushPoint([[.1, .1, .2], [.2, .1, .9]], [.3, .1, .2], 6000, 4000)).toHaveLength(3);
});
it('refuses extra complex points at the limit and ignores subpixel jitter', () => {
  const full: BrushPoint[] = Array.from({length: 1024}, (_, i) => [i / 2048, i % 2 ? .1 : .2, .4]);
  expect(appendBrushPoint(full, [.8, .5, .4], 6000, 4000)).toBeNull();
  const points: BrushPoint[] = [[.5, .5, .4]];
  expect(appendBrushPoint(points, [.50001, .5, .405], 6000, 4000)).toBe(points);
});
