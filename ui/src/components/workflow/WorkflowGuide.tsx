import { useCallback, useEffect, useMemo, useState } from 'react';

import { asIpcError, autopilot, cull, delivery, develop, inTauri } from '../../ipc/client';
import { useStore } from '../../state/store';
import { useWorkflow, type StepEvidence } from '../../state/workflowStore';
import { STEPS, type Step, type StepId } from './steps';
import { StepBar, type StepBarRow } from './StepBar';

/**
 * The container behind the step bar: it asks four cheap read-only questions of the
 * commands that already answer them, and writes the answers into the workflow store.
 *
 * There is no sixth command here, and that is the point. Every step's evidence is a
 * status the owning panel already fetches - the cull's, the autopilot summary's, the
 * develop status', the export's - so the bar reports the product's own records rather
 * than re-deriving a second answer to "has this wedding been culled". The rule every
 * phase has followed since phase 05 applies to the front end too.
 *
 * It refreshes at the moments a step can actually change: a project was opened, the
 * window was refocused, a step was clicked. Not on a timer - an autopilot run polls
 * itself where it lives, and a second loop under it would be a doubled read of the
 * same SQLite for nothing.
 */

export type WorkflowGuideProps = {
  /** The step the middle of the application is on, so the bar can mark it. */
  currentStep: StepId;
  /** Switch the stage to this step's tools. */
  onGo: (step: Step) => void;
  /** Bumped by App when an import finishes, so the bar knows without a poll. */
  refreshToken: number;
};

function evidence(status: StepEvidence['status'], hint: string, fetched = true): StepEvidence {
  return { status, hint, fetched };
}

