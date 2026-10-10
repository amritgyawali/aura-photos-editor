import type { ProfileSelection } from '../../ipc/client';

export type { EditProfile, ProfileSelection, ProfilePreview, ApplyProfileReport, LearnedStyle } from '../../ipc/client';
export { editProfiles, pickLightroomCatalog } from '../../ipc/client';

const STORAGE = 'aura.edit-profile.v1';

/** The profile chosen on the start screen, remembered between launches. */
export function readProfileSelection(): ProfileSelection | null {
  try {
    const value: unknown = JSON.parse(localStorage.getItem(STORAGE) ?? 'null');
    if (!value || typeof value !== 'object') return null;
    const candidate = value as ProfileSelection;
    if (typeof candidate.profileId !== 'string' || !/^[a-z0-9-]{1,64}$/.test(candidate.profileId)
      || !Number.isFinite(candidate.strength) || candidate.strength < 0 || candidate.strength > 1.5) return null;
    return { profileId: candidate.profileId, strength: candidate.strength };
  } catch { return null; }
}

export function saveProfileSelection(value: ProfileSelection | null): void {
  try { if (value) localStorage.setItem(STORAGE, JSON.stringify(value)); else localStorage.removeItem(STORAGE); } catch { /* Session still works if storage is unavailable. */ }
}

/** A CSS stand-in for a profile before a rendered preview exists: its swatch as a soft gradient. */
export function swatchGradient(colors: string[]): string {
  if (colors.length === 0) return '#2a2730';
  if (colors.length === 1) return colors[0] ?? '#2a2730';
  const stops = colors.map((color, index) => `${color} ${Math.round((index / (colors.length - 1)) * 100)}%`);
  return `linear-gradient(135deg, ${stops.join(', ')})`;
}
