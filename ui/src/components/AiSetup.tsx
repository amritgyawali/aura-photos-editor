import { useCallback, useEffect, useMemo, useState } from 'react';

import { api, asIpcError, inTauri } from '../ipc/client';
import type { AiModelDto, AiProviderDto, AiSetupStatusDto } from '../ipc/types';

export type AiSetupProps = {
  /** Called once the question has been answered, either way. */
  onDone: (status: AiSetupStatusDto) => void;
  /** Called when the photographer closes the screen without answering. */
  onDismiss?: () => void;
  onError?: (error: { code: string; message: string }) => void;
};

/** Which half of the screen is showing. */
export type SetupStep = 'choose' | 'connect';

/**
 * Providers whose label, blurb or identifier match what was typed.
 *
 * Matching the blurb as well as the name is deliberate: somebody who knows they
 * want "the fast one" or "a local model" should not have to already know that
 * Groq and Ollama are the answers.
 */
export function filterProviders(
  providers: AiProviderDto[],
  query: string,
): AiProviderDto[] {
  const wanted = query.trim().toLowerCase();
  if (wanted.length === 0) {
    return providers;
  }
  return providers.filter((provider) =>
    `${provider.id} ${provider.label} ${provider.blurb}`.toLowerCase().includes(wanted),
  );
}

/** Money, per million tokens, the way every vendor publishes it. */
export function priceLine(model: AiModelDto): string {
  if (model.inputPerMtokUsd === 0 && model.outputPerMtokUsd === 0) {
    return 'no charge';
  }
  return `$${model.inputPerMtokUsd.toFixed(2)} in / $${model.outputPerMtokUsd.toFixed(
    2,
  )} out per million tokens`;
}

/**
 * What is true about this provider that a photographer should read before they
 * paste a key, rather than find out on their first run.
 *
 * Every one of these is a fact about the choice rather than an error, so they are
 * shown as notes beside the provider and none of them stops anybody continuing.
 */
export function providerWarnings(
  provider: AiProviderDto | null,
  schemes: string[],
): string[] {
  if (!provider) {
    return [];
  }
  const notes: string[] = [];
  const scheme = provider.endpoint.startsWith('https') ? 'https' : 'http';
  if (schemes.length > 0 && !schemes.includes(scheme)) {
    notes.push(
      `This build cannot reach ${scheme} addresses, so a key saved here would not be used.`,
    );
  }
  if (!provider.images) {
    notes.push(
      'These models do not look at photographs, so anything that needs to see a frame will use ' +
        "AURA's own models instead.",
    );
  }
  if (!provider.requiresKey) {
    notes.push('No key is needed. Make sure the server is running before you continue.');
  }
  return notes;
}

/**
 * Whether the Save button should do anything yet.
 *
 * A provider that needs no key is ready as soon as it is chosen. One that does is
 * ready when a key has been typed, or when this machine already has one stored -
 * which is what makes "switch back to the provider I set up last month" a click
 * rather than a trip to a vendor's dashboard.
 */
export function canContinue(
  provider: AiProviderDto | null,
  keyText: string,
  keyedProviders: string[],
): boolean {
  if (!provider) {
    return false;
  }
  if (!provider.requiresKey) {
    return true;
  }
  return keyText.trim().length > 0 || keyedProviders.includes(provider.id);
}

/**
 * Whether the Check button can ask a question that means anything yet.
 *
 * It probes whichever provider the gateway is currently pointed at, so it is only
 * honest once this provider is the saved one. A provider that needs a key also
 * needs one stored; a local server needs nothing, and Check is the most useful
 * button on the screen for one - the thing most likely to be wrong about Ollama
 * is that it is not running.
 */
export function canCheck(
  provider: AiProviderDto | null,
  status: AiSetupStatusDto | null,
  keyedProviders: string[],
): boolean {
  if (!provider || status?.provider !== provider.id) {
    return false;
  }
  return !provider.requiresKey || keyedProviders.includes(provider.id);
}

/** The sentence at the top, which is different on a second visit. */
export function setupHeadline(status: AiSetupStatusDto | null): string {
  if (status?.skipped) {
    return 'You skipped this earlier. Connect a provider whenever you like.';
  }
  if (status?.completed) {
    return 'Change which AI provider AURA asks, or switch to one you have already set up.';
  }
  return 'AURA edits a whole wedding without any of this. A key adds a reasoning layer on top, and you pay your provider directly for what it uses.';
}

/**
 * The first thing a photographer sees, and the one screen in the product whose
 * job is to be skippable.
 *
 * Two halves. The first is the catalogue: nineteen providers, what each is good
 * at, what it costs and whether it can see a photograph. The second is the one
 * provider they picked: the key, the address when it is theirs to set, the three
 * models when they want to name them, and a Check button that spends one round
 * trip proving the key works before anybody starts a four-thousand-frame run.
 *
 * Three things this screen will not do. It does not require an answer - "not now"
 * is a first-class button and the product is complete without a key. It does not
 * read a key back, ever; the field is cleared the moment it is saved and there is
 * no command that could return one. And it does not open a browser: a vendor's
 * key page is printed as text to be copied, because a desktop application that
 * launches a browser from a settings screen is a desktop application that can be
 * talked into launching one somewhere else.
 */
