import { useMemo } from 'react';

import type {
  DevelopParamDto,
  HistoryDto,
  RecipeDto,
  RenderCapsDto,
  RenderDto,
} from '../../ipc/types';

/**
 * PHASE-14. The develop panel.
 *
 * Three rules this component keeps, and each of them is a rule the product would otherwise
 * break somewhere a photographer can see:
 *
 * 1. **A protected control is marked, and the mark says why.** A parameter a person set
 *    carries a dot and a title attribute. Section 6.4 says an automated pass never
 *    overwrites one; this is where that promise becomes visible rather than merely true.
 * 2. **A skipped stage is a caveat, not a silence.** Every note whose `isCaveat` is true is
 *    rendered. A mask that could not be generated, a lens profile that does not exist, an
 *    operator a later phase ships - the photographer is told, once, in plain words.
 * 3. **The panel says which engine drew this.** `backend` and the degradation message are
 *    in the header, because a build with no GPU is slower in a way somebody will otherwise
 *    report as a bug.
 */

/** What the panel needs. Everything comes from the IPC surface; nothing is derived here. */
export type DevelopPanelProps = {
  /** The current edit. */
  recipe: RecipeDto;
  /** The rendered proxy, or `null` while one is in flight. */
  render: RenderDto | null;
  /** The photograph's history. */
  history: HistoryDto;
  /** What this machine's renderer can do. */
  caps: RenderCapsDto;
  /** Change one parameter. */
  onSetParam: (path: string, value: unknown) => void;
  /** Undo, redo, or one of the two resets. */
  onHistory: (action: 'undo' | 'redo' | 'reset_original' | 'reset_ai') => void;
};

/** The controls the panel shows, in the order a photographer works in. */
const CONTROL_ORDER = [
  'global.exposure',
  'global.contrast',
  'global.temperature',
  'global.tint',
  'global.highlights',
  'global.shadows',
  'global.whites',
  'global.blacks',
  'global.clarity',
  'global.texture',
  'global.dehaze',
  'global.vibrance',
  'global.saturation',
  'global.sharpen.amount',
  'global.noise.luminance',
  'global.noise.colour',
];

/** A human label for a dotted path. */
function label(path: string): string {
  const leaf = path.split('.').pop() ?? path;
  const words = leaf.replace(/_/g, ' ');
  return words.charAt(0).toUpperCase() + words.slice(1);
}

/** The data URL for a rendered proxy. Built here so the DTO stays free of presentation. */
function dataUrl(render: RenderDto): string {
  return `data:image/rgb;base64,${render.rgbBase64}`;
}

export function DevelopPanel({
  recipe,
  render,
  history,
  caps,
  onSetParam,
  onHistory,
}: DevelopPanelProps) {
  const controls = useMemo(() => {
    const byPath = new Map<string, DevelopParamDto>();
    for (const param of recipe.params) {
      byPath.set(param.path, param);
    }
    return CONTROL_ORDER.map((path) => byPath.get(path)).filter(
      (param): param is DevelopParamDto => param !== undefined,
    );
  }, [recipe.params]);

  const caveats = (render?.notes ?? []).filter((note) => note.isCaveat);

  return (
    <section className="develop-panel" aria-label="Develop">
      <header className="develop-header">
        <h2>Develop</h2>
        <p className="develop-provenance">
          {recipe.source === 'user'
            ? 'Your edit'
            : `AURA suggested this, ${Math.round(recipe.confidence * 100)}% confident`}
        </p>
        <p className="develop-engine" data-testid="develop-engine">
          Rendered on the {caps.backend === 'cpu' ? 'processor' : caps.backend} · {caps.engine}
        </p>
        {caps.degradationMessage ? (
          <p className="develop-degradation" role="status">
            {caps.degradationMessage}
          </p>
        ) : null}
      </header>

      <div className="develop-viewer">
        {render ? (
          <img
            src={dataUrl(render)}
            width={render.width}
            height={render.height}
            alt="The photograph as this edit renders it"
          />
        ) : (
          <p className="develop-pending">Rendering…</p>
        )}
      </div>

      {caveats.length > 0 ? (
        <ul className="develop-caveats" aria-label="What this render left out">
          {caveats.map((note) => (
            <li key={`${note.stage}:${note.reason}`}>
              <strong>{label(note.stage)}</strong>: {caveatText(note.reason)}
              {note.detail ? ` (${note.detail})` : ''}
            </li>
          ))}
        </ul>
      ) : null}

      <ol className="develop-controls">
        {controls.map((param) => (
          <li key={param.path} className="develop-control">
            <label htmlFor={`control-${param.path}`}>
              {label(param.path)}
              {param.protected ? (
                <span
                  className="develop-protected"
                  title="You set this. AURA will not change it."
                  aria-label="You set this. AURA will not change it."
                >
                  ●
                </span>
              ) : null}
            </label>
            <input
              id={`control-${param.path}`}
              type="number"
              value={String(param.value ?? '')}
              onChange={(event) => onSetParam(param.path, Number(event.target.value))}
            />
          </li>
        ))}
      </ol>

      <div className="develop-history">
        <button type="button" disabled={!history.canUndo} onClick={() => onHistory('undo')}>
          Undo
        </button>
        <button type="button" disabled={!history.canRedo} onClick={() => onHistory('redo')}>
          Redo
        </button>
        <button type="button" onClick={() => onHistory('reset_original')}>
          Reset to original
        </button>
        <button
          type="button"
          disabled={!history.hasAiSuggestion}
          onClick={() => onHistory('reset_ai')}
        >
          Reset to AI suggestion
        </button>
      </div>

      <ol className="develop-steps" aria-label="Edit history">
        {history.entries
          .slice()
          .reverse()
          .map((entry) => (
            <li key={entry.seq}>
              <span className="develop-step-label">{entry.label}</span>
              <span className="develop-step-source">{entry.source}</span>
            </li>
          ))}
      </ol>

      <footer className="develop-footer">
        {/* The original is never modified. Said once, in the panel that edits it. */}
        <p>Your RAW files are never changed. Every edit here is a note beside them.</p>
      </footer>
    </section>
  );
}

/** Plain words for a skip reason. The vocabulary is the backend's; the sentences are here. */
function caveatText(reason: string): string {
  switch (reason) {
    case 'mask_generator_absent':
      return 'this version cannot work out that mask yet, so it was not applied';
    case 'operator_absent':
      return 'this version does not have that retouch tool yet';
    case 'restoration_absent':
      return 'this version does not have that restoration model yet';
    case 'geometry_absent':
      return 'this version cannot apply that geometry correction yet';
    case 'lens_profile_absent':
      return 'AURA has no profile for this lens, so the correction was skipped';
    case 'camera_profile_absent':
      return 'AURA has no colour profile for this camera, so a neutral one was used';
    case 'interactive_path':
      return 'skipped while you are adjusting; it is applied when you zoom in or export';
    default:
      return reason;
  }
}
