import { useEffect, useState } from 'react';
import { asIpcError, inTauri, licence, type LicenceStatus } from '../ipc/client';

type Props = {
  /** Called whenever the standing changes, so the badge in the top bar follows. */
  onChange?: (status: LicenceStatus) => void;
};

/**
 * The licence: where this installation stands, and a place to paste a key. A key is checked
 * against AURA's own signature with no network; editing never depends on it, only exporting.
 */
export function LicencePanel({ onChange }: Props): JSX.Element {
  const [status, setStatus] = useState<LicenceStatus | null>(null);
  const [key, setKey] = useState('');
  const [busy, setBusy] = useState(false);
  const [error, setError] = useState<string | null>(null);

  const show = (next: LicenceStatus): void => { setStatus(next); onChange?.(next); };

  useEffect(() => {
    if (!inTauri()) return;
    licence.status().then(show).catch(cause => setError(asIpcError(cause).message));
  // eslint-disable-next-line react-hooks/exhaustive-deps
  }, []);

  const run = async (action: () => Promise<LicenceStatus>): Promise<void> => {
    setBusy(true); setError(null);
    try { show(await action()); setKey(''); } catch (cause) { setError(asIpcError(cause).message); } finally { setBusy(false); }
  };

  const licensed = status?.state === 'licensed' || status?.state === 'expired';
  return <section className="licence-panel" aria-label="Licence">
    {status && <p className={`licence-state is-${status.state}`} role="status">{status.message}</p>}
    {licensed && status && <dl className="licence-details">
      <dt>Licensed to</dt><dd>{status.name}</dd>
      <dt>Email</dt><dd>{status.email}</dd>
      <dt>Edition</dt><dd>{status.edition}</dd>
      <dt>{status.renews ? 'Paid through' : 'Valid until'}</dt><dd>{status.expires ?? 'Perpetual'}{status.renews ? ' · renews automatically' : ''}</dd>
    </dl>}
    <label className="licence-key">Licence key
      <textarea value={key} rows={3} spellCheck={false} placeholder="AURA1.…" disabled={busy} onChange={event => setKey(event.target.value)} />
    </label>
    <div className="licence-actions">
      <button type="button" className="is-primary" disabled={busy || key.trim().length === 0} onClick={() => void run(() => licence.activate(key))}>Activate</button>
      {status?.renews && <button type="button" disabled={busy} onClick={() => void run(licence.refresh)}>Renew now</button>}
      {licensed && <button type="button" disabled={busy} onClick={() => void run(licence.deactivate)}>Remove from this computer</button>}
    </div>
    {error && <p role="alert" className="reference-error">{error}</p>}
    <p className="studio-footnote">Your key is checked on this computer. Editing always works - a licence is needed only to export finished photographs once the 14-day trial ends. A subscription renews itself: a few days before each period ends, AURA asks the shop for your new key, sending only your subscription number and email - never your photographs.</p>
  </section>;
}

/** A small reminder in the top bar while the trial runs or after it ends. */
export function licenceBadge(status: LicenceStatus | null): string | null {
  if (!status) return null;
  if (status.state === 'trial') return `Trial · ${status.daysLeft ?? 0} day${status.daysLeft === 1 ? '' : 's'} left`;
  if (status.state === 'trial_ended') return 'Trial ended · activate to export';
  if (status.state === 'expired') return 'Licence ended · renew to export';
  return null;
}
