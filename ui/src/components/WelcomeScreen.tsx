import { STEPS } from './workflow/steps';
import { ImportWizard } from './ImportWizard';

/**
 * What a photographer sees before there is a wedding.
 *
 * The review of phases 01 to 30 spent its longest paragraph on panels nobody could reach,
 * and the fix was a nav rail. This is the other half of the same complaint: with no
 * project open the main area said one sentence, and a first session had no way to learn
 * that the product is a five-step process. The screen explains the steps in order, with
 * the number of the step you would click to do it visible on the row, and it stops
 * appearing the moment a project exists - the work itself becomes the guide.
 */

export type WelcomeScreenProps = {
  onImport?: (roots: string[]) => void;
  busy?: boolean;
  onCreateProject: (name: string) => void;
  onFirstRun: () => void;
  /** Whether this install has ever answered the AI provider question, for the footnote. */
  aiAnswered: boolean;
};

export function WelcomeScreen({
  onCreateProject,
  onFirstRun,
  aiAnswered,
  onImport,
  busy = false,
}: WelcomeScreenProps): JSX.Element {
  return (
    <section className="welcome" aria-labelledby="welcome-title">
      <h1 id="welcome-title">Choose your photos. AURA takes it from here.</h1>
      <p className="welcome-lead">
        AURA reads your photographs, helps you choose, edits them, and delivers the files.
        Select photos or a folder to start automatic processing.
      </p>
      {onImport && <ImportWizard automatic disabled={busy} running={false} done={0} total={0} onStart={onImport} onCancel={() => undefined} />}
      <ol className="welcome-steps">
        {STEPS.map((step) => (
          <li key={step.id}>
            <span className="welcome-step-number" aria-hidden="true">
              {step.number}
            </span>
            <span className="welcome-step-body">
              <strong>{step.title}</strong>
              <span>{step.purpose}</span>
            </span>
          </li>
        ))}
      </ol>
      <div className="welcome-actions">
        <button
          type="button"
          className="btn btn-primary"
          onClick={() => {
            onCreateProject('Untitled wedding');
          }}
        >
          Create a wedding
        </button>
        <button type="button" className="btn" onClick={onFirstRun}>
          Show me how it works
        </button>
      </div>
      {!aiAnswered ? (
        <p className="welcome-note">
          Local automatic editing is ready without setup. Add a vision provider in AI provider for scene-aware recommendations.
        </p>
      ) : null}
    </section>
  );
}
