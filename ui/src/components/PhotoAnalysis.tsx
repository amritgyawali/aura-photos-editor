import { useEffect, useState } from 'react';
import { asIpcError, photoAnalysis } from '../ipc/client';
import type { PhotoAnalysisDto } from '../ipc/types';

export function PhotoAnalysis({ projectId, photoId }: { projectId: string; photoId: string | null }): JSX.Element {
  const [result, setResult] = useState<PhotoAnalysisDto | null>(null);
  const [error, setError] = useState<string | null>(null);
  useEffect(() => {
    let disposed = false;
    setResult(null); setError(null);
    if (photoId) void photoAnalysis({ projectId, photoId, jobId: `analysis-${photoId}` }).then(row => {
      if (!disposed) setResult(row);
    }).catch(err => { if (!disposed) setError(asIpcError(err).message); });
    return () => { disposed = true; };
  }, [projectId, photoId]);
  return <section className="panel" aria-label="Photo analysis"><h2>Photo analysis</h2>
    {!photoId ? <p>Import photographs to begin.</p> : !result && !error ? <p>Measuring this photograph…</p> : null}
    {error && <p role="alert">{error}</p>}
    {result && <>
      <p>Local measurements from this photo. A configured vision model can refine the edit during automatic processing.</p>
      <table><tbody>
        <tr><th>Suggested treatment</th><td>{result.recommendation.preset.replaceAll('_', ' ')}</td></tr>
        <tr><th>Exposure correction</th><td>{result.recommendation.exposure.toFixed(2)} EV</td></tr>
        <tr><th>White balance correction</th><td>{result.recommendation.temperature} K / tint {result.recommendation.tint} (5500 / 0 preserves the decoded balance)</td></tr>
        <tr><th>Highlight clipping</th><td>{(result.readings.clipped_bp / 100).toFixed(1)}%</td></tr>
        <tr><th>Deep black pixels</th><td>{(result.readings.black_bp / 100).toFixed(1)}%</td></tr>
        <tr><th>Candidate neutral pixels</th><td>{result.readings.neutral_pixels} / {result.readings.pixels}</td></tr>
      </tbody></table>
      <ul>{result.recommendation.reasons.map((reason, i) => <li key={i}>{reason}</li>)}</ul>
    </>}
  </section>;
}
