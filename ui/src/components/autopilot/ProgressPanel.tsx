import type { AutopilotProgressDto } from '../../ipc/types';

/**
 * PHASE-28. What the run is doing right now, and how long is left.
 *
 * Pure - one progress value and a callback in, nothing fetched.
 *
 * ## Why the ETA says whether it is measured
 *
 * Section 6.4 asks for an ETA within 20 % after 10 % of the run, and the number before that point
 * comes from an estimate the *phase document* wrote for a reference laptop rather than from
 * anything this machine has done. Those are two different claims, and a panel that showed them
 * identically would promise two hours on a machine doing four - which a photographer would find out
 * about at hour three.
 *
 * So a throughput of zero renders as "working out how long this will take" rather than as a time.
 *
 * ## Why the spend meter is here at all
 *
 * Section 7 of the phase document: this phase makes no cloud call of its own. The meter is here
 * because the *stages* can - phase 24's editorial judgement and phase 27's planner both reach a
 * provider - and a run that spent a photographer's money without a meter would be a run they found
 * out about on a bill.
 */
export type ProgressPanelProps = {
  /** What the run is doing, or null when nothing is running. */
  progress: AutopilotProgressDto | null;
  /** Stop the run. The token is polled between units, so nothing is lost. */
  onCancel?: () => void;
};

/** Seconds as the words a person uses. */
export function humanDuration(seconds: number): string {
  if (seconds <= 0) return 'less than a minute';
  const hours = Math.floor(seconds / 3600);
  const minutes = Math.round((seconds % 3600) / 60);
  if (hours === 0) return minutes <= 1 ? 'about a minute' : `about ${minutes} minutes`;
  if (minutes === 0) return hours === 1 ? 'about an hour' : `about ${hours} hours`;
  return `about ${hours}h ${minutes}m`;
}

export function ProgressPanel({ progress, onCancel }: ProgressPanelProps) {
  if (!progress) {
    return (
      <section className="autopilot-progress is-idle" aria-label="Progress">
        <p>Nothing is running.</p>
      </section>
    );
  }

  const stageFraction =
    progress.itemsTotal > 0 ? Math.min(1, progress.itemsDone / progress.itemsTotal) : 0;
  const overall =
    progress.stageTotal > 0
      ? Math.min(1, (progress.stageIndex + stageFraction) / progress.stageTotal)
      : 0;

  // `throughputPerS` is zero until this machine has measured its own speed. That is the honest
  // signal for "the number below is an estimate" and it is the only one: an ETA of zero is also a
  // run that is about to finish, which is why the check is on the throughput rather than on the
  // seconds.
  const measured = progress.throughputPerS > 0;

  return (
    <section className="autopilot-progress" aria-label="Progress">
      <header>
        <h3>{progress.stageTitle}</h3>
        <p className="autopilot-progress-position">
          Step {progress.stageIndex + 1} of {progress.stageTotal}
        </p>
      </header>

      <div
        className="autopilot-progress-bar"
        role="progressbar"
        aria-valuemin={0}
        aria-valuemax={100}
        aria-valuenow={Math.round(overall * 100)}
      >
        <span style={{ width: `${Math.round(overall * 100)}%` }} />
      </div>

      <dl className="autopilot-progress-figures">
        <div>
          <dt>Photographs</dt>
          <dd>
            {progress.itemsDone} of {progress.itemsTotal}
          </dd>
        </div>
        <div>
          <dt>Time left</dt>
          <dd>
            {measured
              ? humanDuration(progress.etaS)
              : 'working out how long this will take'}
          </dd>
        </div>
        <div>
          <dt>Speed</dt>
          <dd>{measured ? `${progress.throughputPerS.toFixed(1)}/s` : '—'}</dd>
        </div>
        <div>
          <dt>Spent on AI</dt>
          <dd>${progress.spendUsd.toFixed(2)}</dd>
        </div>
      </dl>

      {progress.warnings.length > 0 ? (
        <ul className="autopilot-progress-warnings">
          {progress.warnings.map((warning) => (
            <li key={warning}>{warning}</li>
          ))}
        </ul>
      ) : null}

      {onCancel ? (
        <button type="button" onClick={onCancel} disabled={progress.cancelled}>
          {progress.cancelled ? 'Stopping…' : 'Stop'}
        </button>
      ) : null}

      {progress.cancelled ? (
        <p className="autopilot-progress-note">
          Finishing the photograph it is on, then stopping. Everything done so far is saved, and
          starting again picks up from there.
        </p>
      ) : null}
    </section>
  );
}