export function AiSetup({ onDone, onDismiss, onError }: AiSetupProps): JSX.Element {
  const [providers, setProviders] = useState<AiProviderDto[]>([]);
  const [status, setStatus] = useState<AiSetupStatusDto | null>(null);
  const [step, setStep] = useState<SetupStep>('choose');
  const [selectedId, setSelectedId] = useState<string | null>(null);
  const [query, setQuery] = useState('');
  const [keyText, setKeyText] = useState('');
  const [endpoint, setEndpoint] = useState('');
  const [models, setModels] = useState<string[]>(['', '', '']);
  const [advanced, setAdvanced] = useState(false);
  const [busy, setBusy] = useState(false);
  const [checkResult, setCheckResult] = useState<string | null>(null);

  const report = useCallback(
    (error: unknown) => {
      const ipc = asIpcError(error);
      onError?.({ code: ipc.code, message: ipc.message });
    },
    [onError],
  );

  useEffect(() => {
    if (!inTauri()) {
      return;
    }
    void (async () => {
      try {
        setProviders(await api.listAiProviders());
        setStatus(await api.aiSetupStatus());
      } catch (error) {
        report(error);
      }
    })();
  }, [report]);

  const selected = useMemo(
    () => providers.find((provider) => provider.id === selectedId) ?? null,
    [providers, selectedId],
  );

  const visible = useMemo(() => filterProviders(providers, query), [providers, query]);
  const keyed = status?.keyedProviders ?? [];

  const choose = useCallback(
    (provider: AiProviderDto) => {
      setSelectedId(provider.id);
      setKeyText('');
      setCheckResult(null);
      setEndpoint(status?.provider === provider.id ? (status.endpoint ?? '') : '');
      setModels(
        status?.provider === provider.id && status.models.length === 3
          ? status.models
          : ['', '', ''],
      );
      setStep('connect');
    },
    [status],
  );

  const save = useCallback(async () => {
    if (!inTauri() || !selected) {
      return;
    }
    setBusy(true);
    try {
      // The key first and on its own. It is the only value on this screen that
      // is a secret, it travels through the one command that carries one, and the
      // field is cleared the moment it is gone.
      if (keyText.trim().length > 0) {
        await api.setAiKey({
          provider: selected.id,
          key: keyText,
          endpoint: endpoint.trim().length > 0 ? endpoint.trim() : null,
        });
        setKeyText('');
      }
      const next = await api.saveAiSetup({
        provider: selected.id,
        endpoint: endpoint.trim().length > 0 ? endpoint.trim() : null,
        cheapModel: models[0]?.trim() || null,
        balancedModel: models[1]?.trim() || null,
        reasoningModel: models[2]?.trim() || null,
        completed: true,
        skipped: false,
      });
      setStatus(next);
      onDone(next);
    } catch (error) {
      report(error);
    } finally {
      setBusy(false);
    }
  }, [endpoint, keyText, models, onDone, report, selected]);

  const check = useCallback(async () => {
    if (!inTauri()) {
      return;
    }
    setBusy(true);
    setCheckResult('Checking...');
    try {
      const result = await api.checkAiKey();
      setCheckResult(result.message);
    } catch (error) {
      setCheckResult(null);
      report(error);
    } finally {
      setBusy(false);
    }
  }, [report]);

  const skip = useCallback(async () => {
    if (!inTauri()) {
      onDismiss?.();
      return;
    }
    setBusy(true);
    try {
      onDone(await api.skipAiSetup());
    } catch (error) {
      report(error);
    } finally {
      setBusy(false);
    }
  }, [onDismiss, onDone, report]);

  const notes = providerWarnings(selected, status?.schemes ?? []);

  return (
    <div className="ai-setup" role="dialog" aria-modal="true" aria-label="Connect an AI provider">
      <div className="ai-setup-sheet">
        <header className="ai-setup-header">
          <h1>Connect an AI provider</h1>
          <p className="ai-setup-headline">{setupHeadline(status)}</p>
        </header>

        {status?.offlineStudioMode ? (
          <p className="ai-setup-offline">
            Offline studio mode is on, so nothing leaves this computer whatever is saved here.
          </p>
        ) : null}

        {step === 'choose' ? (
          <>
            <label className="ai-setup-search">
              Search
              <input
                type="search"
                placeholder="claude, fast, local, cheap..."
                value={query}
                onChange={(event) => setQuery(event.currentTarget.value)}
              />
            </label>

            <ul className="ai-setup-grid">
              {visible.map((provider) => (
                <li key={provider.id}>
                  <button
                    type="button"
                    className="ai-setup-card"
                    onClick={() => choose(provider)}
                    aria-label={`Choose ${provider.label}`}
                  >
                    <span className="ai-setup-card-label">{provider.label}</span>
                    {keyed.includes(provider.id) ? (
                      <span className="ai-setup-card-keyed">key saved</span>
                    ) : null}
                    <span className="ai-setup-card-blurb">{provider.blurb}</span>
                    <span className="ai-setup-card-price">
                      {provider.models.map((model) => model.model).join(' · ')}
                    </span>
                  </button>
                </li>
              ))}
            </ul>
            {visible.length === 0 ? (
              <p className="ai-setup-empty">
                No provider matches that. Anything that speaks OpenAI&apos;s chat format works
                through &quot;My own server&quot;.
              </p>
            ) : null}
          </>
        ) : null}

        {step === 'connect' && selected ? (
          <div className="ai-setup-connect">
            <h2>{selected.label}</h2>
            <p className="ai-setup-blurb">{selected.blurb}</p>

            <ul className="ai-setup-tiers">
              {selected.models.map((model) => (
                <li key={model.tier}>
                  <strong>{model.tier}</strong> {model.model} - {priceLine(model)}
                </li>
              ))}
            </ul>

            {notes.length > 0 ? (
              <ul className="ai-setup-notes">
                {notes.map((note) => (
                  <li key={note}>{note}</li>
                ))}
              </ul>
            ) : null}

            {selected.requiresKey ? (
              <label className="ai-setup-key">
                Key
                <input
                  type="password"
                  autoComplete="off"
                  spellCheck={false}
                  placeholder={
                    keyed.includes(selected.id) ? 'a key is already saved' : selected.keyHint
                  }
                  value={keyText}
                  disabled={busy}
                  onChange={(event) => setKeyText(event.currentTarget.value)}
                />
              </label>
            ) : null}

            {selected.requiresKey && selected.keysUrl.length > 0 ? (
              <p className="ai-setup-keys-url">
                Keys come from <code>{selected.keysUrl}</code>
              </p>
            ) : null}

            {selected.endpointEditable ? (
              <label className="ai-setup-endpoint">
                Address
                <input
                  type="text"
                  placeholder={selected.endpoint}
                  value={endpoint}
                  disabled={busy}
                  onChange={(event) => setEndpoint(event.currentTarget.value)}
                />
              </label>
            ) : (
              <p className="ai-setup-endpoint-fixed">
                AURA talks to <code>{selected.endpoint}</code> and nowhere else.
              </p>
            )}

            <button
              type="button"
              className="ai-setup-advanced-toggle"
              aria-expanded={advanced}
              onClick={() => setAdvanced((open) => !open)}
            >
              {advanced ? 'Hide model names' : 'Choose model names'}
            </button>

            {advanced ? (
              <div className="ai-setup-advanced">
                <p>
                  Leave a box empty to use the model above. A name AURA has never heard of is fine;
                  what your provider accepts is what matters.
                </p>
                {selected.models.map((model, index) => (
                  <label key={model.tier}>
                    {model.tier}
                    <input
                      type="text"
                      placeholder={model.model}
                      value={models[index] ?? ''}
                      disabled={busy}
                      onChange={(event) => {
                        const typed = event.currentTarget.value;
                        setModels((current) =>
                          current.map((existing, slot) => (slot === index ? typed : existing)),
                        );
                      }}
                    />
                  </label>
                ))}
              </div>
            ) : null}

            {selected.requiresKey ? (
              <p className="ai-setup-storage">
                Your key goes into this computer&apos;s own secure key store. It is never written
                to your catalog, never written to a log, and there is no way to read it back out
                of AURA.
              </p>
            ) : (
              <p className="ai-setup-storage">
                Nothing is stored anywhere for this one. AURA remembers the address and the model
                names, and there is no key to keep.
              </p>
            )}

            {checkResult ? <p className="ai-setup-check">{checkResult}</p> : null}

            <div className="ai-setup-actions">
              <button type="button" disabled={busy} onClick={() => setStep('choose')}>
                Back
              </button>
              <button
                type="button"
                disabled={busy || !canContinue(selected, keyText, keyed)}
                onClick={() => void save()}
              >
                Save and use this
              </button>
              {/* Check asks the *saved* provider, because that is the one the gateway is
                  pointed at. Enabling it before Save would spend a round trip proving
                  that last month's provider still works, and report the answer under
                  this month's name. */}
              <button
                type="button"
                disabled={busy || !canCheck(selected, status, keyed)}
                onClick={() => void check()}
              >
                Check
              </button>
            </div>
          </div>
        ) : null}

        <footer className="ai-setup-footer">
          <button type="button" className="ai-setup-skip" disabled={busy} onClick={() => void skip()}>
            Not now - AURA works without this
          </button>
        </footer>
      </div>
    </div>
  );
}
