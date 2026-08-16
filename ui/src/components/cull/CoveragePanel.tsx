import type { CoverageReportDto, CullStatusDto } from '../../ipc/types';

/**
 * PHASE-12's coverage panel.
 *
 * Section 13's second acceptance criterion: "the coverage panel shows every
 * must-have as covered, weakly covered or genuinely missing."
 *
 * The three states are drawn differently and each carries a written label, so
 * telling them apart never depends on colour. That is not only an accessibility
 * rule here - the difference between "weakly covered" and "missing" is the
 * difference between a photograph a photographer should look at and a part of
 * the wedding that does not exist, and a panel that expressed it as a hue would
 * be a panel that expressed it badly.
 *
 * ## The sentence this panel exists to make impossible
 *
 * "Everything is covered." Nothing here says that unless every guarantee that
 * had a candidate came out covered, and `missing` is always rendered with the
 * reason - "no candidates found" - rather than as a red dot.
 *
 * ## Coverage of the *analysis* is shown first, and deliberately
 *
 * A gallery chosen from 60 % of a wedding looks exactly like a gallery chosen
 * from all of it. `CullStatusDto.coverage` is the only place that difference
 * appears, so it leads the panel rather than sitting in a tooltip.
 */

/** Below this the gallery was chosen from too little of the wedding to trust. */
export const COVERAGE_WARN_BELOW = 0.95;

/** The words for one coverage state. Never a colour alone. */
export function stateLabel(state: string): string {
  switch (state) {
    case 'covered':
      return 'covered';
    case 'covered_weak':
      return 'weakly covered';
    default:
      return 'missing';
  }
}

/** What "weakly covered" means, in the photographer's terms. */
export function stateExplanation(state: string): string {
  switch (state) {
    case 'covered':
      return 'in the gallery, from frames that met the usual bar.';
    case 'covered_weak':
      return 'in the gallery, but only from frames below the usual bar. Worth a look.';
    default:
      return 'not in the gallery. AURA found no photographs of it and cannot invent any.';
  }
}

/** The banner shown when the cull was drawn over part of a wedding. */
export function analysisSentence(status: CullStatusDto): string {
  const percent = Math.round(status.coverage * 100);
  if (status.coverage >= COVERAGE_WARN_BELOW) {
    return `Chosen from ${status.eligible} of ${status.photos} photographs.`;
  }
  return `Chosen from ${percent}% of this wedding. ${
    status.photos - status.eligible
  } photographs have not been checked yet, so they were left out rather than judged.`;
}

/** How much of the fused score was real, per signal. */
export function signalSentence(status: CullStatusDto): string {
  const parts = [
    `expression on ${Math.round(status.emotionAware * 100)}%`,
    `framing on ${Math.round(status.compositionAware * 100)}%`,
    `grouping on ${Math.round(status.grouped * 100)}%`,
  ];
  return `Of the photographs considered, AURA had ${parts.join(', ')}.`;
}

export type CoveragePanelProps = {
  status: CullStatusDto;
  coverage: CoverageReportDto;
  /** Called when the photographer asks to see a guarantee's frames. */
  onShowRule?: (rule: string) => void;
};

export function CoveragePanel({ status, coverage, onShowRule }: CoveragePanelProps) {
  const missing = coverage.mustHaves.filter((rule) => !rule.satisfied);
  const weak = coverage.mustHaves.filter((rule) => rule.state === 'covered_weak');

  return (
    <section className="coverage-panel" aria-label="Story coverage">
      <header className="coverage-panel__header">
        <h2>What is in this gallery</h2>
        <p
          className={
            status.coverage >= COVERAGE_WARN_BELOW
              ? 'coverage-panel__analysis'
              : 'coverage-panel__analysis coverage-panel__analysis--warn'
          }
        >
          {analysisSentence(status)}
        </p>
        <p className="coverage-panel__signals">{signalSentence(status)}</p>
      </header>

      <ul className="coverage-panel__rules">
        {coverage.mustHaves.map((rule) => (
          <li
            key={rule.rule}
            className={`coverage-panel__rule coverage-panel__rule--${rule.state}`}
            data-state={rule.state}
          >
            <button
              type="button"
              onClick={() => onShowRule?.(rule.rule)}
              disabled={!onShowRule || !rule.satisfied}
            >
              <span className="coverage-panel__title">{rule.title}</span>
              <span className="coverage-panel__state">{stateLabel(rule.state)}</span>
            </button>
            <span className="coverage-panel__why">{stateExplanation(rule.state)}</span>
          </li>
        ))}
      </ul>

      <p className="coverage-panel__summary">
        {missing.length === 0
          ? 'Every part of the wedding AURA has rules for is in the gallery.'
          : `${missing.length} of ${coverage.mustHaves.length} are missing because no photographs of them were found.`}
        {weak.length > 0
          ? ` ${weak.length} are covered only weakly and are worth reviewing.`
          : ''}
      </p>

      {coverage.warnings.length > 0 ? (
        <section className="coverage-panel__warnings" aria-label="Before you deliver">
          <h3>Before you deliver</h3>
          <ul>
            {coverage.warnings.map((warning) => (
              <li key={warning}>{warning}</li>
            ))}
          </ul>
        </section>
      ) : null}

      <section className="coverage-panel__chapters" aria-label="Parts of the day">
        <h3>Parts of the day</h3>
        <ul>
          {coverage.chapters
            .filter((chapter) => chapter.delivered > 0 || chapter.target > 0)
            .map((chapter) => (
              <li key={chapter.chapter}>
                <span>{chapter.title}</span>
                <span>
                  {chapter.delivered} delivered, {chapter.target} aimed for
                </span>
              </li>
            ))}
        </ul>
      </section>
    </section>
  );
}
