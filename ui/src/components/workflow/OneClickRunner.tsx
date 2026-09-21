import { useCallback, useEffect, useRef, useState } from 'react';

import {
  oneClickCancel,
  oneClickFinish,
  oneClickStatus,
  pickImportPaths,
  inTauri,
  asIpcError,
} from '../../ipc/client';
import type { OneClickStatusDto } from '../../ipc/types';
import { useStore } from '../../state/store';
import { useAutomatic } from '../../state/automaticStore';

/**
 * One button that finishes a wedding. ADR-0065's surface.
 *
 * The promise in the product's own words: analysis, framing, the cull, the AI
 * edit of every delivered frame, and the export - run in that order, through the
 * same commands every panel uses, to a destination this photographer chose. The
 * destination field is deliberately not remembered and not defaulted: the one
 * thing automation may not do is invent where somebody's files go.
 *
 * Progress is polled, like every pass in the product. The row carries the notes
 * the pipeline made its way through - refusals, degradations, the cap - and they
 * are rendered, because a button that hides what it could not do is the failure
 * mode the review named twice.
 */

export type OneClickRunnerProps = {
  /** Bumped after a run so the step bar re-reads its evidence. */
  onFinished: () => void;
};

const PHASE_LABEL: Record<string, string> = {
  queue: 'Queued',
  ingest: 'Import',
  cloud: 'Provider',
  analyze: 'Analyze',
  geometry: 'Framing',
  cull: 'Cull',
  edit: 'AI edit',
  export: 'Export',
  done: 'Delivered',
};

export function OneClickRunner({ onFinished }: OneClickRunnerProps): JSX.Element {
  const activeProjectId = useStore((state) => state.activeProjectId);
  const progress = useStore((state) => state.progress);
  const [destination, setDestination] = useState('');
  const [status, setStatus] = useState<OneClickStatusDto | null>(null);
  const [error, setError] = useState<string | null>(null);
  const [busy, setBusy] = useState(false);
  const jobRef = useRef<string | null>(null);
  const timerRef = useRef<number | null>(null);

  const stop = useCallback(() => {
    if (timerRef.current !== null) {
      window.clearInterval(timerRef.current);
      timerRef.current = null;
    }
  }, []);

  useEffect(() => stop, [stop]);

  const poll = useCallback(
    (jobId: string) => {
      void oneClickStatus(jobId)
        .then((row) => {
          setStatus(row);
          if (row.status !== 'running' && row.status !== 'cancelling') {
            stop();
            setBusy(false);
            jobRef.current = null;
            onFinished();
          }
        })
        .catch((err: unknown) => {
          setError(asIpcError(err).message);
          stop();
          setBusy(false);
        });
    },
    [onFinished, stop],
  );

  const start = useCallback(async () => {
    if (!activeProjectId || !inTauri() || !destination) {
      return;
    }
    setBusy(true);
    useAutomatic.setState({ starting: true });
    setError(null);
    setStatus(null);
    try {
      const handle = await oneClickFinish({
        projectId: activeProjectId,
        destination,
        // The wizard's still-running import joins this run rather than racing it.
        ingestJobId: progress.running ? progress.jobId : null,
      });
      jobRef.current = handle.jobId;
      useAutomatic.getState().adopt(handle.jobId, activeProjectId);
      timerRef.current = window.setInterval(() => {
        void poll(handle.jobId);
      }, 1000);
      poll(handle.jobId);
    } catch (err) {
      useAutomatic.setState({ starting: false });
      setBusy(false);
      const message =
        typeof err === 'object' && err !== null && 'message' in err
          ? String((err as { message: unknown }).message)
          : String(err);
      setError(message);
    }
  }, [activeProjectId, destination, poll, progress.jobId, progress.running]);

  const cancel = useCallback(() => {
    if (jobRef.current) {
      void oneClickCancel(jobRef.current).catch((err: unknown) => setError(asIpcError(err).message));
    }
  }, []);

  const percent =
    status !== null && status.itemsTotal > 0
      ? Math.round((status.itemsDone / status.itemsTotal) * 100)
      : null;

  return (
    <section className="panel one-click" aria-labelledby="one-click-title">
      <h2 id="one-click-title">Finish everything</h2>
      <p className="one-click-lead">
        Analyze, frame, cull, AI-edit and deliver this wedding in one press. You choose
        where the files go - nothing else clicks anything.
      </p>
      <div className="row">
        <input
          type="text"
          placeholder="Destination folder - required"
          aria-label="Destination folder"
          value={destination}
          disabled={busy}
          onChange={(event) => {
            setDestination(event.target.value);
          }}
        />
        <button
          type="button"
          disabled={busy || !inTauri()}
          onClick={() => {
            void pickImportPaths(true).then((paths) => {
              if (paths[0]) {
                setDestination(paths[0]);
              }
            }).catch((err: unknown) => setError(asIpcError(err).message));
          }}
        >
          Browse…
        </button>
      </div>
      <div className="row">
        <button
          type="button"
          className="btn btn-primary one-click-run"
          disabled={busy || !activeProjectId || !destination}
          onClick={() => void start()}
        >
          {busy ? 'Working…' : 'Finish everything'}
        </button>
        {busy ? (
          <button type="button" className="btn-danger" onClick={cancel}>
            Stop
          </button>
        ) : null}
      </div>

      {status ? (
        <div className="one-click-progress">
          <p className="one-click-phase">
            <strong>{PHASE_LABEL[status.phase] ?? status.phase}</strong> — {status.phaseLabel}
            {percent !== null ? ` (${percent}%)` : ''}
          </p>
          <div className="progress">
            <div
              className="progress-bar"
              style={{ width: `${percent ?? (status.status === 'running' ? 100 : 0)}%` }}
            />
          </div>
          <p className="one-click-counts">
            {status.frames.toLocaleString()} frames
            {' · '}
            {status.aiEdited.toLocaleString()} AI-edited
            {' · '}
            {status.localEdited.toLocaleString()} local
            {' · '}
            {status.written.toLocaleString()} written
            {' · '}
            {status.verified.toLocaleString()} verified
          </p>
          {status.notes.length > 0 ? (
            <details className="one-click-notes" open={status.status === 'failed'}>
              <summary>What the pipeline said ({status.notes.length})</summary>
              <ul>
                {status.notes.map((line, index) => (
                  <li key={index}>{line}</li>
                ))}
              </ul>
            </details>
          ) : null}
          {status.status === 'completed' ? (
            <p className="one-click-done">
              Delivered to {status.destination}. Every file was read back and hashed, and the
              manifest is sealed.
            </p>
          ) : null}
        </div>
      ) : null}
      {error ? <p className="one-click-error" role="alert">{error}</p> : null}
    </section>
  );
}
