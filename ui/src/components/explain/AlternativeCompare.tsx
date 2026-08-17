import type { AlternativeDto } from '../../ipc/types';

/**
 * PHASE-13. The frame that was delivered beside the one that nearly won.
 *
 * Section 6.2 calls this "the single most trust-building screen in the product",
 * and the argument is worth keeping in the file it describes: every other part of
 * the panel asks a photographer to *believe* a decision. This one lets them check
 * it. Two frames, four sub-scores each, and the fused number underneath.
 *
 * Three rules this component keeps:
 *
 * * **It shows the breakdown, not a verdict.** No cell says better, worse, keep or
 *   reject. The columns are the numbers and the photographer draws the conclusion.
 * * **A zero is drawn as "not measured", never as zero.** Phases 09, 10 and 11 all
 *   return nothing when they have not run, and a table full of 0.00 would read as
 *   a wedding full of terrible photographs.
 * * **It cannot swap anything.** Replacing a keeper with its runner-up is phase 27.
 *   There is no button here, and there is nothing on this surface that could do it.
 */

/** A sub-score, or the honest absence of one. */
export function scoreCell(value: number): string {
  if (value <= 0) {
    return 'not measured';
  }
  return value.toFixed(2);
}

/** Which of the two frames the gallery carries. */
export function deliveredLabel(alternative: AlternativeDto): string {
  return alternative.delivered ? 'in the gallery' : 'the closest alternative';
}

/** The sentence above the table. Names the difference rather than judging it. */
export function comparisonSentence(alternatives: AlternativeDto[]): string {
  const delivered = alternatives.find((frame) => frame.delivered);
  const other = alternatives.find((frame) => !frame.delivered);
  if (!delivered || !other) {
    return 'There was no other frame of this moment to compare with.';
  }
  const gap = Math.abs(delivered.keepScore - other.keepScore);
  if (gap < 0.02) {
    return 'These two frames scored within a hundredth of each other. AURA chose between them on the tie-break, and either would be a defensible delivery.';
  }
  return `AURA scored the delivered frame ${gap.toFixed(2)} higher. The columns below show where that difference came from.`;
}

export type AlternativeCompareProps = {
  alternatives: AlternativeDto[];
  /** Called when the photographer wants to open one of the two frames. */
  onOpen?: (photoId: string) => void;
};

export function AlternativeCompare({ alternatives, onOpen }: AlternativeCompareProps) {
  if (alternatives.length === 0) {
    return (
      <p className="alternative-empty">
        This moment held one frame, so there is nothing to compare it with. That is not the same
        as there having been nothing as good.
      </p>
    );
  }

  return (
    <section className="alternative-compare" aria-label="The delivered frame and its alternative">
      <p className="alternative-lead">{comparisonSentence(alternatives)}</p>
      <table>
        <thead>
          <tr>
            <th scope="col">Measure</th>
            {alternatives.map((frame) => (
              <th scope="col" key={frame.photoId}>
                <button type="button" onClick={() => onOpen?.(frame.photoId)}>
                  {deliveredLabel(frame)}
                </button>
              </th>
            ))}
          </tr>
        </thead>
        <tbody>
          {(
            [
              ['Overall', 'keepScore'],
              ['Technical', 'technical'],
              ['Emotion', 'emotion'],
              ['Composition', 'composition'],
              ['Who is in it', 'prominence'],
            ] as const
          ).map(([label, key]) => (
            <tr key={key}>
              <th scope="row">{label}</th>
              {alternatives.map((frame) => (
                <td key={`${frame.photoId}-${key}`}>{scoreCell(frame[key])}</td>
              ))}
            </tr>
          ))}
        </tbody>
      </table>
    </section>
  );
}
