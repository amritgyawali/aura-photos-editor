import { useEffect, useRef } from 'react';
import { asIpcError, oneClickCancel, oneClickStatus } from '../../ipc/client';
import { automaticBusy, useAutomatic } from '../../state/automaticStore';

/** Mounted in the app shell: changing tools cannot orphan a running job. */
export function AutomaticProgress({ onFinished }: { onFinished: (projectId: string) => void }): JSX.Element | null {
  const { jobId, projectId, status, error } = useAutomatic();
  const busy = useAutomatic(automaticBusy);
  const finished = useRef(onFinished);
  finished.current = onFinished;
  useEffect(() => {
    if (!jobId || !projectId || !busy) return;
    let disposed = false;
    let timer: number | undefined;
    const poll = async (): Promise<void> => {
      try {
        const row = await oneClickStatus(jobId);
        if (disposed) return;
        useAutomatic.setState({ status: row, error: null });
        if (row.status === 'running' || row.status === 'cancelling') {
          timer = window.setTimeout(() => void poll(), 800);
        } else {
          finished.current(projectId);
        }
      } catch (err) {
        if (!disposed) useAutomatic.setState({ jobId: null, error: asIpcError(err).message });
      }
    };
    void poll();
    return () => { disposed = true; window.clearTimeout(timer); };
  }, [jobId, projectId, busy]);
  if (!jobId && !error) return null;
  return <section className="panel automatic-progress" aria-label="Automatic processing" aria-live="polite">
    <strong>{status?.phaseLabel ?? 'Starting automatic processing…'}</strong>
    {status && <>
      <p>{status.frames} photos · {status.analyzed ?? 0} analyzed · {status.aiEdited} AI edits · {status.localEdited} local edits · {status.verified} verified exports · {status.failedEdits ?? 0} failed edits</p>
      {busy && status.itemsTotal > 0 && <progress value={status.itemsDone} max={status.itemsTotal} aria-label={status.phase} />}
      <p>Output: <code>{status.destination}</code></p>
      {status.notes.length > 0 && <details open={status.status === 'failed' || status.status === 'completed_with_issues'}><summary>Run notes ({status.notes.length})</summary><ul>{status.notes.map((note, i) => <li key={i}>{note}</li>)}</ul></details>}
    </>}
    {busy && <><p>Leave AURA open while it works. Originals are preserved.</p><button type="button" onClick={() => { if (jobId) void oneClickCancel(jobId).catch(err => useAutomatic.setState({ error: asIpcError(err).message })); }}>Stop automatic processing</button></>}
    {error && <p role="alert">{error}</p>}
  </section>;
}
