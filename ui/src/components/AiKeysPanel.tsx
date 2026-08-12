import { useCallback, useEffect, useState } from 'react';

import { api, asIpcError, inTauri } from '../ipc/client';
import type {
  CloudCallDto,
  CloudSpendDto,
  CloudStatusDto,
} from '../ipc/types';

export type AiKeysPanelProps = {
  projectId: string | null;
  onError?: (error: { code: string; message: string }) => void;
};

/** The providers a user may choose. */
export const PROVIDER_CHOICES = ['anthropic', 'openai', 'google', 'compat'] as const;

/** The label a photographer reads, rather than the identifier we store. */
export function providerLabel(provider: string): string {
  switch (provider) {
    case 'anthropic':
      return 'Anthropic (Claude)';
    case 'openai':
      return 'OpenAI (GPT)';
    case 'google':
      return 'Google (Gemini)';
    case 'compat':
      return 'My own server (OpenAI-compatible)';
    default:
      return provider;
  }
}

/** Money, the way a spend meter should show it. */
export function money(usd: number): string {
  return `$${usd.toFixed(2)}`;
}

/**
 * The sentence under the spend meter.
 *
 * Section 6.3 asks for "this wedding has used $0.42 of your $5 cap" in those
 * words, because a bar with no numbers on it is not something anyone can plan
 * against.
 */
export function spendSentence(spend: CloudSpendDto | null): string {
  if (!spend) {
    return 'No AI spending recorded for this job yet.';
  }
  const base = `This wedding has used ${money(spend.spentUsd)} of your ${money(spend.capUsd)} cap.`;
  if (spend.stopped) {
    return `${base} The limit has been reached, so the rest of the run uses AURA's own models.`;
  }
  return base;
}

/**
 * What is switched off, said plainly.
 *
 * A panel that shows a working system which happens to make no calls is a panel
 * that generates support tickets. Every reason a call would not be made is
 * listed, in the order they are checked.
 */
export function disabledReasons(status: CloudStatusDto | null): string[] {
  if (!status) {
    return [];
  }
  const reasons: string[] = [];
  if (status.offlineStudioMode) {
    reasons.push('Offline studio mode is on, so no photograph or detail leaves this computer.');
  }
  if (!status.projectEnabled) {
    reasons.push('Cloud AI is off for this job. New jobs start with it off.');
  }
  if (!status.keyPresent) {
    reasons.push('No key is saved for this provider yet.');
  }
  if (status.breakerReason) {
    reasons.push(`Calls stopped after a problem: ${status.breakerReason}`);
  }
  if (status.transport === 'offline') {
    reasons.push('This build has no way to reach the internet, so every decision is local.');
  }
  return reasons;
}

/** One audit row, as a sentence rather than a table cell. */
export function callSummary(call: CloudCallDto): string {
  if (call.source === 'local_fallback') {
    return `${call.task} - AURA's own models (${call.fallbackReason ?? 'no reason recorded'})`;
  }
  if (call.source === 'cache') {
    return `${call.task} - reused an earlier answer, no charge`;
  }
  return `${call.task} - ${call.model}, ${call.tokensIn}+${call.tokensOut} tokens, ${money(
    call.costUsd,
  )}, ${call.latencyMs} ms${call.retryCount > 0 ? ', repaired once' : ''}`;
}

/**
 * Settings > AI Keys.
 *
 * The key is typed here, sent once, and never read back. The field is cleared
 * the moment it is saved, the panel shows a fingerprint rather than the secret,
 * and there is no command that could return it.
 */
