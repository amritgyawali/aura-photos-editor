/**
 * The nine workspaces, and the switch between them.
 *
 * ## Why this exists
 *
 * Until the phases 01 to 30 review, `App.tsx` mounted nine panels and forty-two of the eighty-two
 * UI source files were reachable from `main.tsx` by no import path at all - the whole develop
 * stack, people, story, style, cull, cleanup, camera matching and most of the explain rail. Every
 * one of them had passing tests and every command behind them answered; what was missing was the
 * view that put the two together. `PHASE-01-30-REVIEW.md` section 6.4 called that the single
 * largest gap between "the product is built" and "a photographer can use it".
 *
 * ## Why workspaces rather than one long sidebar
 *
 * Because the panels answer questions asked at different times. Culling happens once, in one
 * sitting, over a whole wedding; developing happens per photograph; delivering happens at the
 * end. A single column holding all of them is a column that is wrong for every one of those
 * moments. The order below is the order the work happens in, which is also the order the
 * autopilot's DAG runs its stages.
 *
 * ## What this component is not
 *
 * It holds no state about a photograph and reaches no command. It is a list and a callback, so
 * the shell can be tested without a Tauri window.
 */
export type WorkspaceId =
  | 'library'
  | 'people'
  | 'story'
  | 'moments'
  | 'cull'
  | 'develop'
  | 'cleanup'
  | 'style'
  | 'camera';

export type WorkspaceNavProps = {
  /** Which workspace is open. */
  active: WorkspaceId;
  /** Switch to another. */
  onSelect: (workspace: WorkspaceId) => void;
  /** Everything but the library needs a wedding open. */
  disabled?: boolean;
};

/** The workspaces, in the order the work happens in. */
export const WORKSPACES: ReadonlyArray<{
  id: WorkspaceId;
  title: string;
  /** What a photographer would come here to do, in their own words. */
  purpose: string;
}> = [
  { id: 'library', title: 'Library', purpose: 'Every photograph, as it was shot.' },
  { id: 'people', title: 'People', purpose: 'Who is in this wedding.' },
  { id: 'story', title: 'Story', purpose: 'The day, as chapters.' },
  { id: 'moments', title: 'Moments', purpose: 'What was shot once, stacked.' },
  { id: 'cull', title: 'Cull', purpose: 'What is being delivered, and why.' },
  { id: 'develop', title: 'Develop', purpose: 'How one photograph looks.' },
  { id: 'cleanup', title: 'Cleanup', purpose: 'What AURA would tidy out of a frame.' },
  { id: 'style', title: 'Your look', purpose: 'What AURA has learned from your own work.' },
  { id: 'camera', title: 'Cameras', purpose: 'Two bodies, one visual result.' },
];

export function WorkspaceNav({ active, onSelect, disabled }: WorkspaceNavProps): JSX.Element {
  return (
    <nav className="workspace-nav" aria-label="Workspaces">
      <ul>
        {WORKSPACES.map((workspace) => (
          <li key={workspace.id}>
            <button
              type="button"
              title={workspace.purpose}
              aria-current={active === workspace.id ? 'page' : undefined}
              className={active === workspace.id ? 'is-active' : undefined}
              disabled={disabled && workspace.id !== 'library'}
              onClick={() => onSelect(workspace.id)}
            >
              {workspace.title}
            </button>
          </li>
        ))}
      </ul>
    </nav>
  );
}
