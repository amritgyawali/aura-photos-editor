import type { StepId } from './steps';

/**
 * What each stage holds: the tools, and which of them each step opens.
 *
 * ## Why tools belong to steps rather than to the whole application
 *
 * The old shell mounted thirteen sidebar panels and nine workspaces and asked a first
 * session to work out which of the twenty-two things mattered right now. They did not:
 * curation is meaningless before the cull, delivery before the develop, and the import
 * wizard has nothing to say once the wedding is in. A step now carries only the tools
 * that make sense *at that point in the work*; the middle of the application draws
 * exactly those, and this table is the whole policy. The shell just renders it.
 *
 * ## Why `library` reappears in four of the five steps
 *
 * Because choosing a photograph is part of four of the jobs - seeing what came in,
 * reading the analysis, picking what to develop, checking what a proposal touched -
 * and a view only reachable from one step turns "show me that frame" into a navigation
 * exercise. It is the same mounted view in each, not a copy.
 *
 * ## Why there is a Setup group outside the steps
 *
 * Weddings, cache, hardware, AI keys and the log are about this installation rather
 * than about the wedding's progress. They do not belong to step 3 of five, so they
 * are not offered there; they are always one click away under their own tab.
 */

export type ToolId =
  | 'weddings'
  | 'import'
  | 'library'
  | 'problems'
  | 'autopilot'
  | 'people'
  | 'story'
  | 'moments'
  | 'gallery'
  | 'qc'
  | 'cameras'
  | 'cull'
  | 'curate'
  | 'develop'
  | 'cleanup'
  | 'style'
  | 'oneClick'
  | 'delivery'
  | 'cache'
  | 'hardware'
  | 'ai'
  | 'log';

export type Tool = {
  id: ToolId;
  title: string;
  purpose: string;
};

export const TOOLS: Record<ToolId, Tool> = {
  weddings: { id: 'weddings', title: 'Weddings', purpose: 'The projects on this machine.' },
  import: { id: 'import', title: 'Import', purpose: 'Bring the photographs in from your cards.' },
  library: { id: 'library', title: 'Library', purpose: 'Every photograph, as it was shot.' },
  problems: {
    id: 'problems',
    title: 'Problems',
    purpose: 'Files the library could not read, and why.',
  },
  autopilot: { id: 'autopilot', title: 'Analyze', purpose: 'One button: let AURA read the wedding.' },
  people: { id: 'people', title: 'People', purpose: 'Who is in this wedding.' },
  story: { id: 'story', title: 'Story', purpose: 'The day, as chapters.' },
  moments: { id: 'moments', title: 'Moments', purpose: 'What was shot once, stacked.' },
  gallery: { id: 'gallery', title: 'Consistency', purpose: 'The wedding as one body of work.' },
  qc: { id: 'qc', title: 'Quality check', purpose: 'What AURA found wrong with its own work.' },
  cameras: { id: 'cameras', title: 'Cameras', purpose: 'Two bodies, one visual result.' },
  cull: { id: 'cull', title: 'Cull', purpose: 'What is being delivered, and why.' },
  curate: { id: 'curate', title: 'Album', purpose: 'What AURA proposes after the cull.' },
  develop: { id: 'develop', title: 'Develop', purpose: 'How one photograph looks.' },
  cleanup: { id: 'cleanup', title: 'Cleanup', purpose: 'What AURA would tidy out of a frame.' },
  style: { id: 'style', title: 'Your look', purpose: 'What AURA has learned from your work.' },
  delivery: { id: 'delivery', title: 'Delivery', purpose: 'Write the files and seal the record.' },
  oneClick: {
    id: 'oneClick',
    title: 'Finish everything',
    purpose: 'Analyze, frame, cull, AI-edit and deliver in one press.',
  },
  cache: { id: 'cache', title: 'Cache', purpose: 'Previews, disk budget, and what is stored.' },
  hardware: { id: 'hardware', title: 'Hardware', purpose: 'What this machine can run, and what it refused.' },
  ai: { id: 'ai', title: 'AI provider', purpose: 'The provider behind Auto edit and the cloud tasks.' },
  log: { id: 'log', title: 'Application log', purpose: 'Every click, command and error this session made.' },
};

/** The tools each step offers, in the order they are worth seeing. */
export const STAGE_TOOLS: Record<StepId, ReadonlyArray<Tool>> = {
  import: [TOOLS['weddings'], TOOLS['import'], TOOLS['oneClick'], TOOLS['library'], TOOLS['problems']],
  // The one button is reachable from the step where the work starts as well as the
  // step it ends in: "import and then finish everything" is one intent, and making
  // the photographer click Export first to find it splits that intent in two.
  analyze: [
    TOOLS['autopilot'],
    TOOLS['people'],
    TOOLS['story'],
    TOOLS['moments'],
    TOOLS['gallery'],
    TOOLS['qc'],
    TOOLS['cameras'],
    TOOLS['library'],
  ],
  cull: [TOOLS['cull'], TOOLS['curate'], TOOLS['library']],
  edit: [TOOLS['develop'], TOOLS['cleanup'], TOOLS['style'], TOOLS['library']],
  export: [TOOLS['oneClick'], TOOLS['delivery'], TOOLS['library']],
};

/** The installation's own tools, offered on every step rather than belonging to one. */
export const SYSTEM_TOOLS: ReadonlyArray<Tool> = [
  TOOLS['weddings'],
  TOOLS['cache'],
  TOOLS['hardware'],
  TOOLS['ai'],
  TOOLS['log'],
];

/** The tool a step opens with when the photographer jumps to it. */
export const DEFAULT_TOOL: Record<StepId, ToolId> = {
  import: 'import',
  analyze: 'autopilot',
  cull: 'cull',
  edit: 'develop',
  export: 'delivery',
};

/** Is this tool part of the always-available system group? */
export function isSystemTool(tool: ToolId): boolean {
  return SYSTEM_TOOLS.some((row) => row.id === tool);
}
