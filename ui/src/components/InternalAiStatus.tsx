import { useEffect, useRef, useState } from 'react';
import { api, asIpcError, inTauri } from '../ipc/client';
import type { InternalAiStatus as Status } from '../ipc/internalAi';

/** Connection metadata only. Keys are never returned to this component. */
export function InternalAiStatus() {
  const [status, setStatus] = useState<Status | null>(null);
  const [busy, setBusy] = useState(false);
  const [error, setError] = useState('');
  const mounted = useRef(true);
  const pending = useRef(false);
  async function load(refresh: boolean) {
    if (!inTauri() || pending.current) return;
    pending.current = true; setBusy(true); setError('');
    try { const result = await api.internalAiStatus(refresh); if (mounted.current) setStatus(result); }
    catch (e) { if (mounted.current) setError(asIpcError(e).message); }
    finally { pending.current = false; if (mounted.current) setBusy(false); }
  }
  useEffect(() => { mounted.current = true; void load(false); return () => { mounted.current = false; }; }, []);
  return <section aria-label="Internal AI routing">
    <h3>Internal AI</h3>
    <p>Providers load from your local AI configuration. Photo requests use image-capable models with up to three routes, then local editing. Privacy and spending limits apply.</p>
    {busy && <p role="status">Checking configured providers…</p>}
    {error && <p role="alert">{error}</p>}
    {status?.sourceAvailable && <>
      <p>{status.profiles.filter(p => p.status === 'catalogue_ready' && p.vision && p.inputUsdPerMtok !== null && p.outputUsdPerMtok !== null).length} photo routes available. Catalogue checks do not prove a completion will succeed.</p>
      <ul>{status.profiles.map(profile => <li key={profile.id}>
        {profile.id}: {profile.model} · {profile.vision ? 'image and text' : 'text / image support unconfirmed'} · {profile.status.replaceAll('_', ' ')}
        {(profile.inputUsdPerMtok === null || profile.outputUsdPerMtok === null) && ' · pricing needed before use'}
      </li>)}</ul>
      <p>Routing reviews saved reliability outcomes and your quality ratings daily while AURA is open. It does not retrain models or rewrite application code.</p>
    </>}
    {!busy && status && !status.sourceAvailable && <p>Internal configuration is not loaded. Check offline mode and your local configuration file. Advanced manual setup remains available below.</p>}
    <button type="button" disabled={busy || !inTauri()} onClick={() => { void load(true); }}>Refresh internal providers</button>
  </section>;
}
