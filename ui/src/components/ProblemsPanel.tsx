import type { ProblemRow } from '../ipc/types';

export type ProblemsPanelProps = {
  problems: ProblemRow[];
};

/**
 * Every file that was set aside, with the reason in plain language. A file that
 * failed silently would be the worse bug, so this panel is never empty by design.
 */
export function ProblemsPanel({ problems }: ProblemsPanelProps): JSX.Element {
  if (problems.length === 0) {
    return (
      <section className="panel" aria-label="Problems">
        <h2>Problems</h2>
        <p className="empty">Nothing was set aside. Every file imported.</p>
      </section>
    );
  }

  return (
    <section className="panel" aria-label="Problems">
      <h2>Problems ({problems.length})</h2>
      <ul className="problem-list">
        {problems.map((problem) => (
          <li key={`${problem.code}:${problem.path}`}>
            <span className="problem-code">{problem.code}</span>
            <span className="problem-message">{problem.message}</span>
            <a
              className="problem-runbook"
              href={`https://aura.app/e/${problem.code}`}
              target="_blank"
              rel="noreferrer"
            >
              What to do
            </a>
          </li>
        ))}
      </ul>
    </section>
  );
}
