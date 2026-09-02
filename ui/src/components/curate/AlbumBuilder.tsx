import { useState } from 'react';

import type { CurateAlbumDto } from '../../ipc/types';

/**
 * PHASE-29. The album: chapters, spreads, coverage, and drag-to-reorder.
 *
 * Pure - rows and callbacks in, nothing fetched.
 *
 * ## `rhythmMeasurable` decides how the rhythm score is drawn
 *
 * A rhythm of 1.000 measured over eight per cent of an album is not a claim about the album, and on
 * this build eight per cent is the realistic figure because phase 06's detector finds no faces. So
 * below a third the score is rendered in grey with the share beside it rather than as a result.
 *
 * ## A drag never crosses a chapter
 *
 * The drop target refuses a move into another chapter before the command is sent, and the command
 * refuses it again. Three enforcers for one rule, because a wedding album whose ceremony follows its
 * reception is not an album with an unusual sequence; it is an album that is wrong.
 *
 * ## Coverage is over the album, not the gallery
 *
 * Phase 12 already reported that the gallery covers the ring exchange. The question here is whether
 * the *album* does, and a rule the gallery itself misses is reported as missing rather than blamed
 * on the album.
 */
export type AlbumBuilderProps = {
  /** The album draft, or null when this wedding has not been curated. */
  album: CurateAlbumDto | null;
  /** Open one spread. */
  onSelectSpread: (spreadId: string) => void;
  /** Record a new order for the whole album. */
  onReorder: (order: string[]) => void;
};

export function AlbumBuilder({ album, onSelectSpread, onReorder }: AlbumBuilderProps) {
  const [dragging, setDragging] = useState<{ image: string; chapter: string } | null>(null);

  if (!album) {
    return (
      <section className="curate-album" aria-label="Album">
        <p className="empty">
          No album yet. Curate this wedding to see the sequence AURA would print.
        </p>
      </section>
    );
  }

  const rhythmIsMeaningful = album.rhythmMeasurable >= 0.33;

  /** Move `image` so that it sits where `target` is, inside the same chapter. */
  const reorder = (image: string, target: string) => {
    const order = album.spreads.flatMap((spread) =>
      [spread.left, spread.right].filter((id): id is string => id !== null),
    );
    const from = order.indexOf(image);
    const to = order.indexOf(target);
    if (from < 0 || to < 0 || from === to) {
      return;
    }
    const next = [...order];
    next.splice(from, 1);
    next.splice(to, 0, image);
    onReorder(next);
  };

  return (
    <section className="curate-album" aria-label="Album">
      <header className="album-header">
        <p className="album-summary">{album.summary}</p>
        <dl className="album-scores">
          <div>
            <dt>rhythm</dt>
            <dd data-meaningful={rhythmIsMeaningful}>
              {rhythmIsMeaningful
                ? `${album.rhythmScore.toFixed(2)} over ${(album.rhythmMeasurable * 100).toFixed(0)}% of the album`
                : `measured on only ${(album.rhythmMeasurable * 100).toFixed(0)}% of the album`}
            </dd>
          </div>
          <div>
            <dt>pairing</dt>
            <dd>{album.pairingScore.toFixed(2)}</dd>
          </div>
          <div>
            <dt>size</dt>
            <dd>
              {album.size} of {album.targetSize}
            </dd>
          </div>
        </dl>
        {album.userOrdered ? (
          <p className="album-user-ordered">
            You set this order. AURA has left it alone and will keep leaving it alone.
          </p>
        ) : null}
      </header>

      <ul className="album-coverage" aria-label="Album coverage">
        {album.coverage.map(([rule, state]) => (
          <li key={rule} data-state={state}>
            {rule.replace(/_/g, ' ')}: {state.replace(/_/g, ' ')}
          </li>
        ))}
      </ul>

      {album.warnings.length > 0 ? (
        <ul className="album-warnings">
          {album.warnings.map((warning) => (
            <li key={warning}>{warning}</li>
          ))}
        </ul>
      ) : null}

      <ol className="album-spreads">
        {album.spreads.map((spread) => (
          <li key={spread.spreadId} className="album-spread" data-chapter={spread.chapter}>
            <button
              type="button"
              className="album-spread-open"
              onClick={() => onSelectSpread(spread.spreadId)}
            >
              {spread.index + 1}
            </button>
            {[spread.left, spread.right].map((image, side) =>
              image ? (
                <span
                  key={image}
                  className="album-page"
                  draggable
                  onDragStart={() => setDragging({ image, chapter: spread.chapter })}
                  onDragOver={(event) => {
                    // Refused here as well as in the command: a drop target that accepted a
                    // cross-chapter move and then showed an error would be teaching a photographer
                    // that AURA is arbitrary.
                    if (dragging && dragging.chapter === spread.chapter) {
                      event.preventDefault();
                    }
                  }}
                  onDrop={() => {
                    if (dragging && dragging.chapter === spread.chapter) {
                      reorder(dragging.image, image);
                    }
                    setDragging(null);
                  }}
                >
                  {image}
                </span>
              ) : (
                <span key={`blank-${side}`} className="album-page blank">
                  blank
                </span>
              ),
            )}
          </li>
        ))}
      </ol>

      <ul className="album-reasons">
        {album.reasons.map((reason, index) => (
          <li key={`${reason.code}-${index}`} className={reason.caveat ? 'caveat' : 'argument'}>
            {reason.text}
          </li>
        ))}
      </ul>
    </section>
  );
}
