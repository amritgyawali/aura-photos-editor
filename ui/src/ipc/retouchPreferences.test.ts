import { expect, it } from 'vitest';
import { DEFAULT_AUTO_RETOUCH, DEFAULT_RETOUCH_SETTINGS } from './nativeRetouch';
import { readRetouchPreferences } from './retouchPreferences';
import type { RecipeDto } from './types';

const recipe = (options: unknown) => ({ body: JSON.stringify({ studio_portrait_auto_v1: { options } }) }) as RecipeDto;

it('restores all fine controls and per-photo scope without sharing mutable defaults', () => {
  const settings = Object.fromEntries(Object.entries(DEFAULT_RETOUCH_SETTINGS).map(([key, value]) =>
    [key, typeof value === 'boolean' ? !value : key === 'maxSpots' ? 7 : .23]));
  const saved = { intensity: .65, scope: 'face_and_body', blemishes: false, refine: true, eyes: false, teeth: true, adaptive: false, settings };
  expect(readRetouchPreferences(recipe(saved))).toEqual(saved);
  const first = readRetouchPreferences(recipe(saved));
  first.settings!.smoothing = .9;
  expect(readRetouchPreferences(recipe(saved)).settings?.smoothing).toBe(.23);
  expect(readRetouchPreferences().settings).toEqual(DEFAULT_RETOUCH_SETTINGS);
});

it('bounds numeric values and ignores unknown or incorrectly typed fields', () => {
  const actual = readRetouchPreferences(recipe({ intensity: 100, scope: 'unknown', eyes: 'false',
    settings: { smoothing: 12, skinWarmth: -12, skinTint: -.4, maxSpots: 3.7, keepFreckles: 0, unknown: 1 } }));
  expect(actual).toMatchObject({ intensity: 1.5, scope: 'face', eyes: true,
    settings: { smoothing: 1, skinWarmth: -1, skinTint: -.4, maxSpots: 4, keepFreckles: DEFAULT_RETOUCH_SETTINGS.keepFreckles } });
  expect(actual.settings).not.toHaveProperty('unknown');
});

it.each(['bad json', 'null', '[]', '{}', '{"studio_portrait_auto_v1":{"options":[]}}'])('uses independent defaults for legacy or invalid recipe %s', body => {
  const actual = readRetouchPreferences({ body } as RecipeDto);
  expect(actual).toEqual({ ...DEFAULT_AUTO_RETOUCH, settings: DEFAULT_RETOUCH_SETTINGS });
  expect(actual.settings).not.toBe(DEFAULT_RETOUCH_SETTINGS);
});

it('restores the adaptive choice and the larger spot limit only with deep cleanup', () => {
  expect(readRetouchPreferences(recipe({ settings: { maxSpots: 220, deepBlemishCleanup: true } })).settings?.maxSpots).toBe(220);
  expect(readRetouchPreferences(recipe({ settings: { maxSpots: 220, deepBlemishCleanup: false } })).settings?.maxSpots).toBe(24);
  expect(readRetouchPreferences(recipe({ settings: { maxSpots: 180, deepBlemishCleanup: true } })).settings?.maxSpots).toBe(180);
  expect(readRetouchPreferences(recipe({ settings: { maxSpots: 180 } })).settings?.maxSpots).toBe(24);
  expect(readRetouchPreferences(recipe({ adaptive: false })).adaptive).toBe(false);
  expect(readRetouchPreferences(recipe({ adaptive: 'no' })).adaptive).toBe(true);
  expect(readRetouchPreferences().adaptive).toBe(true);
});
