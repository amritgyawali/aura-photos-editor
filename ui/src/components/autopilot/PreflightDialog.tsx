import type { AutopilotPreflightDto } from '../../ipc/types';

/**
 * PHASE-28. What AURA checked before it started a two-hour job.
 *
 * Pure - a report and two callbacks in, nothing fetched.
 *
 * ## Why every row carries a sentence
 *
 * Section 2.1 asks the pre-flight to "fail fast with actionable messages". A row that said only
 * "Disk space — blocked" sends a photographer to a runbook to find out how many gigabytes they
 * need; the row this renders says "This run needs about 48.0 GB and there is 12.0 GB free. Free up
 * 36.0 GB and start again."
 *
 * The sentences are built in Rust rather than here, so the pre-flight a photographer reads and the
 * one a support bundle carries say the same thing.
 *
 * ## Why the calibration row always warns on this build
 *
 * Phase 13's confidences have not been fitted, so every band is raised one step toward review. The
 * consequence for somebody about to press a Zero-Touch button is concrete - AURA will do the work
 * and ask about more of it than it eventually will - and it belongs *before* the run rather than in
 * a summary two hours later.
 */
export type PreflightDialogProps = {
  /** The report, or null while it loads. */
  report: AutopilotPreflightDto | null;
  /** Start the run. Absent when the report blocks. */
  onStart?: () => void;
  /** Close without starting. */
  onCancel: () => void;
};

/** Bytes as the words a person uses. */
export function humanBytes(bytes: number): string {
  if (bytes >= 1_000_000_000) return `${(bytes / 1_000_000_000).toFixed(1)} GB`;
  if (bytes >= 1_000_000) return `${(bytes / 1_000_000).toFixed(0)} MB`;
  return `${bytes} B`;
}

/** Milliseconds as the words a person uses. */
export function humanEstimate(ms: number): string {
  const hours = ms / 3_600_000;
  if (hours >= 1) return `about ${hours.toFixed(1)} hours`;
  const minutes = Math.max(1, Math.round(ms / 60_000));
  return `about ${minutes} minutes`;
}

export function PreflightDialog({ report, onStart, onCancel }: PreflightDialogProps) {
  if (!report) {
    return (
      <div className="autopilot-preflight is-loading" role="dialog" aria-label="Before AURA starts">
        <p>Checking…</p>
      </div>
    );
  }

  const blocked = !report.permitsStart;

  return (
    <div className="autopilot-preflight" role="dialog" aria-label="Before AURA starts">
      <header>
        <h2>Before AURA starts</h2>
        <p>
          {report.images} photographs, {humanEstimate(report.estimatedMs)}, about{' '}
          {humanBytes(report.estimatedOutputBytes)} written.
        </p>
      </header>

      <ul className="autopilot-preflight-rows">
        {report.rows.map((row) => (
          <li key={row.check} className={`is-${row.verdict}`} data-check={row.check}>
            <span className="autopilot-preflight-title">{row.title}</span>
            <span className="autopilot-preflight-detail">{row.detail}</span>
          </li>
        ))}
      </ul>

      <footer>
        {blocked ? (
          <p className="autopilot-preflight-blocked">
            AURA has not started. Fix what is marked above and try again.
          </p>
        ) : null}
        <button type="button" onClick={onCancel}>
          {blocked ? 'Close' : 'Not now'}
        </button>
        {!blocked && onStart ? (
          <button type="button" className="is-primary" onClick={onStart}>
            Edit complete wedding
          </button>
        ) : null}
      </footer>
    </div>
  );
}
