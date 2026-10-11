import { useState } from 'react';
import { api, asIpcError } from '../ipc/client';

/** A dashboard visit is not authentication. The native command resolves a fixed provider URL. */
export function ProviderConnect({ provider, label, disabled = false, onOpened, onOpeningChange }: {
  provider: string; label: string; disabled?: boolean; onOpened?: () => void;
  onOpeningChange?: (opening: boolean) => void;
}) {
  const [opening, setOpening] = useState(false);
  const [message, setMessage] = useState('');
  const [error, setError] = useState('');
  const open = async () => {
    setOpening(true); setError(''); setMessage('');
    onOpeningChange?.(true);
    try {
      await api.openAiProviderPage(provider);
      setMessage('Key page opened. Create an API key there, return to AURA, paste it, save, then check the connection.');
      onOpened?.();
    } catch (cause) { setError(asIpcError(cause).message); }
    finally { setOpening(false); onOpeningChange?.(false); }
  };
  return <div className="provider-connect">
    <button type="button" disabled={disabled || opening} onClick={() => void open()}>
      {opening ? 'Opening browser…' : `Connect ${label} in browser`}
    </button>
    {message && <p role="status">{message}</p>}
    {error && <p role="alert">{error}</p>}
  </div>;
}