export function WorkflowGuide({
  currentStep,
  onGo,
  refreshToken,
}: WorkflowGuideProps): JSX.Element | null {
  const activeProjectId = useStore((state) => state.activeProjectId);
  const rows = useStore((state) => state.rows);
  const progress = useStore((state) => state.progress);
  const problems = useStore((state) => state.problems);
  const setError = useStore((state) => state.setError);
  const evidenceInStore = useWorkflow((state) => state.evidence);
  const setEvidence = useWorkflow((state) => state.setEvidence);
  const resetEvidence = useWorkflow((state) => state.resetEvidence);
  const [quiet, setQuiet] = useState(false);

  const refresh = useCallback(async () => {
    if (!activeProjectId || !inTauri()) {
      return;
    }
    const projectId = activeProjectId;
    try {
      const [summary, cullStatus, developStatus, exportStatus, manifest] = await Promise.all([
        autopilot.autopilotSummary(projectId),
        cull.cullStatus(projectId),
        develop.developStatus(projectId),
        delivery.exportStatus(projectId),
        delivery.exportManifest(projectId),
      ]);

      // Analyze. The summary is the run's own record; null means a pass has never
      // finished, which is a different thing from a pass that found nothing.
      if (summary === null) {
        setEvidence('analyze', evidence('todo', 'A pass has never run.'));
      } else if (summary.status === 'running') {
        setEvidence('analyze', evidence('running', 'AURA is looking through the wedding.'));
      } else if (summary.status === 'completed') {
        setEvidence(
          'analyze',
          summary.needsReview > 0
            ? evidence('warn', `${summary.needsReview.toLocaleString()} frames want a look.`)
            : evidence('done', 'The whole wedding has been read.'),
        );
      } else if (summary.status === 'completed_degraded') {
        setEvidence('analyze', evidence('warn', 'Finished, with a stage that could not run.'));
      } else {
        setEvidence('analyze', evidence('warn', `The last pass ${summary.status}.`));
      }

      // Cull. `selected > 0` is the only honest completion: a gallery exists.
      if (cullStatus.selected > 0) {
        setEvidence(
          'cull',
          evidence(
            'done',
            `${cullStatus.selected.toLocaleString()} selected of ${cullStatus.photos.toLocaleString()}.`,
          ),
        );
      } else {
        setEvidence('cull', evidence('todo', 'Nothing has been chosen yet.'));
      }

      // Edit. A recipe is what an edit means here - the AI's or a person's.
      if (developStatus.sidecarBehind > 0) {
        setEvidence(
          'edit',
          evidence('warn', `${developStatus.sidecarBehind.toLocaleString()} edits are not written to their files.`),
        );
      } else if (developStatus.withRecipe > 0) {
        setEvidence(
          'edit',
          evidence('done', `${developStatus.withRecipe.toLocaleString()} edited, ${developStatus.fromUser.toLocaleString()} by hand.`),
        );
      } else {
        setEvidence('edit', evidence('todo', 'No photograph has an edit yet.'));
      }

      // Export. A sealed manifest is the delivery; files without one are a partial job.
      if (manifest !== null) {
        setEvidence('export', evidence('done', `Delivered ${exportStatus.written.toLocaleString()} files, verified.`));
      } else if (exportStatus.corrupt > 0) {
        setEvidence('export', evidence('warn', 'A file did not read back. Nothing further was sent.'));
      } else if (exportStatus.requested > 0) {
        setEvidence(
          'export',
          evidence('warn', `${exportStatus.written.toLocaleString()} of ${exportStatus.requested.toLocaleString()} written; the manifest seals only on a complete job.`),
        );
      } else {
        setEvidence('export', evidence('todo', 'Choose a folder and a preset in the Delivery panel.'));
      }
      setQuiet(false);
    } catch (error) {
      // The bar is guidance. A status it cannot read must not also become an error
      // banner over the whole application - the panels themselves still report their
      // own failures where they are actionable.
      const ipc = asIpcError(error);
      setError({ code: ipc.code, message: ipc.message });
      setQuiet(true);
    }
  }, [activeProjectId, setEvidence, setError]);

  // Import needs no command: the rows are the project's, the progress is the pass's,
  // and the problems are already in the store the rest of the shell reads.
  useEffect(() => {
    if (!activeProjectId) {
      resetEvidence();
      // The bar is the navigation now, so it draws with no wedding open - but four
      // of its dots would say "Waiting" about work that cannot be done at all. An
      // absent project is ignorance about the steps, not permission to show them as
      // merely pending: every dot says what it actually needs.
      for (const step of STEPS) {
        if (step.id !== 'import') {
          setEvidence(step.id, evidence('todo', 'Needs a wedding first.'));
        }
      }
      setEvidence(
        'import',
        evidence('todo', 'Create a wedding, then point AURA at your cards.'),
      );
      return;
    }
    if (progress.running) {
      const of = progress.total > 0 ? ` of ${progress.total.toLocaleString()}` : '';
      setEvidence('import', evidence('running', `${progress.done.toLocaleString()}${of} files in.`));
    } else if (problems.length > 0) {
      setEvidence('import', evidence('warn', `${problems.length.toLocaleString()} files had a problem.`));
    } else if (rows.length > 0) {
      setEvidence('import', evidence('done', `${rows.length.toLocaleString()} or more photographs in the library.`));
    } else {
      setEvidence('import', evidence('todo', 'Point AURA at a card or a folder.'));
    }
  }, [activeProjectId, progress, problems, rows, setEvidence, resetEvidence]);

  useEffect(() => {
    void refresh();
  }, [refresh, refreshToken]);

  useEffect(() => {
    const onFocus = (): void => {
      void refresh();
    };
    window.addEventListener('focus', onFocus);
    return () => {
      window.removeEventListener('focus', onFocus);
    };
  }, [refresh]);

  const rowsModel = useMemo(
    () =>
      STEPS.map((step): StepBarRow => ({
        step,
        evidence: evidenceInStore[step.id],
        isHere: step.id === currentStep,
      })),
    [currentStep, evidenceInStore],
  );

  // Without a window there is no catalog to ask. The bar still draws - it is the
  // navigation now, not only guidance - and every dot reads as unknown until a
  // wedding exists, which is the honest state of an empty machine.
  if (!inTauri() || quiet) {
    return null;
  }

  return (
    <StepBar
      rows={rowsModel}
      onGo={(step) => {
        onGo(step);
        void refresh();
      }}
    />
  );
}
