import type { ReferenceSelection } from '../../ipc/client';
export { referenceStyle } from '../../ipc/client';
export type { ReferenceAnalysis, ReferenceSelection, FetchReport, ApplyReport } from '../../ipc/client';

const STORAGE = 'aura.reference-style.v1';
export function readReferenceSelection(): ReferenceSelection | null {
  try {
    const value: unknown = JSON.parse(localStorage.getItem(STORAGE) ?? 'null');
    if (!value || typeof value !== 'object' || !('analysis' in value) || !('strength' in value)) return null;
    const candidate = value as ReferenceSelection;
    if (typeof candidate.analysis?.id !== 'string' || typeof candidate.analysis?.origin !== 'string' || !Array.isArray(candidate.analysis.colors)
      || !candidate.analysis.colors.every(color => typeof color === 'string' && /^#[0-9a-f]{6}$/i.test(color))
      || !Number.isFinite(candidate.strength) || candidate.strength < 0 || candidate.strength > 1) return null;
    return candidate;
  } catch { return null; }
}
export function saveReferenceSelection(value: ReferenceSelection | null): void {
  try { if (value) localStorage.setItem(STORAGE, JSON.stringify(value)); else localStorage.removeItem(STORAGE); } catch { /* Session still works if storage is unavailable. */ }
}
