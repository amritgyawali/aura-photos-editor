import type { AutopilotEventDto, AutopilotSummaryDto } from '../../ipc/types';
import { humanDuration } from './ProgressPanel';

/**
 * PHASE-28. What the run did, read by somebody who has come back two hours later.
 *
 * Pure - a summary and a list of governor events in, nothing fetched.
 *
 * ## Why the skipped list comes before the counts
 *
 * A photographer opening this at one in the morning is not looking for how many photographs were
 * selected. They are looking for what did not happen, and the honest version of that is a list with
 * a sentence per row rather than a number.
 *
 * That is also why `status` is rendered as its own sentence: `CompletedDegraded` is a real outcome
 * with a real meaning - the wedding is done and some steps did not run - and a panel that showed it
 * as a green tick would be a panel that lied by omission.
 *
 * ## Why the governor's events are here
 *
 * A run that took four hours when it promised two is the case this section exists for. Every
 * reading the governor acted on is a row, newest first, and none of them is a failure - they are
 * what a machine asked the product to do about itself.
 */
export type RunSummaryProps = {
  /** The newest finished run, or null when none has finished. */
  summary: AutopilotSummaryDto | null;
  /** Everything the governor did, newest first. */
  events: AutopilotEventDto[];
};

/** Which statuses mean the wedding is not simply finished. */
function statusClass(status: string): string {
  switch (status) {
    case 'completed':
      return 'is-complete';
    case 'completed_degraded':
      return 'is-degraded';
    case 'cancelled':
      return 'is-cancelled';
    case 'failed':
      return 'is-failed';
    default:
      return 'is-running';
  }
}

export function RunSummary({ summary, events }: RunSummaryProps) {
  if (!summary) {
    return (
      <section className="autopilot-summary is-empty" aria-label="What AURA did">
        <p>This wedding has not been run yet.</p>
      </section>
    );
  }

  const slowest = [...summary.stageTimings].sort((a, b) => b[1] - a[1]).slice(0, 5);

  return (
    <section className={`autopilot-summary ${statusClass(summary.status)}`} aria-label="What AURA did">
      <header>
        <h2>{summary.statusTitle}</h2>
        <p>{humanDuration(Math.round(summary.totalMs / 1000))} in total.</p>
      </header>

      {summary.degradedStages.length > 0 ? (
        <div className="autopilot-summary-degraded">
          <h3>What did not happen</h3>
          <ul>
            {summary.degradedStages.map(([stage, why]) => (
              <li key={stage} data-stage={stage}>
                <span className="autopilot-summary-stage">{stage}</span>
                <span className="autopilot-summary-why">{why}</span>
              </li>
            ))}
          </ul>
        </div>
      ) : null}

      <dl className="autopilot-summary-counts">
        <div>
          <dt>Chosen for the gallery</dt>
          <dd>{summary.selected}</dd>
        </div>
        <div>
          <dt>Files written</dt>
          <dd>{summary.exported}</dd>
        </div>
        <div>
          <dt>Waiting for you</dt>
          <dd>{summary.needsReview}</dd>
        </div>
        <div>
          <dt>Spent on AI</dt>
          <dd>${summary.spendUsd.toFixed(2)}</dd>
        </div>
      </dl>

      {summary.outputPath ? (
        <p className="autopilot-summary-path">
          Delivered files: <code>{summary.outputPath}</code>
        </p>
      ) : null}

      {slowest.length > 0 ? (
        <div className="autopilot-summary-timings">
          <h3>Where the time went</h3>
          <ul>
            {slowest.map(([stage, ms]) => (
              <li key={stage}>
                <span>{stage}</span>
                <span>{humanDuration(Math.round(ms / 1000))}</span>
              </li>
            ))}
          </ul>
        </div>
      ) : null}

      {events.length > 0 ? (
        <div className="autopilot-summary-events">
          <h3>What your machine asked for</h3>
          <ul>
            {events.slice(0, 12).map((event, index) => (
              <li key={`${event.kind}-${event.stage}-${index}`}>
                <span>{event.actionText}</span>
                <span className="autopilot-summary-event-kind">
                  {event.kind} during {event.stage}
                </span>
              </li>
            ))}
          </ul>
        </div>
      ) : null}
    </section>
  );
}
