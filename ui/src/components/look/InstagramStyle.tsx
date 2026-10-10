import { useRef, useState, type ReactNode } from 'react';
import { api, asIpcError, inTauri, pickPhotoFolder } from '../../ipc/client';
import { referenceStyle, type ReferenceSelection } from './referenceStyle';

type Props = {
  selection: ReferenceSelection | null;
  disabled: boolean;
  onChange: (selection: ReferenceSelection | null) => void;
  onBusyChange: (busy: boolean) => void;
  onAddPhotos: () => void;
  onApply?: () => void;
  /** Inside the start screen's second step, which supplies its own heading. */
  compact?: boolean;
  /** Shown instead of the full heading in compact mode. */
  heading?: ReactNode;
};

export function InstagramStyle({ selection, disabled, onChange, onBusyChange, onAddPhotos, onApply, compact = false, heading }: Props): JSX.Element {
  const [address, setAddress] = useState('');
  const [busy, setBusy] = useState(false);
  const [status, setStatus] = useState('');
  const [error, setError] = useState<string | null>(null);
  const stopped = useRef(false);
  const running = useRef(false);
  const cancelId = useRef('');

  /** Measure reference photos the photographer already has: a folder, or their Instagram data export. */
  const analyse = async (fromExport: boolean) => {
    if (running.current || disabled || !inTauri()) return;
    running.current = true; stopped.current = false;
    cancelId.current = `reference-${crypto.randomUUID()}`;
    setBusy(true); onBusyChange(true); setError(null);
    setStatus(fromExport ? 'Choose the unzipped folder Instagram sent you.' : 'Choose a folder of reference photos.');
    try {
      const folder = await pickPhotoFolder(fromExport ? 'Choose your unzipped Instagram data export' : 'Choose a folder of at least 8 reference photos');
      if (!folder || stopped.current) { setStatus('Stopped. Your previous style is unchanged.'); return; }
      setStatus('Measuring white balance, tonal range, contrast and color in the reference photos…');
      const analysis = await referenceStyle.analyse(address.trim(), folder, cancelId.current, fromExport);
      if (stopped.current) { setStatus('Stopped. Your previous style is unchanged.'); return; }
      onChange({ analysis, strength: 0.8 });
      setStatus(`Style ready from ${analysis.measured} photos. New imports will use this reference automatically.`);
    } catch (cause) {
      setError(asIpcError(cause).message); setStatus('');
    } finally { running.current = false; setBusy(false); onBusyChange(false); }
  };

  const analysis = selection?.analysis;
  return <section className={compact ? 'instagram-style is-compact' : 'instagram-style'} aria-label="Reference style matching" aria-busy={busy}>
    {compact && heading}
    {!compact && <div className="reference-heading"><div><span className="eyebrow">START WITH YOUR INSPIRATION</span>
      <h1>Love a look?<br /><em>Make it part of yours.</em></h1>
      <p>Show AURA photos with the look you want. It measures their tone and colour and adapts that look to your own photographs.</p>
    </div><div className="reference-process" aria-label="Style matching steps"><span>1 · Add reference photos</span><span>2 · Analyze them</span><span>3 · Edit your collection</span></div></div>}
    <fieldset className="reference-form" disabled={disabled || busy}>
      <label htmlFor="reference-name">Whose look is this?{compact ? ' (optional)' : ' (optional, for your own reference)'}</label>
      <input id="reference-name" type="text" value={address} onChange={event => setAddress(event.target.value)} placeholder="A photographer, a magazine, your own best work…" />
      <div className="reference-options">
        <button type="button" className="is-primary" disabled={!inTauri()} onClick={() => void analyse(false)}>Choose reference photos</button>
        <button type="button" disabled={!inTauri()} onClick={() => void analyse(true)}>Use my Instagram data export</button>
      </div>
    </fieldset>
    {error && <p role="alert" className="reference-error">{error}</p>}
    {status && <p role="status">{status}</p>}
    {busy && <button type="button" onClick={() => { stopped.current = true; setStatus('Stopping…'); void api.cancelJob(cancelId.current).catch(cause => setError(asIpcError(cause).message)); }}>Stop analysis</button>}
    {analysis && selection && <div className="reference-ready">
      <div className="reference-palette" aria-label="Measured reference palette">{analysis.colors.map(color => <span key={color} style={{ backgroundColor: color }} title={color} />)}</div>
      <div><span className="eyebrow">REFERENCE READY</span><h2>{analysis.origin || 'Your reference style'}</h2><p>{analysis.measured} photos analyzed · {analysis.skipped} unreadable</p></div>
      <div className="reference-traits"><span>{analysis.brightness < 0.4 ? 'Deep tones' : analysis.brightness > 0.65 ? 'Light tones' : 'Balanced tones'}</span><span>{analysis.contrast > 0.7 ? 'Strong contrast' : 'Soft contrast'}</span><span>{analysis.warmth > 3 ? 'Warm color lean' : analysis.warmth < -3 ? 'Cool color lean' : 'Neutral color lean'}</span></div>
      <label className="reference-strength">Look strength <output>{Math.round(selection.strength * 100)}%</output><input type="range" min={0} max={100} value={Math.round(selection.strength * 100)} disabled={disabled || busy} onChange={event => onChange({ ...selection, strength: Number(event.target.value) / 100 })} /></label>
      <div className="import-actions"><button type="button" className="is-primary" disabled={disabled || busy} onClick={onAddPhotos}>Choose your photos</button>
        {onApply && <button type="button" disabled={disabled || busy} onClick={onApply}>Apply to this collection</button>}
        <button type="button" disabled={disabled || busy} onClick={() => onChange(null)}>Clear reference</button></div>
      <p className="reference-note">Applies automatically to new imports. Strength changes affect existing photos when you press Apply. Your manual settings stay protected.</p>
    </div>}
    <p className="reference-note">At least 8 JPEG or PNG photos. Use photos you have the right to use - your own work, or images you have saved for personal reference. For your own Instagram, request your data from Instagram (Settings → Your activity → Download your information), unzip it and choose the folder. AURA reads only what you choose and downloads nothing. Matching estimates the visible style - it cannot recover exact editing settings or recreate lighting.</p>
  </section>;
}
