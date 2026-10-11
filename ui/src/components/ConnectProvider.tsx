import { useState } from 'react';

import { api, asIpcError, inTauri, type ProviderPageDto } from '../ipc/client';

/** The two providers most photographers already pay for, by catalogue id. */
export const QUICK_CONNECT: ReadonlyArray<readonly [string, string, string]> = [
  ['anthropic', 'Connect Claude', 'Opens your Anthropic console to create a Claude API key'],
  ['openai', 'Connect ChatGPT', 'Opens your OpenAI dashboard to create an API key for GPT models'],
] as const;

/**
 * Opens a provider's own key page in the default browser, then says what to do next.
 *
 * Neither Anthropic nor OpenAI lets a desktop application sign somebody in and receive an API
 * key, so "connect" is: the browser opens on the key page, the photographer creates a key there,
 * and pastes it into the key field beside this button. The address comes from the backend's
 * catalogue, never from this component. ADR-0108 section 5.
 */
export function OpenKeyPageButton({ provider, label, title, onOpened, onError }: {
  provider: string;
  label: string;
  title?: string;
  onOpened?: (provider: string, result: ProviderPageDto) => void;
  onError?: (error: { code: string; message: string }) => void;
}): JSX.Element {
  const [busy, setBusy] = useState(false);
  const open = async () => {
    if (!inTauri()) {
      onOpened?.(provider, { url: null, opened: false, message: 'Open the AURA desktop app to connect a provider.' });
      return;
    }
    setBusy(true);
    try {
      onOpened?.(provider, await api.openProviderPage(provider));
    } catch (error) {
      const ipc = asIpcError(error);
      onError?.({ code: ipc.code, message: ipc.message });
    } finally {
      setBusy(false);
    }
  };
  return (
    <button type="button" className={`connect-provider connect-provider-${provider}`} title={title} disabled={busy} onClick={() => void open()}>
      {label}
    </button>
  );
}

/** "Connect Claude" and "Connect ChatGPT", with the instruction that follows a click. */
export function ConnectProviders({ onConnect, onError }: {
  /** Called with the provider id once its page was asked for, so the panel can select it. */
  onConnect?: (provider: string) => void;
  onError?: (error: { code: string; message: string }) => void;
}): JSX.Element {
  const [result, setResult] = useState<ProviderPageDto | null>(null);
  return (
    <div className="connect-providers" aria-label="Connect an AI account">
      <div className="connect-providers-buttons">
        {QUICK_CONNECT.map(([provider, label, title]) => (
          <OpenKeyPageButton key={provider} provider={provider} label={label} title={title} onError={onError}
            onOpened={(id, next) => { setResult(next); onConnect?.(id); }} />
        ))}
      </div>
      {result ? (
        <p className="connect-providers-next" role="status">
          {result.message}
          {result.url ? <> <code>{result.url}</code></> : null}
        </p>
      ) : (
        <p className="connect-providers-hint">
          Your browser opens on the provider&apos;s key page. Sign in, create a key, and paste it below.
        </p>
      )}
    </div>
  );
}
