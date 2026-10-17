import { useEffect, useRef, useState } from 'react';
import { asIpcError, oneClickCancel, oneClickStatus } from '../../ipc/client';
import { automaticBusy, useAutomatic } from '../../state/automaticStore';
import type { OneClickStatusDto } from '../../ipc/types';

const POLL_MS = 800;
const MAX_STATUS_FAILURES = 3;

/**
 * The run in one line. A photo that could not be opened is neither left out by the cull nor a
 * failed edit, and saying either would send somebody looking in the wrong place.
 */
export function summary(status: OneClickStatusDto): string {
  const analyzed = status.analyzed ?? 0;
  const unreadable = status.phase === 'ingest' || status.phase === 'analyze' ? 0 : Math.max(0, status.frames - analyzed);
  const culled = status.selected > 0 ? Math.max(0, analyzed - status.selected) : 0;
  const failed = Math.max(0, (status.failedEdits ?? 0) - unreadable);
  return [
    `${status.frames} photos`,
    `${analyzed} analyzed`,
    unreadable > 0 ? `${unreadable} could not be opened` : '',
    culled > 0 ? `${culled} left out by the cull` : '',
    `${status.aiEdited + status.localEdited} of ${status.selected || analyzed || status.frames} edited and retouched`,
    `${status.written} written`,
    `${status.verified} verified exports`,
    `${failed} failed edits`,
  ].filter(Boolean).join(' · ');
}

/** Mounted in the app shell: changing tools cannot orphan a running job. */
export function AutomaticProgress({ onFinished }: { onFinished: (projectId: string) => void }): JSX.Element | null {
  const { jobId, projectId, status, error, starting } = useAutomatic();
  const busy = useAutomatic(automaticBusy);
  const [retry, setRetry] = useState(0);
  const [paused, setPaused] = useState(false);
  const finished = useRef(onFinished);
  finished.current = onFinished;
  useEffect(() => {
    // Starting the next run makes the UI busy before its new handle arrives. The
    // previous terminal run must not start polling again in that interval.
    if (!jobId || !projectId || !busy || starting) return;
    let disposed = false;
    let timer: number | undefined;
    let failures = 0;
    setPaused(false);
    const poll = async (): Promise<void> => {
      try {
        const row = await oneClickStatus(jobId);
        if (disposed) return;
        useAutomatic.setState({ status: row, error: null });
        if (row.status === 'running' || row.status === 'cancelling') {
          failures = 0;
          timer = window.setTimeout(() => void poll(), POLL_MS);
        } else {
          finished.current(projectId);
        }
      } catch (err) {
        if (disposed) return;
        // A failed status read does not stop the native worker. Keep its handle, its last
        // progress and the busy lock, so the user can reconnect or cancel the same run.
        useAutomatic.setState({ error: asIpcError(err).message });
        failures++;
        if (failures < MAX_STATUS_FAILURES) {
          timer = window.setTimeout(() => void poll(), POLL_MS * 2 ** failures);
        } else {
          setPaused(true);
        }
      }
    };
    void poll();
    return () => { disposed = true; window.clearTimeout(timer); };
  }, [jobId, projectId, busy, starting, retry]);
  if (!jobId && !error) return null;
  return <section className="panel automatic-progress" aria-label="Automatic processing" aria-live="polite">
    <strong>{status?.phaseLabel ?? 'Starting automatic processing…'}</strong>
    {status && <>
      <p>{summary(status)}</p>
      {busy && status.itemsTotal > 0 && <progress value={status.itemsDone} max={status.itemsTotal} aria-label={status.phase} />}
      <p>Output: <code>{status.destination}</code></p>
      {status.notes.length > 0 && <details open={status.status === 'failed' || status.status === 'completed_with_issues'}><summary>Run notes ({status.notes.length})</summary><ul>{status.notes.map((note, i) => <li key={i}>{note}</li>)}</ul></details>}
    </>}
    {busy && <><p>Leave AURA open while it works. Originals are preserved.</p><button type="button" onClick={() => { if (jobId) void oneClickCancel(jobId).catch(err => useAutomatic.setState({ error: asIpcError(err).message })); }}>Stop automatic processing</button></>}
    {error && <p role="alert">{error}</p>}
    {busy && error && <p>The last known progress is shown. {paused ? 'Status checks are paused.' : 'Reconnecting to the running job…'} <button type="button" onClick={() => setRetry(value => value + 1)}>Retry status</button></p>}
  </section>;
}
