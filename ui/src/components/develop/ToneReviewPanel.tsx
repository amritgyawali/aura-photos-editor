import { useMemo, useState } from 'react';

import type { ToneDto } from '../../ipc/types';

/**
 * PHASE-15. The low-confidence white-balance review queue.
 *
 * Section 9's MFE deliverable: "per-scene review queue for low-confidence WB, batch
 * accept/adjust". Three rules, and the first is the one that decides what this component is:
 *
 * 1. **It is a queue, not a cull.** Nothing here keeps, rejects, ranks or deletes a
 *    photograph, and no prop or handler on this component could. Phase 12 owns delivery;
 *    this owns "AURA was not sure about the colour of these, do you want to look".
 * 2. **Per scene, because the answer usually is.** A wedding's low-confidence frames are
 *    almost never scattered - they are the forty frames shot under the one weird uplighter
 *    in the corner of the reception. Grouping by scene turns forty decisions into one, and
 *    that is the whole reason the batch actions exist.
 * 3. **Accepting is not authoring.** "These look right" records that somebody looked; it
 *    does not mark the values as theirs. Phase 30's learning loop has to be able to tell
 *    an accepted suggestion from a typed correction, and a queue that conflated them would
 *    teach the model that it was right about frames nobody actually checked.
 */

/** What the panel needs. The caller fetches the estimates; nothing is derived here. */
export type ToneReviewPanelProps = {
  /** The queue, weakest colour confidence first. */
  queue: ToneDto[];
  /** Record that these frames were looked at and agreed with. */
  onAcceptAll: (photoIds: string[]) => void;
  /** Record one frame's own correction. */
  onAdjust: (photoId: string, values: { temperatureK?: number; tint?: number }) => void;
  /** Open one frame. */
  onOpen: (photoId: string) => void;
};

/** A percentage, rounded the way the sentences round it. */
function percent(value: number): string {
  return `${Math.round(value * 100)}%`;
}

/** A scene slug as words. Unknown slugs read as themselves rather than as an error. */
function sceneName(slug: string): string {
  if (slug === 'unknown') {
    return 'Scene not identified';
  }
  const words = slug.replace(/_/g, ' ');
  return words.charAt(0).toUpperCase() + words.slice(1);
}

export function ToneReviewPanel({
  queue,
  onAcceptAll,
  onAdjust,
  onOpen,
}: ToneReviewPanelProps) {
  const [shift, setShift] = useState<Record<string, number>>({});

  const groups = useMemo(() => {
    // A `Map` rather than an object, so the insertion order is the queue's own order and
    // the weakest scene stays at the top rather than wherever a key hash puts it.
    const bySemantic = new Map<string, ToneDto[]>();
    for (const frame of queue) {
      const existing = bySemantic.get(frame.scene);
      if (existing) {
        existing.push(frame);
      } else {
        bySemantic.set(frame.scene, [frame]);
      }
    }
    return [...bySemantic.entries()];
  }, [queue]);

  if (queue.length === 0) {
    return (
      <section className="tone-review" aria-label="Colour to check">
        <header>
          <h2>Colour to check</h2>
        </header>
        <p data-testid="tone-review-empty">
          AURA was confident about the colour of every photograph it has looked at.
        </p>
      </section>
    );
  }

  return (
    <section className="tone-review" aria-label="Colour to check">
      <header>
        <h2>Colour to check</h2>
        <p>
          {queue.length} photograph{queue.length === 1 ? '' : 's'} where AURA was not sure what
          colour the light was. Nothing here has been kept or rejected; this is a list to look
          at.
        </p>
      </header>

      {groups.map(([scene, frames]) => {
        const ids = frames.map((frame) => frame.photoId);
        const kelvin = shift[scene] ?? 0;
        return (
          <article className="tone-review-group" key={scene} aria-label={sceneName(scene)}>
            <h3>
              {sceneName(scene)} <span className="tone-review-count">{frames.length}</span>
            </h3>

            <div className="tone-review-batch">
              <button
                type="button"
                onClick={() => onAcceptAll(ids)}
                data-testid={`tone-review-accept-${scene}`}
              >
                These look right
              </button>
              <label htmlFor={`tone-review-shift-${scene}`}>Warm or cool them all</label>
              <input
                id={`tone-review-shift-${scene}`}
                type="number"
                step="50"
                value={String(kelvin)}
                onChange={(event) =>
                  setShift((current) => ({ ...current, [scene]: Number(event.target.value) }))
                }
              />
              <button
                type="button"
                disabled={kelvin === 0}
                onClick={() => {
                  for (const frame of frames) {
                    onAdjust(frame.photoId, { temperatureK: frame.temperatureK + kelvin });
                  }
                }}
                data-testid={`tone-review-adjust-${scene}`}
              >
                Apply to these {frames.length}
              </button>
            </div>

            <ol className="tone-review-frames">
              {frames.map((frame) => (
                <li key={frame.photoId}>
                  <button type="button" onClick={() => onOpen(frame.photoId)}>
                    {Math.round(frame.temperatureK)} K, tint {Math.round(frame.tint)}
                  </button>
                  <span className="tone-review-confidence">
                    {percent(frame.wbConf)} confident
                  </span>
                  {frame.mixedLight ? (
                    <span className="tone-review-mixed">more than one light</span>
                  ) : null}
                  {frame.constrainedIdentities === 0 ? (
                    <span className="tone-review-unconstrained">no skin reference</span>
                  ) : null}
                </li>
              ))}
            </ol>
          </article>
        );
      })}
    </section>
  );
}
