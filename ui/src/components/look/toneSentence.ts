import type { ColorMatchTone } from '../../ipc/colorMatch';

/** One sentence on the tone a reference's look implies for this photo (ADR-0180). Lightness is
 *  OKLab, so a hundredth of it is about a percent of the visible range. */
export function toneSentence(tone: ColorMatchTone): string {
  const pct = (value: number) => Math.round(value * 100);
  const parts: string[] = [];
  if (tone.lookBlack > 0.03) parts.push(`lifts blacks by about ${pct(tone.lookBlack)}%`);
  if (tone.lookWhite < 0.97) parts.push(`softens whites to about ${pct(tone.lookWhite)}%`);
  const look = parts.length > 0
    ? `The reference's look ${parts.join(' and ')}.`
    : 'The reference has an ordinary tone range, so this photo keeps its own.';
  const flat = tone.ownBlack > 0.03 || tone.ownWhite < 0.97;
  return flat ? `${look} This photo's range was flatter than a finished photo's, so it was given a finished range first.` : look;
}
