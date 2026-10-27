import type { ColorMatchControls, ColorReference } from '../../ipc/colorMatch';

/** AURA-authored strength presets. All keep the native clipping and skin guards enabled. */
export const COLOR_MATCH_PREFERENCES = [
  { id: 'subtle', name: 'Subtle', amount: 0.5 },
  { id: 'standard', name: 'Standard', amount: 1 },
  { id: 'strong', name: 'Strong', amount: 1.5 },
  { id: 'extreme', name: 'Extreme', amount: 2 },
] as const;

export function matchPreference(id: string): ColorMatchControls | null {
  const preference = COLOR_MATCH_PREFERENCES.find(value => value.id === id);
  return preference ? { amount: preference.amount, tone: 1, color: 1, whiteBalance: 1 } : null;
}

export function preferenceOf(controls: ColorMatchControls): string {
  if (controls.tone !== 1 || controls.color !== 1 || controls.whiteBalance !== 1 ||
      Object.values(controls.detail ?? {}).some(value => value !== 1)) return 'custom';
  return COLOR_MATCH_PREFERENCES.find(value => value.amount === controls.amount)?.id ?? 'custom';
}

/** Rank only the owner's references: shared measured regions, then favorites, then name. */
export function recommendedReferences(references: ColorReference[], regions: readonly string[]): ColorReference[] {
  const names = new Set(regions.filter(region => region !== 'whole image'));
  const score = (reference: ColorReference): number => reference.regions.filter(region => names.has(region)).length * 2 + Number(Boolean(reference.favorite));
  return [...references].sort((left, right) => score(right) - score(left) || left.name.localeCompare(right.name));
}
