import { useEffect, useState } from 'react';
import { activityStatus, type ActivityItem, type ActivityStatus } from '../../ipc/activity';
import { inTauri } from '../../ipc/client';

const POLL_MS = 1000;

const KIND: Record<string, string> = {
  folder: 'Folder',
  export: 'Export',
  edit: 'Auto edit',
  retouch: 'Retouch',
  ai: 'AI',
  import: 'Import',
};

/** `1 h 04 min`, `3 min 12 s`, `9 s`. */
export function elapsed(seconds: number): string {
  if (seconds >= 3600) return `${Math.floor(seconds / 3600)} h ${String(Math.floor((seconds % 3600) / 60)).padStart(2, '0')} min`;
  if (seconds >= 60) return `${Math.floor(seconds / 60)} min ${seconds % 60} s`;
  return `${seconds} s`;
}

function Line({ item }: { item: ActivityItem }): JSX.Element {
  const share = item.total > 0 ? Math.min(100, Math.round((item.done / item.total) * 100)) : null;
  return <li className="status-item">
    <span className={`status-kind is-${item.kind}`}>{KIND[item.kind] ?? item.kind}</span>
    <span className="status-label">{item.label}</span>
    {item.detail && <span className="status-detail">{item.detail}</span>}
    {item.total > 0 && <span className="status-count">{item.done} of {item.total}{share !== null ? ` (${share}%)` : ''}</span>}
    {item.total > 0 && <progress value={item.done} max={item.total} aria-label={`${item.label}: ${item.done} of ${item.total}`} />}
    {item.seconds > 0 && <span className="status-time">{elapsed(item.seconds)}</span>}
  </li>;
}

/**
 * What AURA is busy with right now, on every screen.
 *
 * A long job that says nothing looks exactly like one that has stopped, so every piece of work
 * the app is doing - a folder run, an export, an Auto edit, a retouch, an AI request, an import -
 * is listed here while it runs, with the photograph it is on and how far it has got. "Idle" is
 * shown only when nothing at all is running.
 */
export function StatusBar({ read = activityStatus }: { read?: () => Promise<ActivityStatus> }): JSX.Element | null {
  const [status, setStatus] = useState<ActivityStatus | null>(null);
  const [lost, setLost] = useState(false);
  useEffect(() => {
    if (!inTauri() && read === activityStatus) return;
    let disposed = false;
    let timer: number | undefined;
    const poll = async (): Promise<void> => {
      try {
        const next = await read();
        if (disposed) return;
        setStatus(next);
        setLost(false);
      } catch {
        if (!disposed) setLost(true);
      }
      if (!disposed) timer = window.setTimeout(() => void poll(), POLL_MS);
    };
    void poll();
    return () => { disposed = true; window.clearTimeout(timer); };
  }, [read]);
  if (!status && !lost) return null;
  const items = status?.items ?? [];
  const workers = status ? `Image workers: ${status.imageWorkersBusy} of ${status.imageWorkers} busy` : '';
  return <footer className={`status-bar${items.length ? ' is-busy' : ''}`} aria-label="What AURA is doing" aria-live="polite">
    {lost
      ? <span className="status-idle">Status unavailable - AURA may still be working.</span>
      : items.length === 0
        ? <span className="status-idle">Idle - nothing is running.</span>
        : <ul>{items.map((item, i) => <Line key={`${item.kind}-${item.id}-${i}`} item={item} />)}</ul>}
    {workers && <span className="status-workers">{workers}</span>}
  </footer>;
}
