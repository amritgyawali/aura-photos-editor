import { useRef, useState } from 'react';
import { api, asIpcError, inTauri, pickPhotoFolder } from '../../ipc/client';
import { referenceStyle, type FetchReport, type ReferenceSelection } from './referenceStyle';

type Props = {
  selection: ReferenceSelection | null;
  disabled: boolean;
  onChange: (selection: ReferenceSelection | null) => void;
  onBusyChange: (busy: boolean) => void;
  onAddPhotos: () => void;
  onApply?: () => void;
};

export function InstagramStyle({ selection, disabled, onChange, onBusyChange, onAddPhotos, onApply }: Props): JSX.Element {
  const [address, setAddress] = useState('');
  const [limit, setLimit] = useState(240);
  const [busy, setBusy] = useState(false);
  const [status, setStatus] = useState('');
  const [error, setError] = useState<string | null>(null);
  const [coverage, setCoverage] = useState<FetchReport | null>(null);
  const stopped = useRef(false);
  const running = useRef(false);
  const cancelId = useRef('');

  const analyse = async (fromInstagram: boolean) => {
    if (running.current || disabled || !inTauri()) return;
    running.current = true; stopped.current = false;
    cancelId.current = `reference-${crypto.randomUUID()}`;
    setBusy(true); onBusyChange(true); setError(null); setCoverage(null);
    setStatus(fromInstagram ? 'Reading public Instagram posts… This may take a few minutes.' : 'Choose your saved reference photographs.');
    try {
      let folder: string | null;
      if (fromInstagram) {
        const fetched = await referenceStyle.fetch(address.trim(), limit, cancelId.current);
        setCoverage(fetched);
        folder = fetched.folder;
        if (fetched.fetched < 8) throw new Error(`${fetched.message} Retrieved ${fetched.fetched} photos; at least 8 distinct readable references are needed.`);
      } else folder = await pickPhotoFolder('Choose at least 8 saved reference photos');
      if (!folder || stopped.current) { setStatus('Stopped. Your previous style is unchanged.'); return; }
      setStatus('Measuring white balance, tonal range, contrast and color in the reference photos…');
      const analysis = await referenceStyle.analyse(address.trim(), folder, cancelId.current);
      if (stopped.current) { setStatus('Stopped. Your previous style is unchanged.'); return; }
      onChange({ analysis, strength: 0.8 });
      setStatus(`Style ready from ${analysis.measured} photos. New imports will use this reference automatically.`);
    } catch (cause) {
      setError(asIpcError(cause).message); setStatus('');
    } finally { running.current = false; setBusy(false); onBusyChange(false); }
  };

  const analysis = selection?.analysis;
  return <section className="instagram-style" aria-label="Instagram style matching" aria-busy={busy}>
    <div className="reference-heading"><div><span className="eyebrow">START WITH YOUR INSPIRATION</span>
      <h1>Love their look?<br /><em>Make it part of yours.</em></h1>
      <p>Paste a photographer’s Instagram profile. Learn from the available photos, then automatically adapt the look to your own.</p>
    </div><div className="reference-process" aria-label="Style matching steps"><span>1 · Add a reference</span><span>2 · Analyze the photos</span><span>3 · Edit your collection</span></div></div>
    <fieldset className="reference-form" disabled={disabled || busy}>
      <label htmlFor="instagram-profile">Photographer’s Instagram profile</label>
      <div className="reference-url-row"><input id="instagram-profile" type="text" inputMode="url" value={address} onChange={event => setAddress(event.target.value)} placeholder="https://www.instagram.com/chrisburkard/" />
        <button type="button" className="is-primary" disabled={!address.trim() || !inTauri()} onClick={() => void analyse(true)}>Analyze Instagram style</button></div>
      <div className="reference-options"><label>Photo limit <select value={limit} onChange={event => setLimit(Number(event.target.value))}><option value={60}>60 photos · quick study</option><option value={240}>240 photos · broader study</option><option value={2000}>All accessible · up to 2,000</option></select></label>
        <button type="button" disabled={!inTauri()} onClick={() => void analyse(false)}>Use saved reference photos</button></div>
    </fieldset>
    {error && <p role="alert" className="reference-error">{error}</p>}
    {status && <p role="status">{status}</p>}
    {busy && <button type="button" onClick={() => { stopped.current = true; setStatus('Stopping…'); void api.cancelJob(cancelId.current).catch(cause => setError(asIpcError(cause).message)); }}>Stop analysis</button>}
    {coverage && <p className="reference-note">Retrieved {coverage.fetched} photos · {coverage.skipped} videos or unsupported items skipped · {coverage.complete ? 'Reached the end of accessible posts' : 'Partial profile coverage'}. {coverage.message}</p>}
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
    <p className="reference-note">Public photos only. Instagram may limit access; saved JPEG or PNG references work offline. Matching estimates the visible style, including white balance and grading—it cannot recover exact editing settings, recreate lighting, or guarantee an identical result.</p>
  </section>;
}
