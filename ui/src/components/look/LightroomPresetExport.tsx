import { useEffect, useRef, useState } from 'react';
import { api, asIpcError, inTauri } from '../../ipc/client';
import { colorMatch, type LightroomPreset } from '../../ipc/colorMatch';

/** Downloads a snapshot of the applied match; never an unsaved slider preview. */
export function LightroomPresetExport({ photoId, sessionId, referenceName, eligible, disabled }: {
  photoId: string; sessionId: string; referenceName: string; eligible: boolean; disabled: boolean;
}): JSX.Element {
  const [name, setName] = useState(() => `AURA ${referenceName.replace(/\.[^.]+$/, '') || 'Matched look'}`.slice(0, 120));
  const [artifact, setArtifact] = useState<{ preset: LightroomPreset; url: string } | null>(null);
  const [busy, setBusy] = useState(false);
  const [error, setError] = useState<string | null>(null);
  const alive = useRef(true);
  const job = useRef<string | null>(null);
  const objectUrl = useRef<string | null>(null);
  useEffect(() => {
    alive.current = true;
    return () => {
      alive.current = false;
      if (job.current) void api.cancelJob(job.current).catch(() => undefined);
      if (objectUrl.current) URL.revokeObjectURL(objectUrl.current);
    };
  }, []);

  const generate = async () => {
    if (disabled || !eligible || !name.trim() || job.current || !inTauri()) return;
    const id = crypto.randomUUID(); job.current = id;
    setBusy(true); setError(null); setArtifact(null);
    if (objectUrl.current) { URL.revokeObjectURL(objectUrl.current); objectUrl.current = null; }
    try {
      const preset = await colorMatch.generatePreset(photoId, sessionId, name.trim(), id);
      if (!alive.current) return;
      const url = URL.createObjectURL(new Blob([preset.xml], { type: 'application/rdf+xml;charset=utf-8' }));
      objectUrl.current = url; setArtifact({ preset, url });
    } catch (cause) { if (alive.current) setError(asIpcError(cause).message); }
    finally { job.current = null; if (alive.current) setBusy(false); }
  };

  return <section className="lightroom-preset-export" aria-label="Lightroom preset export" aria-busy={busy}>
    <h4>Use this look in Lightroom</h4>
    <p className="lr-hint">Generate a standalone .xmp preset from the saved colour match and supported develop settings.</p>
    {!eligible && <p className="lr-hint">Match this reference and apply your strengths before generating its preset.</p>}
    <label>Lightroom preset name<input maxLength={120} value={name} disabled={disabled || busy || !eligible}
      onChange={event => {
        setName(event.target.value); setArtifact(null);
        if (objectUrl.current) { URL.revokeObjectURL(objectUrl.current); objectUrl.current = null; }
      }}/></label>
    <button type="button" disabled={disabled || busy || !eligible || !name.trim() || !inTauri()} onClick={() => void generate()}>
      {busy ? 'Generating .xmp…' : 'Generate .xmp for Lightroom'}
    </button>
    {error && <p role="alert">{error}</p>}
    {artifact && <>
      <p role="status">Your Lightroom preset is ready.</p>
      <a className="lightroom-preset-download" href={artifact.url} download={artifact.preset.fileName}>Download .xmp</a>
      <p className="lr-hint">In Lightroom, open Presets, choose Import Presets, and select the downloaded file. Find it in AURA Matched Looks.</p>
      <details><summary>Preset accuracy and compatibility</summary>
        <p className="lr-hint">The embedded colour table was checked against AURA's saved treatment on held-out colours.
          {' '}Mean OKLab difference: {artifact.preset.meanTableError.toFixed(2)}; 95th percentile: {artifact.preset.p95TableError.toFixed(2)}.
          {' '}These measure table approximation, not Lightroom's final rendering.</p>
        <ul>{artifact.preset.warnings.map(warning => <li key={warning}>{warning}</li>)}</ul>
      </details>
    </>}
  </section>;
}