export function AiKeysPanel({ projectId, onError }: AiKeysPanelProps): JSX.Element {
  const [status, setStatus] = useState<CloudStatusDto | null>(null);
  const [spend, setSpend] = useState<CloudSpendDto | null>(null);
  const [calls, setCalls] = useState<CloudCallDto[]>([]);
  const [provider, setProvider] = useState<string>('anthropic');
  const [keyText, setKeyText] = useState('');
  const [endpoint, setEndpoint] = useState('');
  const [checking, setChecking] = useState(false);
  const [checkResult, setCheckResult] = useState<string | null>(null);
  const [busy, setBusy] = useState(false);

  const report = useCallback(
    (error: unknown) => {
      const ipc = asIpcError(error);
      onError?.({ code: ipc.code, message: ipc.message });
    },
    [onError],
  );

  const refresh = useCallback(async () => {
    if (!inTauri()) {
      return;
    }
    try {
      const next = await api.cloudStatus();
      setStatus(next);
      setProvider(next.provider);
    } catch (error) {
      report(error);
    }
    if (!projectId) {
      return;
    }
    // Spend and the audit rows are separate calls on purpose: a spending query
    // that fails must not stop the panel showing whether a key is stored.
    try {
      setSpend(await api.cloudSpend(projectId));
      setCalls(await api.cloudCalls(projectId, 25));
    } catch (error) {
      report(error);
    }
  }, [projectId, report]);

  useEffect(() => {
    void refresh();
  }, [refresh]);

  const save = useCallback(async () => {
    if (!inTauri() || keyText.trim().length === 0) {
      return;
    }
    setBusy(true);
    try {
      const next = await api.setAiKey({
        provider,
        key: keyText,
        endpoint: endpoint.trim().length > 0 ? endpoint.trim() : null,
      });
      setStatus(next);
      // Cleared immediately: the field is the only place the secret exists in
      // the renderer, and a panel left holding it ends up in a screenshot.
      setKeyText('');
      setCheckResult(null);
    } catch (error) {
      report(error);
    } finally {
      setBusy(false);
    }
  }, [endpoint, keyText, provider, report]);

  const forget = useCallback(async () => {
    if (!inTauri()) {
      return;
    }
    setBusy(true);
    try {
      setStatus(await api.clearAiKey(provider));
      setCheckResult(null);
    } catch (error) {
      report(error);
    } finally {
      setBusy(false);
    }
  }, [provider, report]);

  const check = useCallback(async () => {
    if (!inTauri()) {
      return;
    }
    setChecking(true);
    setCheckResult('Checking...');
    try {
      const result = await api.checkAiKey();
      setCheckResult(result.message);
      setStatus(await api.cloudStatus());
    } catch (error) {
      setCheckResult(null);
      report(error);
    } finally {
      setChecking(false);
    }
  }, [report]);

  const setPrivacy = useCallback(
    async (change: Partial<{ enabled: boolean; offline: boolean; blur: boolean }>) => {
      if (!inTauri() || !projectId || !status) {
        return;
      }
      setBusy(true);
      try {
        setStatus(
          await api.setCloudPrivacy({
            projectId,
            enabled: change.enabled ?? status.projectEnabled,
            offlineStudioMode: change.offline ?? status.offlineStudioMode,
            blurFaces: change.blur ?? status.blurFaces,
          }),
        );
      } catch (error) {
        report(error);
      } finally {
        setBusy(false);
      }
    },
    [projectId, report, status],
  );

  const setCap = useCallback(
    async (capUsd: number) => {
      if (!inTauri() || !projectId) {
        return;
      }
      setBusy(true);
      try {
        setSpend(
          await api.setCloudBudget({
            projectId,
            capUsd,
            monthCapUsd: Math.max(capUsd * 20, capUsd),
            hardStop: true,
          }),
        );
      } catch (error) {
        report(error);
      } finally {
        setBusy(false);
      }
    },
    [projectId, report],
  );

  const reasons = disabledReasons(status);

  return (
    <section className="ai-keys-panel" aria-label="AI keys">
      <h2>AI keys</h2>

      <p className="ai-keys-intro">
        AURA works completely without this. A key adds extra reasoning on top of the models that
        ship with the app, and you pay your provider directly for what it uses.
      </p>

      <label className="ai-keys-provider">
        Provider
        <select
          disabled={busy}
          value={provider}
          onChange={(event) => setProvider(event.currentTarget.value)}
        >
          {PROVIDER_CHOICES.map((choice) => (
            <option key={choice} value={choice}>
              {providerLabel(choice)}
            </option>
          ))}
        </select>
      </label>

      {provider === 'compat' ? (
        <label className="ai-keys-endpoint">
          Server address
          <input
            type="text"
            placeholder="http://127.0.0.1:11434"
            value={endpoint}
            disabled={busy}
            onChange={(event) => setEndpoint(event.currentTarget.value)}
          />
        </label>
      ) : null}

      <label className="ai-keys-secret">
        Key
        <input
          type="password"
          autoComplete="off"
          spellCheck={false}
          placeholder={status?.keyPresent ? status.keyFingerprint : 'paste your key'}
          value={keyText}
          disabled={busy}
          onChange={(event) => setKeyText(event.currentTarget.value)}
        />
      </label>

      <p className="ai-keys-storage">
        {status?.keyPresent
          ? `Saved as ${status.keyFingerprint} in this computer's secure key store (${status.keyStore}).`
          : "Keys are stored in this computer's secure key store, never in your catalog and never in a log."}
      </p>

      <div className="ai-keys-actions">
        <button type="button" disabled={busy || keyText.trim().length === 0} onClick={() => void save()}>
          Save key
        </button>
        <button type="button" disabled={busy || checking || !status?.keyPresent} onClick={() => void check()}>
          Check
        </button>
        <button type="button" disabled={busy || !status?.keyPresent} onClick={() => void forget()}>
          Forget key
        </button>
      </div>
      {checkResult ? <p className="ai-keys-check">{checkResult}</p> : null}

      <h3>Privacy</h3>
      <label className="ai-keys-switch">
        <input
          type="checkbox"
          checked={status?.projectEnabled ?? false}
          disabled={busy || !projectId}
          onChange={(event) => void setPrivacy({ enabled: event.currentTarget.checked })}
        />
        Use cloud AI for this job
      </label>
      <label className="ai-keys-switch">
        <input
          type="checkbox"
          checked={status?.blurFaces ?? false}
          disabled={busy || !projectId}
          onChange={(event) => void setPrivacy({ blur: event.currentTarget.checked })}
        />
        Blur faces before anything is sent
      </label>
      <label className="ai-keys-switch">
        <input
          type="checkbox"
          checked={status?.offlineStudioMode ?? false}
          disabled={busy || !projectId}
          onChange={(event) => void setPrivacy({ offline: event.currentTarget.checked })}
        />
        Offline studio mode - nothing leaves this computer, ever
      </label>
      <p className="ai-keys-retention">
        Only small copies leave your computer: thumbnails no larger than 768 pixels and a short
        camera summary. Your original files, their names, and where they were taken never do.
      </p>

      <h3>Spending</h3>
      <p className="ai-keys-spend">{spendSentence(spend)}</p>
      {spend ? (
        <progress
          className="ai-keys-meter"
          max={Math.max(spend.capUsd, 0.01)}
          value={Math.min(spend.spentUsd, spend.capUsd)}
        />
      ) : null}
      <div className="ai-keys-caps">
        {[1, 5, 20].map((cap) => (
          <button key={cap} type="button" disabled={busy || !projectId} onClick={() => void setCap(cap)}>
            Cap at {money(cap)}
          </button>
        ))}
      </div>
      {spend && spend.downgrades > 0 ? (
        <p className="ai-keys-downgrades">
          {spend.downgrades} decisions used a cheaper model to stay inside the limit.
        </p>
      ) : null}
      {spend && spend.fallbacks > 0 ? (
        <p className="ai-keys-fallbacks">
          {spend.fallbacks} decisions used AURA&apos;s own models. Re-running with a working key
          would improve them.
        </p>
      ) : null}

      {reasons.length > 0 ? (
        <ul className="ai-keys-disabled">
          {reasons.map((reason) => (
            <li key={reason}>{reason}</li>
          ))}
        </ul>
      ) : null}

      <h3>What the AI was asked</h3>
      <ul className="ai-keys-audit">
        {calls.map((call) => (
          <li key={call.id}>{callSummary(call)}</li>
        ))}
      </ul>
      {calls.length === 0 ? <p className="ai-keys-audit-empty">No AI decisions yet.</p> : null}
    </section>
  );
}
