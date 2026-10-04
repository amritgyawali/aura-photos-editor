import { useCallback, useEffect, useState } from 'react';

import { STEPS_BY_ID, STEPS, type StepId } from './workflow/steps';

/**
 * The first-run tour, and nothing more.
 *
 * Five slides - one per step, reusing the exact sentences the bar and the welcome
 * screen use, so there is one description of the workflow rather than three that
 * drift apart. It is deliberately dumb: local state, one localStorage flag read and
 * written inside a try/catch (a private window that refuses storage simply means the
 * tour shows again, which is a smaller failure than a tour that can never be opened
 * again), no routing, and it unmounts the moment a project exists because the app
 * itself has become the thing being looked at.
 */

export type OnboardingOverlayProps = {
  onDismiss: () => void;
};

const SEEN_KEY = 'aura.onboarded';

function hasSeenTour(): boolean {
  try {
    return window.localStorage.getItem(SEEN_KEY) === '1';
  } catch {
    return false;
  }
}

/** Whether the tour should be offered. App checks this once, on the empty state. */
export function tourIsOpenable(): boolean {
  return !hasSeenTour();
}

function markSeen(): void {
  try {
    window.localStorage.setItem(SEEN_KEY, '1');
  } catch {
    // A storage that will not take the flag is a tour that repeats. Fine.
  }
}

/** The slide order, named by id so indexing can never fall off the end. */
const ORDER: readonly StepId[] = ['import', 'analyze', 'cull', 'edit', 'export'];

export function OnboardingOverlay({ onDismiss }: OnboardingOverlayProps): JSX.Element {
  const [index, setIndex] = useState(0);
  const last = ORDER.length - 1;
  const stepId: StepId = ORDER[Math.min(Math.max(index, 0), last)] ?? 'import';
  const step = STEPS_BY_ID[stepId];

  const finish = useCallback(() => {
    markSeen();
    onDismiss();
  }, [onDismiss]);

  useEffect(() => {
    const onKey = (event: KeyboardEvent): void => {
      if (event.key === 'Escape') {
        finish();
      }
    };
    window.addEventListener('keydown', onKey);
    return () => {
      window.removeEventListener('keydown', onKey);
    };
  }, [finish]);

  return (
    <div className="tour-scrim" role="dialog" aria-modal="true" aria-label="How AURA works">
      <div className="tour-card">
        <p className="tour-kicker">
          Step {step.number} of {STEPS.length}
        </p>
        <h2>{step.title}</h2>
        <p className="tour-body">{TOUR_COPY[stepId]}</p>
        <div className="tour-dots" aria-hidden="true">
          {STEPS.map((row, dot) => (
            <span key={row.id} className={dot === index ? 'is-on' : undefined} />
          ))}
        </div>
        <div className="tour-actions">
          <button type="button" className="tour-skip" onClick={finish}>
            Skip the tour
          </button>
          <div className="tour-nav">
            <button
              type="button"
              className="btn"
              disabled={index === 0}
              onClick={() => {
                setIndex((current) => Math.max(0, current - 1));
              }}
            >
              Back
            </button>
            {index === last ? (
              <button type="button" className="btn btn-primary" onClick={finish}>
                Start working
              </button>
            ) : (
              <button
                type="button"
                className="btn btn-primary"
                onClick={() => {
                  setIndex((current) => Math.min(last, current + 1));
                }}
              >
                Next
              </button>
            )}
          </div>
        </div>
      </div>
    </div>
  );
}

/** One paragraph per step, in the product's own words, from each panel's purpose. */
const TOUR_COPY: Record<StepId, string> = {
  import:
    'In the sidebar, name your wedding and add the folder or card it lives on. AURA reads the RAW files and builds previews as it goes; a file already in the catalog is left exactly where it was, so re-importing a card is safe.',
  analyze:
    'The Autopilot runs the whole pipeline in one go: faces and people, the day broken into chapters, bursts and duplicates, quality marks, emotion and composition, and a gallery-normalizing pass so the wedding looks like one body of work. You can watch it run, or leave and come back.',
  cull:
    'Cull proposes what to deliver from what the analysis found, with a reason beside every keep and every rejection. Guarantees outrank taste: a must-have moment stays even when a slider argues otherwise, and your own manual keeps and removes are never overwritten.',
  edit:
    'Develop works one photograph at a time. Auto edit sends the frame to your configured provider and writes back an honest starting point - or, with no provider, the local reference grade. Every slider stays yours: a value a person set is never overwritten by a later pass.',
  export:
    'Delivery writes the files. Pick a preset and a folder, preview the names before a byte is written, and AURA renders, writes, reads every file back, hashes it, and seals a manifest that can never be edited afterwards. A corrupt read stops the job rather than continuing.',
};
