import type { Step } from './steps';
import type { StepEvidence } from '../../state/workflowStore';

/**
 * The numbered strip: where a wedding is, and what is waiting at each step.
 *
 * Pure view by this project's rule - props in, one callback out, no store and no
 * command - so it is testable without a window and the container can own fetching.
 *
 * A status is never carried by color alone: every dot has the word beside it for a
 * screen reader and for a photographer on a bad panel, and the hint line says the
 * *reason*, which is the difference between guidance and decoration.
 */

export type StepBarRow = {
  step: Step;
  evidence: StepEvidence;
  /** This is the workspace the photographer is looking at right now. */
  isHere: boolean;
};

export type StepBarProps = {
  rows: ReadonlyArray<StepBarRow>;
  /** Jump to where the step's work happens. */
  onGo: (step: Step) => void;
};

const STATUS_WORD: Record<StepEvidence['status'], string> = {
  todo: 'Waiting',
  running: 'Running',
  done: 'Done',
  warn: 'Needs you',
};

export function StepBar({ rows, onGo }: StepBarProps): JSX.Element {
  return (
    <nav className="step-bar" aria-label="Workflow">
      <ol>
        {rows.map((row) => {
          const { step, evidence, isHere } = row;
          const classes = [
            'step-bar-item',
            `is-${evidence.status}`,
            isHere ? 'is-here' : null,
          ]
            .filter(Boolean)
            .join(' ');
          return (
            <li key={step.id} className={classes}>
              <button
                type="button"
                className="step-bar-go"
                title={step.purpose}
                onClick={() => {
                  onGo(step);
                }}
              >
                <span className="step-bar-number" aria-hidden="true">
                  {step.number}
                </span>
                <span className="step-bar-text">
                  <span className="step-bar-title">{step.title}</span>
                  <span className="step-bar-hint">
                    {evidence.hint.length > 0 ? evidence.hint : step.purpose}
                  </span>
                </span>
                <span className={`step-bar-status is-${evidence.status}`}>
                  <span className="step-bar-dot" aria-hidden="true" />
                  {STATUS_WORD[evidence.status]}
                </span>
              </button>
            </li>
          );
        })}
      </ol>
    </nav>
  );
}
