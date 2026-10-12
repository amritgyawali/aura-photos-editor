import { useEffect, useRef, useState } from 'react';
import { api, asIpcError, inTauri, pairedReference, pickPairedReference, type PairedReferenceReport } from '../../ipc/client';

export function PairedReference({ photoId, disabled, onApply }: {
  photoId: string; disabled: boolean; onApply: (action: () => Promise<unknown>) => Promise<void>;
}): JSX.Element {
  const [busy, setBusy] = useState(false);
  const [report, setReport] = useState<PairedReferenceReport | null>(null);
  const [error, setError] = useState<string | null>(null);
  const running = useRef(false);
  const job = useRef<string | null>(null);
  const mounted = useRef(true);
  useEffect(() => { mounted.current = true; return () => {
    mounted.current = false;
    if (job.current) void api.cancelJob(job.current).catch(() => { /* Worker checks cancellation. */ });
  }; }, []);

  const match = async () => {
    if (running.current || disabled) return;
    running.current = true; setBusy(true); setError(null); setReport(null);
    try {
      const reference = await pickPairedReference();
      if (!reference || !mounted.current) return;
      await onApply(async () => {
        job.current = crypto.randomUUID();
        const result = await pairedReference.match(photoId, reference, job.current);
        if (mounted.current) setReport(result);
      });
    } catch (cause) { if (mounted.current) setError(asIpcError(cause).message); }
    finally { job.current = null; running.current = false; if (mounted.current) setBusy(false); }
  };

  return <section aria-label="Match finished reference" aria-busy={busy} className="lr-panel">
    <h3>Match finished reference</h3>
    <p className="lr-hint">Choose a finished JPEG or PNG of this same photo, with the same dimensions and crop. AURA fits tone and colour while keeping your saved healing and manual settings. Up to 16 megapixels.</p>
    <button type="button" disabled={disabled || busy || !inTauri()} onClick={() => void match()}>{busy ? 'Matching reference…' : 'Choose finished reference'}</button>
    {busy && <><p role="status">Comparing rendered edits, then checking the result at full resolution…</p>
      <button type="button" onClick={() => { if (job.current) void api.cancelJob(job.current).catch(cause => setError(asIpcError(cause).message)); }}>Stop reference matching</button></>}
    {error && <p role="alert">{error}</p>}
    {report && <p role="status" data-testid="paired-reference-result" data-reference-report={JSON.stringify(report)}>{report.changed ? `Saved ${report.changed} tone and colour changes. Use Undo to compare. ` : 'Your edit was kept. '}
      RGB error: {report.beforeError.toFixed(3)} → {report.afterError.toFixed(3)} / 255. {report.exactMatch ? 'Pixels match exactly.' : 'Pixels still differ from the reference.'}</p>}
  </section>;
}
