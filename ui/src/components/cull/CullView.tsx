import { useCallback, useEffect, useState } from 'react';

import { asIpcError, cull as cullApi } from '../../ipc/client';
import type { CullStatusDto, DecisionDto, SelectionDto } from '../../ipc/types';

import { CoveragePanel } from './CoveragePanel';
import { RejectReasons } from './RejectReasons';
import { SizeSlider } from './SizeSlider';

/**
 * PHASE-12's culling view.
 *
 * Four things on one screen, in the order a photographer needs them: what the
 * gallery guarantees, how big it is, what is in it, and why any one photograph
 * is or is not.
 *
 * ## The header says what the gallery was chosen *from*
 *
 * Before anything else. A gallery culled from 60 % of a wedding is a gallery
 * with a four-hour hole in it that looks exactly like a gallery with a decision
 * in it, and `CullStatusDto.coverage` is the only number that tells them apart.
 *
 * ## Nothing on this screen deletes anything
 *
 * There is no delete, no move to trash, no export and no upload. A rejection is
 * a row and every one of them is one click from being overturned. Phase 14 edits
 * the survivors, phase 27 swaps in runner-ups, phase 29 builds albums and phase
 * 30 delivers.
 *
 * ## The mode switcher cannot weaken a guarantee
 *
 * Section 6.4, and the switcher says so in its own help text - because a control
 * labelled "Aggressive" invites exactly the fear that it might.
 */

const MODES: Array<{ id: string; label: string; help: string }> = [
  {
    id: 'conservative',
    label: 'Conservative',
    help: 'Keeps more and flags more of what it is unsure about.',
  },
  { id: 'balanced', label: 'Balanced', help: 'The default.' },
  {
    id: 'aggressive',
    label: 'Aggressive',
    help: 'A tighter gallery. It still cannot drop a part of the wedding AURA guarantees.',
  },
];

type LoadState =
  | { kind: 'loading' }
  | { kind: 'never' }
  | { kind: 'ready'; status: CullStatusDto; selection: SelectionDto }
  | { kind: 'error'; message: string };

export type CullViewProps = {
  projectId: string;
  /** Called when the photographer opens one photograph. */
  onOpenImage?: (photoId: string) => void;
};

export function CullView({ projectId, onOpenImage }: CullViewProps) {
  const [state, setState] = useState<LoadState>({ kind: 'loading' });
  const [busy, setBusy] = useState(false);
  const [decision, setDecision] = useState<DecisionDto | null>(null);
  const [focused, setFocused] = useState<string | null>(null);

  const refresh = useCallback(async () => {
    try {
      const [status, selection] = await Promise.all([
        cullApi.cullStatus(projectId),
        cullApi.gallery(projectId),
      ]);
      // Null is "nobody has decided", never "deliver nothing".
      setState(selection ? { kind: 'ready', status, selection } : { kind: 'never' });
    } catch (error) {
      setState({ kind: 'error', message: asIpcError(error).message });
    }
  }, [projectId]);

  useEffect(() => {
    void refresh();
  }, [refresh]);

  const runCull = useCallback(async () => {
    setBusy(true);
    try {
      await cullApi.cullProject({ projectId });
      await refresh();
    } catch (error) {
      setState({ kind: 'error', message: asIpcError(error).message });
    } finally {
      setBusy(false);
    }
  }, [projectId, refresh]);

  const resize = useCallback(
    async (target: number) => {
      setBusy(true);
      try {
        await cullApi.resizeGallery({ projectId, target });
        await refresh();
      } catch (error) {
        setState({ kind: 'error', message: asIpcError(error).message });
      } finally {
        setBusy(false);
      }
    },
    [projectId, refresh],
  );

  const setMode = useCallback(
    async (mode: string) => {
      setBusy(true);
      try {
        await cullApi.setCullMode({ projectId, mode });
        await refresh();
      } catch (error) {
        setState({ kind: 'error', message: asIpcError(error).message });
      } finally {
        setBusy(false);
      }
    },
    [projectId, refresh],
  );

  const open = useCallback(
    async (photoId: string) => {
      setFocused(photoId);
      onOpenImage?.(photoId);
      try {
        setDecision(await cullApi.imageDecision(photoId));
      } catch (error) {
        setState({ kind: 'error', message: asIpcError(error).message });
      }
    },
    [onOpenImage],
  );

  const override = useCallback(
    async (action: 'keep' | 'reject' | 'clear') => {
      if (!focused) {
        return;
      }
      setBusy(true);
      try {
        setDecision(await cullApi.overrideDecision({ photoId: focused, action }));
        await refresh();
      } catch (error) {
        setState({ kind: 'error', message: asIpcError(error).message });
      } finally {
        setBusy(false);
      }
    },
    [focused, refresh],
  );

  if (state.kind === 'loading') {
    return <section className="cull-view">Reading this wedding…</section>;
  }
  if (state.kind === 'error') {
    return (
      <section className="cull-view cull-view--error" role="alert">
        {state.message}
      </section>
    );
  }
  if (state.kind === 'never') {
    return (
      <section className="cull-view cull-view--never">
        <p>
          AURA has not chosen a gallery for this wedding yet. That is not the same as an
          empty gallery.
        </p>
        <button type="button" onClick={() => void runCull()} disabled={busy}>
          Choose the gallery
        </button>
      </section>
    );
  }

  const { status, selection } = state;

  return (
    <section className="cull-view" aria-label="The gallery">
      <header className="cull-view__header">
        <h1>
          {selection.actualCount} photographs, chosen from {status.eligible}
        </h1>
        <p className="cull-view__provenance">
          Run {selection.deterministicHash}. Nothing here has been deleted; every
          photograph is one click from being put back.
        </p>
      </header>

      <fieldset className="cull-view__modes" disabled={busy}>
        <legend>How much should AURA decide?</legend>
        {MODES.map((mode) => (
          <label key={mode.id}>
            <input
              type="radio"
              name="cull-mode"
              value={mode.id}
              checked={selection.mode === mode.id}
              onChange={() => void setMode(mode.id)}
            />
            <span>{mode.label}</span>
            <small>{mode.help}</small>
          </label>
        ))}
      </fieldset>

      <SizeSlider
        selection={selection}
        eligible={status.eligible}
        onResize={resize}
        busy={busy}
      />

      <CoveragePanel status={status} coverage={selection.coverage} />

      <ol className="cull-view__gallery">
        {selection.selected.map((keep) => (
          <li key={keep.photoId}>
            <button
              type="button"
              onClick={() => void open(keep.photoId)}
              aria-pressed={focused === keep.photoId}
              data-protected={keep.protected}
            >
              {keep.photoId}
            </button>
          </li>
        ))}
      </ol>

      <RejectReasons decision={decision} onOverride={override} onCompare={open} />
    </section>
  );
}
