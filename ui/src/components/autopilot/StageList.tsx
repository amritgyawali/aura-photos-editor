import type { AutopilotStageDto } from '../../ipc/types';

/**
 * PHASE-28. The twenty-five steps of a wedding, and what happened to each one.
 *
 * Pure - a list and a callback in, nothing fetched - like every view beside it.
 *
 * ## Why a skipped step never looks like a finished one
 *
 * A step that did not run means one of several completely different things: the photographer
 * switched it off, this release does not have it, its model is untrained, or AURA is not confident
 * enough to do it unattended. Only the first of those is fine, and the other four are the reason
 * somebody opens this list at one in the morning.
 *
 * So a skipped row renders in its own style, carries the sentence the orchestrator gave for it, and
 * is never drawn with the tick a completed row gets. Phase 27's rule - clean and skipped are
 * different values - one level up, where the subject is a whole step rather than one inspection.
 *
 * ## Why the checklist and the outcome are the same list
 *
 * Before a run, `stages` is empty and this renders the plan with every row switchable. During and
 * after one, the same rows carry outcomes. A separate "checklist" component and "results"
 * component would be two places for the stage vocabulary to drift, and a photographer would have to
 * learn that the thing they unticked and the thing that says "skipped" are the same row.
 */
export type StageListProps = {
  /** Every stage of the newest run, or an empty list before the first one. */
  stages: AutopilotStageDto[];
  /** Stage slugs the photographer has switched off. */
  disabled: string[];
  /** Switch one stage on or off. Absent while a run is in flight. */
  onToggle?: (stage: string, enabled: boolean) => void;
  /** The stage running right now, for the highlight. */
  current?: string | null;
};

/**
 * The plan, in pipeline order, with the words a photographer uses.
 *
 * The slug is the wire's and the label is the panel's - phase 09's rule about a reason storing its
 * code rather than its sentence, at the other end. The four stages a wedding cannot be delivered
 * without carry `required`, and this component renders their switch as absent rather than as
 * disabled: a control that looks like a control and does nothing is worse than no control.
 */
export const PLAN: Array<{ slug: string; label: string; required?: boolean }> = [
  { slug: 'ingest', label: 'Importing', required: true },
  { slug: 'previews', label: 'Building previews', required: true },
  { slug: 'embed', label: 'Looking at every photograph', required: true },
  { slug: 'faces', label: 'Finding people' },
  { slug: 'story', label: 'Working out the day' },
  { slug: 'moments', label: 'Grouping what you shot once' },
  { slug: 'integrity', label: 'Checking focus and eyes' },
  { slug: 'emotion', label: 'Reading the moment' },
  { slug: 'composition', label: 'Reading the framing' },
  { slug: 'cull', label: 'Choosing the gallery', required: true },
  { slug: 'masks', label: 'Finding regions' },
  { slug: 'tone', label: 'Judging the light' },
  { slug: 'colour', label: 'Grading' },
  { slug: 'style', label: 'Applying your look' },
  { slug: 'local_light', label: 'Shaping the light' },
  { slug: 'retouch', label: 'Retouching skin' },
  { slug: 'micro', label: 'Hair, teeth and eyes' },
  { slug: 'restoration', label: 'Cleaning up noise' },
  { slug: 'geometry', label: 'Straightening and cropping' },
  { slug: 'cleanup', label: 'Removing distractions' },
  { slug: 'camera_match', label: 'Matching your cameras' },
  { slug: 'consistency', label: 'Making it one gallery' },
  { slug: 'qc', label: 'Checking the work' },
  { slug: 'curation', label: 'Building the album' },
  { slug: 'export', label: 'Writing the files' },
];

/** What a row's outcome is called in the panel. */
function outcomeLabel(row: AutopilotStageDto | undefined): string {
  if (!row) return '';
  switch (row.outcome) {
    case 'completed':
      return `${row.itemsDone} done`;
    case 'partial':
      return `${row.itemsDone} of ${row.itemsTotal} done`;
    case 'failed':
      return 'could not finish';
    case 'skipped':
      return row.skipText ?? 'not run';
    case 'running':
      return 'working';
    default:
      return row.outcome;
  }
}

/**
 * Whether a row is a step that did not do what it was meant to.
 *
 * A step the photographer switched off is not one of those, which is the whole distinction: the
 * skip cause decides, not the outcome.
 */
function isDegraded(row: AutopilotStageDto | undefined): boolean {
  if (!row) return false;
  if (row.outcome === 'failed' || row.outcome === 'partial') return true;
  return row.outcome === 'skipped' && row.skipCause !== 'turned_off';
}

export function StageList({ stages, disabled, onToggle, current }: StageListProps) {
  const byStage = new Map(stages.map((row) => [row.stage, row]));

  return (
    <ol className="autopilot-stages" aria-label="What AURA will do">
      {PLAN.map(({ slug, label, required }) => {
        const row = byStage.get(slug);
        const off = disabled.includes(slug);
        const degraded = isDegraded(row);
        const done = row?.outcome === 'completed';
        const running = current === slug;

        const classes = ['autopilot-stage'];
        if (off) classes.push('is-off');
        if (done) classes.push('is-done');
        if (degraded) classes.push('is-degraded');
        if (running) classes.push('is-running');
        if (row?.skipCause === 'awaiting_review') classes.push('is-held');

        return (
          <li key={slug} className={classes.join(' ')} data-stage={slug}>
            {required || !onToggle ? (
              <span className="autopilot-stage-fixed" aria-hidden="true" />
            ) : (
              <label>
                <input
                  type="checkbox"
                  checked={!off}
                  onChange={(event) => onToggle(slug, event.target.checked)}
                  aria-label={label}
                />
              </label>
            )}
            <span className="autopilot-stage-label">{label}</span>
            {row ? (
              <span
                className={degraded ? 'autopilot-stage-outcome is-degraded' : 'autopilot-stage-outcome'}
              >
                {outcomeLabel(row)}
              </span>
            ) : null}
            {row?.verdict === 'act_and_review' ? (
              <span className="autopilot-stage-review" title="These decisions go in the review queue">
                worth a look
              </span>
            ) : null}
          </li>
        );
      })}
    </ol>
  );
}
