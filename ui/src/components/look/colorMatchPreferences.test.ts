import { expect, it } from 'vitest';
import { COLOR_MATCH_PREFERENCES, matchPreference, preferenceOf, recommendedReferences } from './colorMatchPreferences';

it('uses distinct authored strengths without disabling native protection', () => {
  expect(COLOR_MATCH_PREFERENCES.map(item => matchPreference(item.id)?.amount)).toEqual([0.5, 1, 1.5, 2]);
  expect(matchPreference('extreme')).toEqual({ amount: 2, tone: 1, color: 1, whiteBalance: 1 });
  expect(matchPreference('missing')).toBeNull();
  expect(preferenceOf({ ...matchPreference('standard')!, detail: { tint: 0 } })).toBe('custom');
  expect(preferenceOf(matchPreference('strong')!)).toBe('strong');
});

it('ranks only owned references by measured regions and keeps the source order intact', () => {
  const reference = { width: 10, height: 10, convertedIcc: false };
  const items = [
    { ...reference, id: 'plain', name: 'A', regions: ['whole image'] },
    { ...reference, id: 'favorite', name: 'B', regions: [], favorite: true },
    { ...reference, id: 'skin', name: 'C', regions: ['face skin'] },
  ];
  expect(recommendedReferences(items, ['face skin']).map(item => item.id)).toEqual(['skin', 'favorite', 'plain']);
  expect(items.map(item => item.id)).toEqual(['plain', 'favorite', 'skin']);
});
