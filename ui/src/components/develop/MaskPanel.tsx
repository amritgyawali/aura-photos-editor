/**
 * PHASE-18. The mask panel: what regions AURA found, how good they are, and what a photographer
 * may do about it.
 *
 * # Two bars, never one
 *
 * `confidence` and `edgeQuality` are shown separately because they fail independently and are
 * fixed by different things: a photographer can re-brush a boundary and cannot re-brush a class.
 * Collapsing them into one "quality" number loses which of the two they are looking at, and the
 * panel names the limiting one in a sentence. ADR-0038 decision 2.
 *
 * # The allowance is not computed here
 *
 * `allowance` and `allowsAggressive` arrive on the wire. Two implementations of a gating rule is
 * two answers to "may this mask carry skin smoothing", and the one written in TypeScript is the
 * one nobody tests against a fixture. ADR-0038 decision 3.
 *
 * # Nothing in this panel edits a photograph
 *
 * Section 2.2 puts every *use* of a mask in phases 19 to 24. What is here is inspection, a
 * brush, a feather and a refine - the four things section 2.1 asks for - and the overlay, which
 * is a plane of alpha drawn over the preview rather than a rendered pixel.
 */

import { useCallback, useEffect, useMemo, useState } from 'react';

import { maskApi } from '../../ipc/client';
import type { MaskDto, MaskOverlayDto, MaskStatusDto } from '../../ipc/types';

/**
 * The five operations a later phase can ask about, in the order a photographer meets them.
 *
 * Mirrors `aura_vision::mask::quality::Operation`. It is a constant here rather than fetched,
 * because it is a closed set the panel renders labels for and a fetch would mean a round trip to
 * draw a list of five words.
 */
const OPERATIONS: ReadonlyArray<{ id: string; label: string }> = [
  { id: 'local_tone', label: 'Local light and colour' },
  { id: 'skin_smooth', label: 'Skin smoothing' },
  { id: 'micro_retouch', label: 'Blemish removal' },
  { id: 'restoration', label: 'Detail recovery' },
  { id: 'generative_cleanup', label: 'Generative cleanup' },
];

/** Human labels for the twenty class slugs. */
const KIND_LABELS: Record<string, string> = {
  skin: 'Skin',
  face: 'Face',
  eyes: 'Eyes',
  sclera: 'Eye whites',
  iris: 'Irises',
  teeth: 'Teeth',
  lips: 'Lips',
  eyebrows: 'Eyebrows',
  hair: 'Hair',
  facial_hair: 'Facial hair',
  clothing: 'Clothing',
  dress: 'Dress',
  background: 'Background',
  sky: 'Sky',
  subject: 'Subject',
  greenery: 'Greenery',
  water: 'Water',
  floor: 'Floor',
  window: 'Window or light',
  skin_safe: 'Skin-safe zone',
};

function kindLabel(kind: string): string {
  return KIND_LABELS[kind] ?? kind;
}

/** The word for a boundary, as a photographer reads it. */
function edgeLabel(edge: string): string {
  switch (edge) {
    case 'matted':
      return 'Refined';
    case 'soft':
      return 'Soft';
    case 'binary':
      return 'Hard';
    default:
      return 'Not determined';
  }
}

/**
 * Which of the two numbers is holding this mask back.
 *
 * A sentence rather than a colour, because "amber" does not tell anybody what to do and
 * "the edge of this region is not well determined" does.
 */
export function limitingFactor(mask: MaskDto): string | null {
  if (mask.userEdited) {
    return null;
  }
  if (mask.allowance >= 0.999) {
    return null;
  }
  if (mask.edgeQuality < mask.confidence) {
    return 'The edge of this region is not well determined, so changes here are made gently.';
  }
  return 'AURA is not certain this region is what it says it is, so changes here are made gently.';
}

type Props = {
  projectId: string;
  imageId: string;
};

