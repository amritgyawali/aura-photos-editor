/**
 * PHASE-17. Side by side, before adoption: what AURA does now, what the candidate would do, and
 * how far each sits from doing nothing.
 *
 * Section 6.3 asks for exactly this and for the reason the panel exists: **adoption is an
 * explicit action**, and a photographer who has not seen the difference cannot make it.
 *
 * It renders numbers rather than pixels, deliberately. The develop surface already owns turning
 * a recipe into an image; a second previewer here would be a second answer to "what does this
 * look like", which is the failure phase 14 spent a page on. ADR-0036 decision 3.
 */
import type { StyleComparisonDto } from '../../ipc/types';

export type AbCompareProps = {
  rows: StyleComparisonDto[];
  currentName?: string;
  candidateName?: string;
};

/** The four axes the comparison summarises, in the order the arrays carry them. */
export const AXES = ['exposure', 'temperature', 'contrast', 'vibrance'] as const;

/** One value, in its own units, with a sign so a lean reads as a lean. */
export function format(axis: (typeof AXES)[number], value: number): string {
  if (!Number.isFinite(value)) return '—';
  const sign = value > 0 ? '+' : '';
  switch (axis) {
    case 'exposure':
      return `${sign}${value.toFixed(2)} EV`;
    case 'temperature':
      return `${sign}${Math.round(value)} K`;
    default:
      return `${sign}${Math.round(value)}`;
  }
}

/** What the level column says. A borrowed answer is named rather than hidden. */
export function levelLabel(level: string): string {
  switch (level) {
    case 'bucket':
      return 'from your own photographs of this kind';
    case 'group':
      return 'borrowed from this kind of scene';
    case 'global':
      return 'borrowed from your overall look';
    default:
      return 'no style applies here';
  }
}

export function AbCompare({ rows, currentName, candidateName }: AbCompareProps) {
  if (rows.length === 0) {
    return (
      <section className="ab-compare empty" aria-label="Compare style profiles">
        <p>
          This candidate has not learned anything about the kinds of photograph in this wedding
          yet, so there is nothing to compare. Every frame would be edited exactly as it is now.
        </p>
      </section>
    );
  }

  return (
    <section className="ab-compare" aria-label="Compare style profiles">
      <table>
        <caption>
          What would change if you adopted {candidateName ?? 'this profile'}
          {currentName ? `, against ${currentName}` : ''}
        </caption>
        <thead>
          <tr>
            <th scope="col">Kind of photograph</th>
            <th scope="col">No profile</th>
            <th scope="col">{currentName ?? 'In use now'}</th>
            <th scope="col">{candidateName ?? 'Candidate'}</th>
            <th scope="col">Where it came from</th>
          </tr>
        </thead>
        <tbody>
          {rows.map((row) => (
            <tr key={row.bucket}>
              <th scope="row">{row.title}</th>
              <td className="ab-baseline">no change</td>
              <td>
                {row.current.length === 0
                  ? 'no profile in use'
                  : AXES.map((axis, index) => (
                      <span key={axis} className="ab-value">
                        {format(axis, row.current[index] ?? 0)}
                      </span>
                    ))}
              </td>
              <td>
                {AXES.map((axis, index) => (
                  <span key={axis} className="ab-value">
                    {format(axis, row.candidate[index] ?? 0)}
                  </span>
                ))}
              </td>
              <td className={`ab-level ab-level-${row.level}`}>
                {levelLabel(row.level)}
                <span className="ab-confidence">
                  {' '}
                  ({Math.round(row.confidence * 100)}% sure)
                </span>
              </td>
            </tr>
          ))}
        </tbody>
      </table>
      <p className="ab-caveat">
        These are the changes a profile would make on top of what AURA already decides about
        each photograph. Nothing here replaces a setting you made yourself, and the skin check
        still runs afterwards on every frame.
      </p>
    </section>
  );
}
