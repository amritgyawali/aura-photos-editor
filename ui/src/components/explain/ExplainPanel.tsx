import { useCallback, useEffect, useState } from 'react';

import { asIpcError, explain as explainApi } from '../../ipc/client';
import type { ExplainPanelDto, ExplainTabDto, LedgerReasonDto } from '../../ipc/types';

import { AlternativeCompare } from './AlternativeCompare';
import { EvidenceCrop } from './EvidenceCrop';
import { ReasonRow } from './ReasonRow';

/**
 * PHASE-13. Every decision in the app, opened up.
 *
 * Six tabs: Selection, Technical, Emotion, Composition, Edit and Quality check.
 * Four of them read a frozen service; two of them belong to phases that do not
 * exist yet and **say so**, because a tab that renders blank because phase 14 has
 * not been written looks exactly like a tab that renders blank because a
 * photograph is perfect.
 *
 * ## What this panel is not allowed to do
 *
 * Change anything. There is no keep button, no reject button, no swap and no
 * apply. A photographer who disagrees changes the decision on the culling
 * surface, and the ledger records a new decision that supersedes the old one -
 * so the explanation and the decision can never drift apart, which is section
 * 12's first failure mode.
 *
 * ## The confidence badge
 *
 * Two numbers and a band. The raw number is what the deciding code believed; the
 * calibrated one is what that belief is worth. In this build they are equal,
 * because nothing has been calibrated yet, and the panel says that out loud
 * rather than showing a percentage it cannot stand behind.
 */

/** Show the top three by weight and let the photographer ask for the rest. */
export const DEFAULT_REASONS = 3;

const CANCEL_CODES = new Set(['AURA-JOB-7001']);

type LoadState =
  | { kind: 'loading' }
  | { kind: 'ready'; value: ExplainPanelDto }
  | { kind: 'error'; message: string };

/** The strongest absolute weight on a tab, for the bars' shared scale. */
export function strongestWeight(reasons: LedgerReasonDto[]): number {
  return reasons.reduce((best, reason) => Math.max(best, Math.abs(reason.weight)), 0.01);
}

/** Reasons in the order the panel shows them: strongest first, ties on the code. */
export function orderedReasons(reasons: LedgerReasonDto[]): LedgerReasonDto[] {
  return [...reasons].sort((left, right) => {
    const byWeight = Math.abs(right.weight) - Math.abs(left.weight);
    return byWeight !== 0 ? byWeight : left.code.localeCompare(right.code);
  });
}

/** What the badge says about a confidence that nothing has calibrated. */
export function calibrationCaveat(panel: ExplainPanelDto): string | null {
  if (!panel.decision) {
    return null;
  }
  if (panel.decision.calibrated) {
    return null;
  }
  return 'AURA has not yet learned how often it is right about this kind of decision, so read this as a rough guide rather than a percentage.';
}

/** The confidence as a percentage, clamped. */
export function percent(value: number): string {
  return `${Math.round(Math.max(0, Math.min(1, value)) * 100)}%`;
}

/** The sentence a tab with nothing in it shows. Never blank. */
export function tabPlaceholder(tab: ExplainTabDto): string {
  return tab.unavailableReason ?? 'There is nothing recorded here.';
}

export type ExplainPanelProps = {
  photoId: string;
};

export function ExplainPanel({ photoId }: ExplainPanelProps) {
  const [state, setState] = useState<LoadState>({ kind: 'loading' });
  const [active, setActive] = useState('selection');
  const [showAll, setShowAll] = useState(false);
  const [evidence, setEvidence] = useState<LedgerReasonDto | null>(null);

  const load = useCallback(async () => {
    setState({ kind: 'loading' });
    try {
      const value = await explainApi.explainImage(photoId);
      setState({ kind: 'ready', value });
    } catch (raw) {
      const error = asIpcError(raw);
      if (CANCEL_CODES.has(error.code)) {
        setState({ kind: 'error', message: 'That read stopped before it finished.' });
        return;
      }
      setState({ kind: 'error', message: error.message });
    }
  }, [photoId]);

  useEffect(() => {
    void load();
  }, [load]);

  if (state.kind === 'loading') {
    return <p className="explain-loading">Reading what AURA decided…</p>;
  }
  if (state.kind === 'error') {
    return <p className="explain-error">{state.message}</p>;
  }

  const panel = state.value;
  const tab = panel.tabs.find((candidate) => candidate.id === active) ?? panel.tabs[0];
  const reasons = orderedReasons(tab?.reasons ?? []);
  const shown = showAll ? reasons : reasons.slice(0, DEFAULT_REASONS);
  const caveat = calibrationCaveat(panel);

  return (
    <section className="explain-panel" aria-label="Why AURA decided this">
      <header className="explain-header">
        {panel.headline && <h2>{panel.headline}</h2>}
        <p className="explain-summary">{panel.summary}</p>
        {panel.decision && (
          <p className="explain-badge" data-autonomy={panel.decision.autonomy}>
            <span className="badge-title">{panel.decision.autonomyTitle}</span>
            <span className="badge-confidence">
              {percent(panel.decision.calibratedConfidence)} sure
            </span>
            <span className="badge-text">{panel.decision.autonomyText}</span>
          </p>
        )}
        {caveat && <p className="explain-caveat">{caveat}</p>}
      </header>

      <nav className="explain-tabs" aria-label="What to explain">
        {panel.tabs.map((candidate) => (
          <button
            type="button"
            key={candidate.id}
            aria-pressed={candidate.id === active}
            className={candidate.available ? 'tab' : 'tab tab-unavailable'}
            onClick={() => {
              setActive(candidate.id);
              setShowAll(false);
              setEvidence(null);
            }}
          >
            {candidate.title}
          </button>
        ))}
      </nav>

      {tab && (
        <div className="explain-body">
          {tab.available ? (
            <>
              {tab.score !== null && (
                <p className="tab-score">
                  {tab.title} score {tab.score.toFixed(2)}
                  {tab.confidence !== null && `, at ${percent(tab.confidence)} confidence`}
                </p>
              )}
              <ul className="reason-list">
                {shown.map((reason) => (
                  <ReasonRow
                    key={`${reason.code}-${reason.text}`}
                    reason={reason}
                    strongest={strongestWeight(reasons)}
                    onShowEvidence={setEvidence}
                  />
                ))}
              </ul>
              {reasons.length > DEFAULT_REASONS && (
                <button type="button" className="show-all" onClick={() => setShowAll(!showAll)}>
                  {showAll ? 'Show fewer' : `Show all ${reasons.length}`}
                </button>
              )}
              {evidence?.crop && (
                <EvidenceCrop crop={evidence.crop} caption={evidence.text} />
              )}
            </>
          ) : (
            <p className="tab-placeholder">{tabPlaceholder(tab)}</p>
          )}
        </div>
      )}

      <AlternativeCompare alternatives={panel.alternatives} />

      {panel.decision && (
        <footer className="explain-provenance">
          <p>
            Decision {panel.decision.decisionId}, question {panel.decision.inputsHash}.
          </p>
          <p>
            {panel.decision.modelVersions
              .map(([name, version]) => `${name} v${version}`)
              .concat(
                panel.decision.configVersions.map(([name, version]) => `${name} v${version}`),
              )
              .join(' · ')}
          </p>
        </footer>
      )}
    </section>
  );
}
