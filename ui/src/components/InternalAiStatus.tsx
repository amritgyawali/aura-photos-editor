import { useEffect, useRef, useState } from 'react';
import { api, asIpcError, inTauri } from '../ipc/client';
import type { InternalAiStatus as Status } from '../ipc/internalAi';

/** What a route's check found, in a photographer's words. The code is shown when it is not one of these. */
export function routeStatus(status: string): string {
  const http = /^(check_)?http_(\d+)$/.exec(status);
  if (http) {
    const code = http[2];
    if (code === '401' || code === '403') return 'the provider did not accept this key';
    if (code === '402') return 'the account has no balance';
    if (code === '429') return 'over its quota or rate limit today';
    if (code === '404') return 'the model is not offered to this account';
    return `the provider answered with an error (${code})`;
  }
  switch (status) {
    case 'catalogue_ready': return 'ready';
    case 'check_no_answer_in_time': return 'did not answer within 15 seconds';
    case 'check_image_not_read': return 'answered, but did not read the test picture';
    case 'check_unreadable_answer': return 'answered in a form AURA could not read';
    case 'check_not_sent': return 'could not be checked';
    case 'model_not_listed': return 'the provider does not list this model';
    case 'unreadable_catalogue': return 'the provider\'s model list could not be read';
    case 'unreachable': return 'could not be reached';
    case 'not_checked': return 'not checked yet';
    default: return status.replaceAll('_', ' ');
  }
}

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
      <p>{status.profiles.filter(p => p.status === 'catalogue_ready' && p.vision && p.inputUsdPerMtok !== null && p.outputUsdPerMtok !== null).length} photo routes available. Each one read a generated test picture when it was last checked; no photograph is sent to check a provider.</p>
      <ul>{status.profiles.map(profile => <li key={profile.id}>
        {profile.id}: {profile.model} · {profile.vision ? 'image and text' : 'text / image support unconfirmed'} · {routeStatus(profile.status)}
        {(profile.inputUsdPerMtok === null || profile.outputUsdPerMtok === null) && ' · pricing needed before use'}
      </li>)}</ul>
      <p>Routing reviews saved reliability outcomes and your quality ratings daily while AURA is open. It does not retrain models or rewrite application code.</p>
    </>}
    {!busy && status && !status.sourceAvailable && <p>Internal configuration is not loaded. Check offline mode and your local configuration file. Advanced manual setup remains available below.</p>}
    <button type="button" disabled={busy || !inTauri()} onClick={() => { void load(true); }}>Refresh internal providers</button>
  </section>;
}
