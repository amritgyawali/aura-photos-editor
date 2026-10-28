import { readPassReport, type PassCategory, type PassStepReport } from '../../ipc/photoPass';
import type { RecipeDto } from '../../ipc/types';

const STATUS: Record<PassStepReport['outcome'], string> = {
  applied: 'changed', unchanged: 'checked - nothing needed', not_applicable: 'not applicable', protected: 'kept your setting',
};
const GROUPS: [PassCategory, string][] = [['capture', 'Capture, light & colour'], ['face', 'Faces & skin']];

/** What the hundred-step photo pass found and did on this photograph (ADR-0186). */
export function PhotoPassPanel({ recipe, disabled, onRun }: { recipe: RecipeDto | null; disabled: boolean; onRun: () => void }) {
  const report = readPassReport(recipe);
  return <section className="lr-hint auto-edit-report" aria-label="Photo pass">
    <p><strong>Photo pass</strong> runs after Auto enhance on every imported photo: each step measures one fault
      Auto enhance does not look for (dust, noise, casts, haze, fringes, a tilted horizon…) and then checks every face
      on the edited result - skin colour after the grade, pores, under-eyes, teeth, ceremonial marks - fixing only
      what it measured. Every change is its own history step; values you set by hand are kept.</p>
    <button type="button" disabled={disabled} onClick={onRun}>{report ? 'Run photo pass again' : 'Run photo pass'}</button>
    {report && <>
      <p role="status">Photo pass ({report.steps.length} of {report.planned} steps built): {report.summary}</p>
      {GROUPS.map(([category, title]) => {
        const steps = report.steps.filter(step => step.category === category);
        if (!steps.length) return null;
        const changed = steps.filter(step => step.outcome === 'applied').length;
        return <details key={category} open={changed > 0}><summary>{title} ({changed} of {steps.length} changed the photo)</summary>
          <ol className="auto-edit-steps" aria-label={`${title} steps`}>{steps.map(step => <li key={step.number}>
            <strong>{step.number}. {step.title}</strong> · {STATUS[step.outcome]}
            {step.operations > 0 && step.outcome === 'applied' && ` · ${step.operations} editable operation${step.operations === 1 ? '' : 's'}`}
            {[...step.checks, ...step.changes].map(line => <p key={line}>{line}</p>)}
          </li>)}</ol>
        </details>;
      })}
    </>}
  </section>;
}
