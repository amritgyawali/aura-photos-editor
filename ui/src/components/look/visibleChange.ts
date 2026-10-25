import type { ColorMatchReport } from '../../ipc/colorMatch';

/** Below this mean OKLab x100 difference, a saved match looks the same as the original. OKLab x100
 *  runs at about 0.41 of CIE76 on real photos, so this is about one CIE76 unit (ADR-0181). */
export const SLIGHT_CHANGE = 0.45;

/** Whether a saved result is too slight to see, so the panel must not call it a match. */
export function barelyChanged(result: ColorMatchReport): boolean {
  return result.changed > 0 && result.visibleChange !== undefined && result.visibleChange < SLIGHT_CHANGE;
}

/** What to say when a saved match barely changed the photo. */
export function slightMessage(result: ColorMatchReport): string {
  return `Saved, but this photo looks almost the same as before (difference ${(result.visibleChange ?? 0).toFixed(1)}; `
    + 'below about 0.45 is hard to see). The reference\'s look was already close to this photo, or the skin, highlight and '
    + 'smoothness checks allowed only a slight change. For a stronger match choose Reference palette and tone, '
    + 'or add the reference\'s original to this collection so AURA can learn your exact edit.';
}
