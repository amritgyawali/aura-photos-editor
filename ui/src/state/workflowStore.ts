import { create } from 'zustand';

import type { StepId } from '../components/workflow/steps';

/**
 * What the five workflow steps currently know about themselves.
 *
 * ## Why this is state and not a computation in the bar
 *
 * The bar reads five answers from four different commands, and a strip that refetched on
 * every render would be a second polling loop underneath panels that already poll. So
 * the *evidence* lives here, written by `WorkflowGuide` at the few moments it actually
 * changes - a project was opened, a run finished, a step was clicked - and read cheaply
 * by anything that draws a dot.
 *
 * ## Why each row is a string hint and a status rather than a number
 *
 * Because a step is done for a reason the photographer should be able to read without
 * opening the panel: "3,140 selected", "a pass has never run". The numbers stay where
 * they belong - in the panels that own them - and what travels here is the one sentence
 * the command was fetched to answer.
 */

export type StepStatus = 'todo' | 'running' | 'done' | 'warn';

export type StepEvidence = {
  status: StepStatus;
  /** The sentence under the step. Empty means the status alone is the whole answer. */
  hint: string;
  /** Whether the evidence has ever been fetched. Absent renders as neutral, not as todo. */
  fetched: boolean;
};

const UNKNOWN: StepEvidence = { status: 'todo', hint: '', fetched: false };

export type WorkflowState = {
  evidence: Record<StepId, StepEvidence>;
  setEvidence: (step: StepId, evidence: StepEvidence) => void;
  /** Opening a different wedding is opening a different set of facts. */
  resetEvidence: () => void;
};

export const useWorkflow = create<WorkflowState>((set) => ({
  evidence: {
    import: UNKNOWN,
    analyze: UNKNOWN,
    cull: UNKNOWN,
    edit: UNKNOWN,
    export: UNKNOWN,
  },

  setEvidence: (step, evidence) =>
    set((state) => ({
      evidence: { ...state.evidence, [step]: evidence },
    })),

  resetEvidence: () =>
    set({
      evidence: {
        import: UNKNOWN,
        analyze: UNKNOWN,
        cull: UNKNOWN,
        edit: UNKNOWN,
        export: UNKNOWN,
      },
    }),
}));
