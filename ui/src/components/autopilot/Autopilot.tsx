import type {
  AutopilotEventDto,
  AutopilotPreflightDto,
  AutopilotProgressDto,
  AutopilotStageDto,
  AutopilotStatusDto,
  AutopilotSummaryDto,
} from '../../ipc/types';
import { PreflightDialog } from './PreflightDialog';
import { ProgressPanel } from './ProgressPanel';
import { RunSummary } from './RunSummary';
import { StageList } from './StageList';

/**
 * PHASE-28. One button: EDIT COMPLETE WEDDING.
 *
 * Pure - everything it shows arrives as props and every action leaves as a callback - so the whole
 * screen is testable without a window, which is the pattern every panel in this product follows.
 *
 * ## The one sentence this panel exists to be honest about
 *
 * Zero-Touch does not mean "AURA does everything and asks nothing". It means AURA does the work
 * unattended *where phase 13's bands allow*, and on this build nothing has been calibrated, so
 * every band is raised one step toward review. The concrete consequence - the product does the
 * work and puts more of it in the review queue than it eventually will - is on the screen before
 * the run starts and again in the summary, rather than left for somebody to infer from a queue
 * with four hundred frames in it.
 *
 * `status.calibrated` is the field that drives it, and it is on the wire for exactly this reason.
 *
 * ## What this panel cannot do
 *
 * It cannot set an autonomy level, a threshold or a per-stage strength, and it cannot reorder the
 * pipeline. The only things a photographer changes here are which steps run and whether the run is
 * unattended; what that unlocks is decided by the bands. ADR-0058 section 8.
 */
export type AutopilotProps = {
  /** The project header, or null while it loads. */
  status: AutopilotStatusDto | null;
  /** Every stage of the newest run. */
  stages: AutopilotStageDto[];
  /** What the run in flight is doing, or null when nothing is running. */
  progress: AutopilotProgressDto | null;
  /** The newest finished run. */
  summary: AutopilotSummaryDto | null;
  /** Everything the governor did. */
  events: AutopilotEventDto[];
  /** The pre-flight, when the dialog is open. */
  preflight: AutopilotPreflightDto | null;
  /** Stage slugs the photographer has switched off. */
  disabled: string[];
  /** Whether the run may act unattended where the bands allow. */
  zeroTouch: boolean;
  /** Open the pre-flight. */
  onPreflight: () => void;
  /** Close the pre-flight without starting. */
  onClosePreflight: () => void;
  /** Start the run. */
  onStart: () => void;
  /** Stop the run. */
  onCancel: () => void;
  /** Switch one stage on or off. */
  onToggleStage: (stage: string, enabled: boolean) => void;
  /** Turn Zero-Touch on or off. */
  onZeroTouch: (on: boolean) => void;
};

export function Autopilot(props: AutopilotProps) {
  const {
    status,
    stages,
    progress,
    summary,
    events,
    preflight,
    disabled,
    zeroTouch,
    onPreflight,
    onClosePreflight,
    onStart,
    onCancel,
    onToggleStage,
    onZeroTouch,
  } = props;

  const running = progress !== null;

  return (
    <div className="autopilot" aria-label="Autopilot">
      <header className="autopilot-header">
        <h2>Edit complete wedding</h2>
        <p>
          Import the RAWs, click once, come back to a delivered gallery. Everything below is
          saved as it goes, so stopping loses nothing.
        </p>
      </header>

      <section className="autopilot-mode" aria-label="How much AURA may do on its own">
        <label>
          <input
            type="checkbox"
            checked={zeroTouch}
            onChange={(event) => onZeroTouch(event.target.checked)}
            disabled={running}
          />
          <span>Zero-Touch: let AURA work while I am away</span>
        </label>

        {status && !status.calibrated ? (
          <p className="autopilot-uncalibrated">
            AURA has not yet learned how often it is right, so it is being careful. It will do the
            work and put more of it in the review queue than it will once it has learned — and it
            will not do anything it cannot take back without asking.
          </p>
        ) : null}
      </section>

      {running ? (
        <ProgressPanel progress={progress} onCancel={onCancel} />
      ) : (
        <div className="autopilot-actions">
          <button type="button" className="is-primary" onClick={onPreflight}>
            Edit complete wedding
          </button>
          {status?.status === 'cancelled' || status?.status === 'failed' ? (
            <p className="autopilot-resume-note">
              The last run stopped part way through. Starting again picks up where it left off.
            </p>
          ) : null}
        </div>
      )}

      <StageList
        stages={stages}
        disabled={disabled}
        current={progress?.stage ?? null}
        onToggle={running ? undefined : onToggleStage}
      />

      {!running ? <RunSummary summary={summary} events={events} /> : null}

      {preflight !== null ? (
        <PreflightDialog
          report={preflight}
          onStart={preflight.permitsStart ? onStart : undefined}
          onCancel={onClosePreflight}
        />
      ) : null}
    </div>
  );
}
