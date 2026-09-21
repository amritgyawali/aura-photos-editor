import { useCallback, useEffect, useState } from 'react';

import { asIpcError, tone as toneApi } from '../../ipc/client';
import type { ToneDto } from '../../ipc/types';
import { ToneReviewPanel } from './ToneReviewPanel';

/**
 * PHASE-15. The container behind the per-scene white-balance review queue.
 *
 * `tone_review_queue` returns photograph ids, weakest confidence first; this fetches each one's
 * estimate so the panel can show what AURA decided and how sure it was. `ToneReviewPanel` is
 * pure and was reachable from nothing until this file (`PHASE-01-30-REVIEW.md` section 6.4).
 *
 * **A queue, not a cull.** Nothing on this surface rejects a photograph. Accepting a batch
 * records that a photographer looked and agrees, which does *not* set `userEdited` - phase 15's
 * distinction, and the reason a row can carry both AURA's numbers and a photographer's
 * agreement without the two being confused.
 *
 * **The estimates are fetched one at a time and in order.** The queue is capped, and a wedding
 * whose white balance is uncertain everywhere is a wedding where the first twenty frames are the
 * ones worth looking at rather than the whole four hundred.
 */
export type ToneReviewQueueProps = {
  /** The open wedding, or null. */
  projectId: string | null;
  /** Open one frame in the develop workspace. */
  onOpen?: (photoId: string) => void;
  /** Surface an error to the app's banner. */
  onError: (error: { code: string; message: string } | null) => void;
};

/** How many frames the queue asks for. A morning's review, not a wedding. */
const QUEUE_LIMIT = 24;

function toBanner(error: unknown): { code: string; message: string } {
  const ipc = asIpcError(error);
  return {
    code: ipc?.code ?? 'AURA-ML-5060',
    message: ipc?.message ?? 'The review queue could not be read.',
  };
}

export function ToneReviewQueue({
  projectId,
  onOpen,
  onError,
}: ToneReviewQueueProps): JSX.Element {
  const [queue, setQueue] = useState<ToneDto[]>([]);
  const [busy, setBusy] = useState(false);

  const fail = useCallback(
    (error: unknown) => {
      onError(toBanner(error));
    },
    [onError],
  );

  const reload = useCallback(async () => {
    if (!projectId) {
      setQueue([]);
      return;
    }
    try {
      const ids = await toneApi.toneReviewQueue({ projectId, limit: QUEUE_LIMIT });
      const estimates = await Promise.all(ids.map((photoId) => toneApi.imageTone(photoId)));
      setQueue(estimates.filter((estimate): estimate is ToneDto => estimate !== null));
    } catch (error) {
      fail(error);
    }
  }, [fail, projectId]);

  useEffect(() => {
    void reload();
  }, [reload]);

  const write = useCallback(
    async (action: () => Promise<unknown>) => {
      setBusy(true);
      try {
        await action();
        onError(null);
        await reload();
      } catch (error) {
        fail(error);
      } finally {
        setBusy(false);
      }
    },
    [fail, onError, reload],
  );

  if (!projectId) {
    return (
      <section className="tone-review">
        <p className="empty">Open a wedding to review its light.</p>
      </section>
    );
  }

  return (
    <section className="tone-review" aria-label="White balance review">
      {busy ? <p role="status">Saving…</p> : null}
      <ToneReviewPanel
        queue={queue}
        onAcceptAll={(photoIds) =>
          void write(async () => {
            for (const photoId of photoIds) {
              await toneApi.acceptTone({ photoId });
            }
          })
        }
        onAdjust={(photoId, values) =>
          void write(() =>
            toneApi.setToneOverride({
              projectId,
              photoId,
              temperatureK: values.temperatureK ?? null,
              tint: values.tint ?? null,
            }),
          )
        }
        onOpen={(photoId) => onOpen?.(photoId)}
      />
    </section>
  );
}
