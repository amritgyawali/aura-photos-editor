import { useCallback, useEffect, useState } from 'react';

import { api, asIpcError, inTauri } from '../ipc/client';
import type { HardwarePlanDto, InferStatsDto, ModelStatusDto } from '../ipc/types';

export type HardwarePanelProps = {
  onError?: (error: { code: string; message: string }) => void;
};

/** The providers a user may choose by hand, in preference order. */
export const PROVIDER_CHOICES = ['auto', 'tensorrt', 'cuda', 'directml', 'coreml', 'cpu'] as const;

/** The label a photographer reads, rather than the identifier the plan stores. */
export function providerLabel(ep: string): string {
  switch (ep) {
    case 'tensorrt':
      return 'NVIDIA (TensorRT)';
    case 'cuda':
      return 'NVIDIA (CUDA)';
    case 'directml':
      return 'Windows graphics (DirectML)';
    case 'coreml':
      return 'Apple silicon (Core ML)';
    case 'cpu':
      return 'Processor';
    case 'auto':
      return 'Choose automatically';
    default:
      return ep;
  }
}

/**
 * One line per model: which version is live, and what happened to the rest.
 *
 * A rejected version is the interesting case. It means an update was installed,
 * failed its first real use and was rolled back, and the photographer is running
 * the older model - which is exactly the state a support conversation needs to
 * start from rather than discover.
 */
export function modelSummary(model: ModelStatusDto): string {
  if (model.pendingVersion) {
    return `${model.pendingVersion} installed, not yet proved`;
  }
  if (model.activeVersion) {
    const rolledBack =
      model.rejectedVersions.length > 0
        ? ` (${model.rejectedVersions.join(', ')} rolled back)`
        : '';
    return `${model.activeVersion} in use${rolledBack}`;
  }
  return `${model.version} pinned, not yet used`;
}

/**
 * Settings > Hardware: what AURA found, what it is using, and what the user may
 * change.
 *
 * Unavailable providers are listed with their reasons rather than hidden. "AURA
 * cannot see my graphics card" is the support question this panel exists to
 * answer, and a panel that shows only what works answers it with silence.
 */
export function HardwarePanel({ onError }: HardwarePanelProps): JSX.Element {
  const [plan, setPlan] = useState<HardwarePlanDto | null>(null);
  const [models, setModels] = useState<ModelStatusDto[]>([]);
  const [stats, setStats] = useState<InferStatsDto | null>(null);
  const [busy, setBusy] = useState(false);
  const [warmup, setWarmup] = useState<string | null>(null);

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
      setPlan(await api.hardwarePlan());
    } catch (error) {
      report(error);
    }
    // Models and counters are separate calls on purpose: a model pack that fails
    // its signature check must not stop the panel showing the hardware.
    try {
      setModels(await api.listModels());
      setStats(await api.inferStats());
    } catch (error) {
      report(error);
    }
  }, [report]);

  useEffect(() => {
    void refresh();
  }, [refresh]);

  const recheck = useCallback(async () => {
    if (!inTauri()) {
      return;
    }
    setBusy(true);
    try {
      setPlan(await api.recheckHardware());
    } catch (error) {
      report(error);
    } finally {
      setBusy(false);
    }
  }, [report]);

  const choose = useCallback(
    async (ep: string) => {
      if (!inTauri()) {
        return;
      }
      setBusy(true);
      try {
        setPlan(await api.setExecutionProvider({ ep }));
      } catch (error) {
        report(error);
      } finally {
        setBusy(false);
      }
    },
    [report],
  );

  const warm = useCallback(async () => {
    if (!inTauri()) {
      return;
    }
    setBusy(true);
    setWarmup('Loading models...');
    try {
      const report_ = await api.warmupModels();
      setWarmup(`${report_.loaded} models ready in ${report_.elapsedMs} ms`);
      setStats(await api.inferStats());
    } catch (error) {
      setWarmup(null);
      report(error);
    } finally {
      setBusy(false);
    }
  }, [report]);

  return (
    <section className="hardware-panel" aria-label="Hardware">
      <h2>Hardware</h2>

      <p className="hardware-selected">
        AI runs on <strong>{plan ? providerLabel(plan.selectedEp) : '-'}</strong>
        {plan && !plan.probed ? ' (safe settings; this computer was not measured)' : ''}
      </p>
      {plan?.gpu ? <p className="hardware-gpu">{plan.gpu}</p> : null}

      <ul className="hardware-scores">
        {plan?.probeScoresMs.map((score) => (
          <li key={score.ep}>
            {providerLabel(score.ep)}: {score.medianMs.toFixed(1)} ms
          </li>
        ))}
      </ul>

      {plan && plan.unavailable.length > 0 ? (
        <details className="hardware-unavailable">
          <summary>{plan.unavailable.length} unavailable</summary>
          <ul>
            {plan.unavailable.map((note) => (
              <li key={note.ep}>
                {providerLabel(note.ep)}: {note.reason}
              </li>
            ))}
          </ul>
        </details>
      ) : null}

      {plan && plan.setAside.length > 0 ? (
        <ul className="hardware-set-aside">
          {plan.setAside.map((note) => (
            <li key={note.ep}>
              {providerLabel(note.ep)} was set aside - {note.reason}
            </li>
          ))}
        </ul>
      ) : null}

      <label className="hardware-choice">
        Use
        <select
          disabled={busy}
          value={plan?.overrideEp ?? 'auto'}
          onChange={(event) => void choose(event.currentTarget.value)}
        >
          {PROVIDER_CHOICES.map((ep) => (
            <option key={ep} value={ep}>
              {providerLabel(ep)}
            </option>
          ))}
        </select>
      </label>

      <div className="hardware-actions">
        <button type="button" disabled={busy} onClick={() => void recheck()}>
          Re-check hardware
        </button>
        <button type="button" disabled={busy} onClick={() => void warm()}>
          Prepare AI models
        </button>
      </div>
      {warmup ? <p className="hardware-warmup">{warmup}</p> : null}

      <h3>Models</h3>
      <ul className="hardware-models">
        {models.map((model) => (
          <li key={model.name}>
            <span className="model-name">{model.name}</span> - {modelSummary(model)}
            {model.int8Forbidden ? <span className="model-note"> full precision only</span> : null}
          </li>
        ))}
      </ul>

      {stats ? (
        <p className="hardware-stats">
          {stats.residentSessions} models loaded, {stats.requests} requests,{' '}
          {stats.downshifts} batch reductions, {stats.peakMemoryMb} MB peak
        </p>
      ) : null}
    </section>
  );
}
