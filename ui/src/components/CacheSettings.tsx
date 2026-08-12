import { useCallback, useEffect, useState } from 'react';

import { api, asIpcError, inTauri } from '../ipc/client';
import type { CacheStatsDto } from '../ipc/types';

export type CacheSettingsProps = {
  projectId: string | null;
  onError?: (error: { code: string; message: string }) => void;
};

const GIGABYTE = 1024 * 1024 * 1024;

/** Byte counts in the units a photographer thinks in. */
export function formatBytes(bytes: number): string {
  if (bytes >= GIGABYTE) {
    return `${(bytes / GIGABYTE).toFixed(1)} GB`;
  }
  if (bytes >= 1024 * 1024) {
    return `${Math.round(bytes / (1024 * 1024))} MB`;
  }
  return `${Math.round(bytes / 1024)} kB`;
}

/**
 * "Previews use X GB of Y", a slider, and one button that throws it all away.
 *
 * The purge button is deliberately unguarded by a confirmation dialog: nothing
 * in the cache is truth, every entry can be rebuilt from the original file, and
 * a photographer who is short of disk space at 2 a.m. should not have to read a
 * warning to get it back.
 */
export function CacheSettings({ projectId, onError }: CacheSettingsProps): JSX.Element | null {
  const [stats, setStats] = useState<CacheStatsDto | null>(null);
  const [busy, setBusy] = useState(false);

  const refresh = useCallback(async () => {
    if (!projectId || !inTauri()) {
      return;
    }
    try {
      setStats(await api.previewStats(projectId));
    } catch (error) {
      const ipc = asIpcError(error);
      onError?.({ code: ipc.code, message: ipc.message });
    }
  }, [onError, projectId]);

  useEffect(() => {
    void refresh();
  }, [refresh]);

  const changeBudget = useCallback(
    async (gigabytes: number) => {
      if (!projectId || !inTauri()) {
        return;
      }
      setBusy(true);
      try {
        setStats(
          await api.setCacheBudget({ projectId, budgetBytes: Math.round(gigabytes * GIGABYTE) }),
        );
      } catch (error) {
        const ipc = asIpcError(error);
        onError?.({ code: ipc.code, message: ipc.message });
      } finally {
        setBusy(false);
      }
    },
    [onError, projectId],
  );

  const purge = useCallback(async () => {
    if (!projectId || !inTauri()) {
      return;
    }
    setBusy(true);
    try {
      setStats(await api.purgeCache(projectId));
    } catch (error) {
      const ipc = asIpcError(error);
      onError?.({ code: ipc.code, message: ipc.message });
    } finally {
      setBusy(false);
    }
  }, [onError, projectId]);

  if (!projectId) {
    return null;
  }

  const used = stats ? formatBytes(stats.bytesUsed) : '-';
  const budget = stats ? formatBytes(stats.budgetBytes) : '-';
  const fill = stats && stats.budgetBytes > 0 ? stats.bytesUsed / stats.budgetBytes : 0;
  const hitRate = stats ? Math.round(stats.hitRate * 100) : 0;

  return (
    <section className="cache-settings" aria-label="Preview cache">
      <h2>Previews</h2>
      <p className="cache-usage">
        Previews use {used} of {budget}
      </p>
      <div
        className="cache-bar"
        role="progressbar"
        aria-valuemin={0}
        aria-valuemax={100}
        aria-valuenow={Math.round(fill * 100)}
      >
        <div className="cache-bar-fill" style={{ width: `${Math.min(100, fill * 100)}%` }} />
      </div>
      <p className="cache-hit-rate">{hitRate}% of preview requests were already cached</p>

      <label className="cache-budget">
        Cache limit
        <input
          type="range"
          min={1}
          max={200}
          step={1}
          disabled={busy}
          value={stats ? Math.max(1, Math.round(stats.budgetBytes / GIGABYTE)) : 40}
          onChange={(event) => void changeBudget(Number(event.currentTarget.value))}
        />
      </label>

      <button type="button" disabled={busy} onClick={() => void purge()}>
        Delete cached previews
      </button>
    </section>
  );
}
