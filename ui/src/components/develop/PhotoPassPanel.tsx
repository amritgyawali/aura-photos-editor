import { readPassReport, type PassStepReport } from '../../ipc/photoPass';
import type { RecipeDto } from '../../ipc/types';

const STATUS: Record<PassStepReport['outcome'], string> = {
  applied: 'changed', unchanged: 'checked - nothing needed', not_applicable: 'not applicable', protected: 'kept your setting',
};

/** What the hundred-step photo pass found and did on this photograph (ADR-0186). */
export function PhotoPassPanel({ recipe, disabled, onRun }: { recipe: RecipeDto | null; disabled: boolean; onRun: () => void }) {
  const report = readPassReport(recipe);
  const applied = report?.steps.filter(step => step.outcome === 'applied') ?? [];
  return <section className="lr-hint auto-edit-report" aria-label="Photo pass">
    <p><strong>Photo pass</strong> runs after Auto enhance on every imported photo: each step measures one fault
      Auto enhance does not look for (dust, noise, casts, haze, fringes, vignetting, a tilted horizon…) and fixes only
      what it measured. Every change is its own history step; values you set by hand are kept.</p>
    <button type="button" disabled={disabled} onClick={onRun}>{report ? 'Run photo pass again' : 'Run photo pass'}</button>
    {report && <>
      <p role="status">Photo pass ({report.steps.length} of {report.planned} steps built): {report.summary}</p>
      <details open={applied.length > 0}><summary>Steps ({applied.length} changed the photo)</summary>
        <ol className="auto-edit-steps" aria-label="Photo pass steps">{report.steps.map(step => <li key={step.number}>
          <strong>{step.number}. {step.title}</strong> · {STATUS[step.outcome]}
          {step.operations > 0 && step.outcome === 'applied' && ` · ${step.operations} editable operation${step.operations === 1 ? '' : 's'}`}
          {[...step.checks, ...step.changes].map(line => <p key={line}>{line}</p>)}
        </li>)}</ol>
      </details>
    </>}
  </section>;
}
