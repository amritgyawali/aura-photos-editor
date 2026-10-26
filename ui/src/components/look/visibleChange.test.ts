import { expect, it } from 'vitest';
import type { ColorMatchReport } from '../../ipc/colorMatch';
import { reducedNote } from './visibleChange';

const report = (extra: Partial<ColorMatchReport>): ColorMatchReport => ({
  photoId: 'photo', sessionId: 'session',
  reference: { id: 'ref', name: 'Reference', width: 10, height: 10, convertedIcc: false, regions: [] },
  controls: { amount: 0.6, tone: 1, color: 1, whiteBalance: 1 }, matchedRegions: [], beforeDistance: 5,
  afterDistance: 3, changed: 1, protectedFields: 0, fullResolutionChecked: true, current: true, ...extra,
});

it('says when a smaller amount than asked for was saved, and how much', () => {
  expect(reducedNote(report({ reducedFrom: 0.8 })))
    .toBe('Saved at 60%: 80% failed this photo\'s smoothness, highlight or skin-colour check. ');
});

it('says nothing when the amount asked for was saved', () => {
  expect(reducedNote(report({}))).toBe('');
});