export function MaskPanel({ projectId, imageId }: Props): JSX.Element {
  const [status, setStatus] = useState<MaskStatusDto | null>(null);
  const [masks, setMasks] = useState<MaskDto[] | null>(null);
  const [selected, setSelected] = useState<string | null>(null);
  const [overlay, setOverlay] = useState<MaskOverlayDto | null>(null);
  const [busy, setBusy] = useState(false);
  const [error, setError] = useState<string | null>(null);

  const load = useCallback(async () => {
    setError(null);
    try {
      const [next, rows] = await Promise.all([
        maskApi.maskStatus(projectId),
        maskApi.imageMasks(imageId),
      ]);
      setStatus(next);
      setMasks(rows);
    } catch (caught) {
      setError(String(caught));
    }
  }, [projectId, imageId]);

  useEffect(() => {
    void load();
  }, [load]);

  const current = useMemo(
    () => masks?.find((mask) => mask.id === selected) ?? null,
    [masks, selected],
  );

  useEffect(() => {
    if (!current) {
      setOverlay(null);
      return;
    }
    let cancelled = false;
    void maskApi
      .maskOverlay(current.id)
      .then((plane) => {
        if (!cancelled) {
          setOverlay(plane);
        }
      })
      .catch((caught) => setError(String(caught)));
    return () => {
      cancelled = true;
    };
  }, [current]);

  const findRegions = useCallback(async () => {
    setBusy(true);
    setError(null);
    try {
      const rows = await maskApi.ensureMasks({ projectId, imageId, kinds: [] });
      setMasks(rows);
      setStatus(await maskApi.maskStatus(projectId));
    } catch (caught) {
      setError(String(caught));
    } finally {
      setBusy(false);
    }
  }, [projectId, imageId]);

  const feather = useCallback(
    async (mask: MaskDto, amount: number) => {
      setBusy(true);
      try {
        const updated = await maskApi.editMask({
          maskId: mask.id,
          ops: [{ op: 'feather', amount }],
          feather: amount,
        });
        setMasks((rows) =>
          (rows ?? []).map((row) => (row.id === updated.id ? updated : row)),
        );
      } catch (caught) {
        setError(String(caught));
      } finally {
        setBusy(false);
      }
    },
    [],
  );

  const refineEdge = useCallback(async (mask: MaskDto) => {
    setBusy(true);
    try {
      // Refine Edge is a grow followed by a shrink: it closes the pinholes a colour-grown
      // region picks up along a busy boundary without moving the boundary itself. The pair is
      // sent as one command, so it is one undo step.
      const updated = await maskApi.editMask({
        maskId: mask.id,
        ops: [
          { op: 'grow', radius: 2 },
          { op: 'shrink', radius: 2 },
        ],
      });
      setMasks((rows) => (rows ?? []).map((row) => (row.id === updated.id ? updated : row)));
    } catch (caught) {
      setError(String(caught));
    } finally {
      setBusy(false);
    }
  }, []);

  const regenerate = useCallback(
    async (mask: MaskDto) => {
      setBusy(true);
      try {
        await maskApi.regenerateMask(mask.id);
        await load();
        setSelected(null);
      } catch (caught) {
        setError(String(caught));
      } finally {
        setBusy(false);
      }
    },
    [load],
  );

  return (
    <section className="mask-panel" aria-label="Masks">
      <header className="mask-panel__header">
        <h2>Regions</h2>
        {status ? (
          <p className="mask-panel__coverage">
            {/* Two numbers, never a ratio. A project where the cull has not run says so. */}
            {status.selected === 0
              ? 'Nothing is selected yet. Cull the wedding and AURA will find regions in what you keep.'
              : `${status.masked} of ${status.selected} selected photographs have regions.`}
          </p>
        ) : null}
        {status && !status.headTrained ? (
          <p className="mask-panel__caveat" role="note">
            AURA&rsquo;s learned segmentation is not trained in this build. Regions here are
            measured from the photograph rather than predicted, and the model card says what
            that means.
          </p>
        ) : null}
      </header>

      {error ? (
        <p className="mask-panel__error" role="alert">
          {error}
        </p>
      ) : null}

      {masks && masks.length === 0 ? (
        <div className="mask-panel__empty">
          <p>Nobody has looked for regions in this photograph yet.</p>
          <button type="button" onClick={() => void findRegions()} disabled={busy}>
            Find regions
          </button>
        </div>
      ) : null}

      <ul className="mask-panel__list">
        {(masks ?? []).map((mask) => (
          <li key={mask.id}>
            <button
              type="button"
              className={mask.id === selected ? 'is-selected' : undefined}
              aria-pressed={mask.id === selected}
              onClick={() => setSelected(mask.id === selected ? null : mask.id)}
            >
              <span className="mask-panel__kind">
                {mask.identityName
                  ? `${mask.identityName}: ${kindLabel(mask.kind).toLowerCase()}`
                  : kindLabel(mask.kind)}
              </span>
              {mask.userEdited ? <span className="mask-panel__edited">Yours</span> : null}
              {!mask.allowsAggressive ? (
                <span className="mask-panel__gated">Limited</span>
              ) : null}
            </button>
          </li>
        ))}
      </ul>

      {current ? (
        <div className="mask-panel__detail">
          <h3>{kindLabel(current.kind)}</h3>

          {/* Two bars. See the module note. */}
          <dl className="mask-panel__quality">
            <dt>Certainty</dt>
            <dd>
              <meter min={0} max={1} value={current.confidence} />
              <span>{Math.round(current.confidence * 100)}%</span>
            </dd>
            <dt>Edge</dt>
            <dd>
              <meter min={0} max={1} value={current.edgeQuality} />
              <span>{edgeLabel(current.edge)}</span>
            </dd>
          </dl>

          {limitingFactor(current) ? (
            <p className="mask-panel__limit" role="note">
              {limitingFactor(current)}
            </p>
          ) : null}

          <ul className="mask-panel__reasons">
            {current.reasons.map((reason) => (
              <li key={reason.code}>{reason.text}</li>
            ))}
          </ul>

          <ul className="mask-panel__operations">
            {OPERATIONS.map((operation) => {
              const permitted =
                operation.id === 'skin_smooth' || operation.id === 'generative_cleanup'
                  ? current.allowsAggressive
                  : true;
              return (
                <li key={operation.id} data-permitted={permitted}>
                  {operation.label}
                  <span>
                    {permitted
                      ? `up to ${Math.round(current.allowance * 100)}%`
                      : 'not through this region'}
                  </span>
                </li>
              );
            })}
          </ul>

          <label className="mask-panel__feather">
            Feather
            <input
              type="range"
              min={0}
              max={1}
              step={0.01}
              value={current.feather}
              disabled={busy}
              onChange={(event) => void feather(current, Number(event.target.value))}
            />
          </label>

          <div className="mask-panel__actions">
            <button type="button" onClick={() => void refineEdge(current)} disabled={busy}>
              Refine edge
            </button>
            {current.userEdited ? (
              <button
                type="button"
                className="mask-panel__regenerate"
                onClick={() => void regenerate(current)}
                disabled={busy}
              >
                Reset to AURA&rsquo;s version
              </button>
            ) : null}
          </div>

          {overlay ? (
            <p className="mask-panel__overlay-note">
              Overlay shown at {overlay.width}&times;{overlay.height}.
            </p>
          ) : null}

          <p className="mask-panel__storage">
            Stored as {current.form === 'alpha8' ? 'a soft plane' : 'an outline'},{' '}
            {Math.round(current.bytes / 1024)} KB.
          </p>
        </div>
      ) : null}
    </section>
  );
}
