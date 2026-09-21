import type { ToolId } from './stages';

export type StepId = 'import' | 'analyze' | 'cull' | 'edit' | 'export';

/**
 * The five steps of a wedding, in the order the work happens in.
 *
 * ## Why this exists
 *
 * The product grew one workspace per phase and the review of phases 01 to 30 found the
 * result: every part worked and nothing said what to do first. These five rows are that
 * sentence. Since the middle-of-the-app redesign they are also the navigation: a step
 * opens the tools that belong to it (`stages.ts`) and `defaultTool` is what the stage
 * shows when the step is entered. The step bar is how the photographer moves; nothing
 * is locked - the tools a step offers are only the ones that make sense at that point.
 */

export type Step = {
  id: StepId;
  number: number;
  title: string;
  /** What a photographer comes here to do, in their own words. */
  purpose: string;
  /** The tool the stage opens on. Each step's full tool set is in `STAGE_TOOLS`. */
  defaultTool: ToolId;
};

/** The steps keyed by id: the single source, so a row cannot disagree with itself. */
export const STEPS_BY_ID: Record<StepId, Step> = {
  import: {
    id: 'import',
    number: 1,
    title: 'Import',
    purpose: 'Bring the photographs in from your cards.',
    defaultTool: 'import',
  },
  analyze: {
    id: 'analyze',
    number: 2,
    title: 'Analyze',
    purpose: 'Let AURA look at the wedding: people, story, quality.',
    defaultTool: 'autopilot',
  },
  cull: {
    id: 'cull',
    number: 3,
    title: 'Cull',
    purpose: 'Decide what is being delivered, and why.',
    defaultTool: 'cull',
  },
  edit: {
    id: 'edit',
    number: 4,
    title: 'Edit',
    purpose: 'Develop each photograph - one click, or every slider.',
    defaultTool: 'develop',
  },
  export: {
    id: 'export',
    number: 5,
    title: 'Export',
    purpose: 'One press: edit everything and deliver the files.',
    defaultTool: 'oneClick',
  },
};

/** The steps in the order the work happens in, drawn by the bar. */
export const STEPS: ReadonlyArray<Step> = [
  STEPS_BY_ID['import'],
  STEPS_BY_ID['analyze'],
  STEPS_BY_ID['cull'],
  STEPS_BY_ID['edit'],
  STEPS_BY_ID['export'],
];

export function stepOf(id: StepId): Step {
  // No thrown `undefined`, and the reason is this file's own shape: `StepId` is closed,
  // every key is checked at compile time, and there is no runtime branch to reach.
  // No UI source in this project throws.
  return STEPS_BY_ID[id];
}
