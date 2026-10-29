import { useEffect, useRef, useState } from 'react';
import { activityStatus, type ActivityItem, type ActivityStatus } from '../../ipc/activity';
import { inTauri } from '../../ipc/client';
import { audit, getAuditSnapshot } from '../../audit/log';
import { describeCommand, inflightRequests, lastRequestActivity, recentRequests, type FinishedRequest, type InflightRequest } from '../../state/inflight';

const POLL_MS = 1000;
/** A lock held this long with nothing running behind it is a lock nobody will release. */
export const STALE_LOCK_MS = 15_000;
/** And the engine must have been quiet for this long too: a poller between two reads is not stuck. */
const QUIET_ENGINE_MS = 10_000;
/** A job whose progress and step have not changed for this long is flagged as possibly stuck. */
export const STALL_MS = 60_000;
/** A request still unanswered after this long is flagged as slow. */
const SLOW_REQUEST_MS = 30_000;
/** After this long a lock can be released by hand even while work is still running. */
const FORCE_UNLOCK_MS = 30_000;
const MAX_FINISHED_JOBS = 30;

const KIND: Record<string, string> = {
  folder: 'Folder',
  export: 'Export',
  edit: 'Auto edit',
  retouch: 'Retouch',
  cull: 'Cull',
  ai: 'AI',
  import: 'Import',
};

/** One reason the workspace sections are locked, as the window knows it. */
export type UiLock = { key: string; label: string; active: boolean };

type Tracked = { firstDone: number; firstAt: number; done: number; detail: string; changedAt: number; label: string; kind: string; startedAt: number };
type FinishedJob = { label: string; kind: string; endedAt: number; seconds: number };

/** `1 h 04 min`, `3 min 12 s`, `9 s`. */
export function elapsed(seconds: number): string {
  if (seconds >= 3600) return `${Math.floor(seconds / 3600)} h ${String(Math.floor((seconds % 3600) / 60)).padStart(2, '0')} min`;
  if (seconds >= 60) return `${Math.floor(seconds / 60)} min ${seconds % 60} s`;
  return `${seconds} s`;
}

const secondsSince = (from: number, now: number): number => Math.max(0, Math.floor((now - from) / 1000));
const clock = (t: number): string => new Date(t).toLocaleTimeString([], { hour: '2-digit', minute: '2-digit', second: '2-digit' });
const keyOf = (item: ActivityItem): string => `${item.kind}-${item.id}-${item.label}`;

/** Time left from the rate seen since this window first saw the job; null until it can tell. */
export function remaining(track: Pick<Tracked, 'firstDone' | 'firstAt'>, done: number, total: number, now: number): number | null {
  const moved = done - track.firstDone;
  const spent = now - track.firstAt;
  if (total <= done || moved <= 0 || spent < 5000) return null;
  return Math.round(((total - done) * spent) / moved / 1000);
}

function Line({ item, track, now }: { item: ActivityItem; track: Tracked | undefined; now: number }): JSX.Element {
  const share = item.total > 0 ? Math.min(100, Math.round((item.done / item.total) * 100)) : null;
  const left = track ? remaining(track, item.done, item.total, now) : null;
  const still = track ? now - track.changedAt : 0;
  return <li className={`status-item${still >= STALL_MS ? ' is-stalled' : ''}`}>
    <span className={`status-kind is-${item.kind}`}>{KIND[item.kind] ?? item.kind}</span>
    <span className="status-label">{item.label}</span>
    {item.detail && <span className="status-detail">{item.detail}</span>}
    {item.total > 0 && <span className="status-count">{item.done} of {item.total}{share !== null ? ` (${share}%)` : ''}</span>}
    {item.total > 0 && <progress value={item.done} max={item.total} aria-label={`${item.label}: ${item.done} of ${item.total}`} />}
    {item.seconds > 0 && <span className="status-time">{elapsed(item.seconds)}</span>}
    {left !== null && <span className="status-eta">about {elapsed(left)} left</span>}
    {still >= STALL_MS && <span className="status-stall">No change for {elapsed(Math.floor(still / 1000))} - this step may be slow or stuck</span>}
  </li>;
}

/**
 * What AURA is doing right now, on every screen, and why a section will not open.
 *
 * A long job that says nothing looks exactly like one that has stopped, so every piece of work
 * the engine is doing - a folder run, an export, an Auto edit, a full edit, a cull, a retouch, an
 * AI request, an import - is listed with the photograph and step it is on, how far it has got and
 * roughly how long is left. The strip also names every lock that is keeping the other sections
 * shut, and when a lock is held while nothing at all is running behind it, it says so and offers
 * to release it (ADR-0188). "Idle" is shown only when nothing is running and nothing is locked.
 *
 * "Details" opens the full picture: the locks and for how long, the engine's jobs, every request
 * still waiting for an answer, what finished or failed in the last minutes, and a copy of all of
 * it for a bug report.
 */
export function StatusBar({ read = activityStatus, locks = [], onUnlock, screen, now: readNow = Date.now }: {
  read?: () => Promise<ActivityStatus>;
  locks?: UiLock[];
  onUnlock?: () => void;
  screen?: string;
  now?: () => number;
}): JSX.Element | null {
  const [status, setStatus] = useState<ActivityStatus | null>(null);
  const [lost, setLost] = useState(false);
  const [open, setOpen] = useState(false);
  const [copied, setCopied] = useState<string | null>(null);
  const [, setTick] = useState(0);
  const tracked = useRef(new Map<string, Tracked>());
  const finished = useRef<FinishedJob[]>([]);
  const lockSince = useRef(new Map<string, number>());

  useEffect(() => {
    if (!inTauri() && read === activityStatus) return;
    let disposed = false;
    let timer: number | undefined;
    const poll = async (): Promise<void> => {
      try {
        const next = await read();
        if (disposed) return;
        const at = readNow();
        const seen = new Set<string>();
        for (const item of next.items) {
          const key = keyOf(item);
          seen.add(key);
          const before = tracked.current.get(key);
          if (!before) {
            tracked.current.set(key, { firstDone: item.done, firstAt: at, done: item.done, detail: item.detail, changedAt: at, label: item.label, kind: item.kind, startedAt: at - item.seconds * 1000 });
          } else if (before.done !== item.done || before.detail !== item.detail) {
            tracked.current.set(key, { ...before, done: item.done, detail: item.detail, changedAt: at });
          }
        }
        for (const [key, track] of tracked.current) {
          if (seen.has(key)) continue;
          tracked.current.delete(key);
          finished.current = [...finished.current, { label: track.label, kind: track.kind, endedAt: at, seconds: secondsSince(track.startedAt, at) }].slice(-MAX_FINISHED_JOBS);
        }
        setStatus(next);
        setLost(false);
      } catch {
        if (!disposed) setLost(true);
      }
      if (!disposed) {
        setTick(value => value + 1);
        timer = window.setTimeout(() => void poll(), POLL_MS);
      }
    };
    void poll();
    return () => { disposed = true; window.clearTimeout(timer); };
  }, [read, readNow]);

  useEffect(() => {
    if (!open) return;
    const close = (event: KeyboardEvent): void => { if (event.key === 'Escape') setOpen(false); };
    window.addEventListener('keydown', close);
    return () => window.removeEventListener('keydown', close);
  }, [open]);

  const now = readNow();
  const active = locks.filter(lock => lock.active);
  for (const lock of active) if (!lockSince.current.has(lock.key)) lockSince.current.set(lock.key, now);
  for (const key of [...lockSince.current.keys()]) if (!active.some(lock => lock.key === key)) lockSince.current.delete(key);

  if (!status && !lost && active.length === 0) return null;

  const items = status?.items ?? [];
  // A quick preview read is noise; one that has taken three seconds is something to wait for.
  const waiting = inflightRequests().filter(request => !request.quiet || now - request.startedAt >= 3000);
  const oldestLock = active.reduce((oldest, lock) => Math.min(oldest, lockSince.current.get(lock.key) ?? now), now);
  const lockedFor = active.length ? now - oldestLock : 0;
  const engineQuietFor = now - lastRequestActivity();
  const stale = active.length > 0 && !lost && items.length === 0 && waiting.length === 0
    && lockedFor >= STALE_LOCK_MS && engineQuietFor >= QUIET_ENGINE_MS;
  const stalled = items.some(item => now - (tracked.current.get(keyOf(item))?.changedAt ?? now) >= STALL_MS)
    || waiting.some(request => now - request.startedAt >= SLOW_REQUEST_MS * 2);
  const tone = lost ? 'lost' : stale ? 'stuck' : stalled ? 'slow' : items.length || active.length || waiting.length ? 'busy' : 'idle';
  const workers = status ? `Image workers: ${status.imageWorkersBusy} of ${status.imageWorkers} busy` : '';
  const lockNames = active.map(lock => lock.label).join(', ');

  const unlock = (forced: boolean): void => {
    audit('warning', forced ? 'sections unlocked by hand while work was running' : 'stale lock released by hand', {
      locks: active.map(lock => lock.key).join(','),
      lockedMs: lockedFor,
      screen: screen ?? null,
    });
    onUnlock?.();
  };

  const copy = (): void => {
    const report = {
      at: new Date(now).toISOString(),
      screen: screen ?? null,
      state: tone,
      locks: active.map(lock => ({ ...lock, heldMs: now - (lockSince.current.get(lock.key) ?? now) })),
      engine: status,
      waiting: inflightRequests().map(request => ({ ...request, waitedMs: now - request.startedAt })),
      recentRequests: recentRequests(),
      finishedJobs: finished.current,
      audit: getAuditSnapshot().slice(-200),
    };
    const text = JSON.stringify(report, null, 2);
    const done = (message: string): void => { setCopied(message); window.setTimeout(() => setCopied(null), 3000); };
    if (navigator.clipboard?.writeText) {
      navigator.clipboard.writeText(text).then(() => done('Copied. Paste it into your bug report.'), () => done('Could not copy - your system refused the clipboard.'));
    } else {
      done('Could not copy - no clipboard here.');
    }
  };

  let headline: JSX.Element;
  if (lost) headline = <span className="status-idle">Status unavailable - AURA may still be working.</span>;
  else if (stale) headline = <span className="status-stuck">Sections are locked by {lockNames}, but nothing is running.
    {onUnlock && <button type="button" className="status-unlock" onClick={() => unlock(false)}>Unlock sections</button>}</span>;
  else if (items.length) headline = <ul>{items.map((item, i) => <Line key={`${keyOf(item)}-${i}`} item={item} track={tracked.current.get(keyOf(item))} now={now} />)}</ul>;
  else if (active.length) headline = <span className="status-locked">Waiting: {lockNames} ({elapsed(Math.floor(lockedFor / 1000))}). Other sections open when it ends.</span>;
  else if (waiting.length) headline = <span className="status-waiting">Waiting for the engine: {describeCommand(waiting[0]?.command ?? '')} ({elapsed(secondsSince(waiting[0]?.startedAt ?? now, now))}){waiting.length > 1 ? ` and ${waiting.length - 1} more` : ''}</span>;
  else headline = <span className="status-idle">Idle - nothing is running.</span>;

  return <footer className={`status-bar is-${tone}${items.length ? ' is-busy' : ''}${open ? ' is-open' : ''}`} aria-label="What AURA is doing">
    {open && <StatusDetails now={now} active={active} lockSince={lockSince.current} items={items} tracked={tracked.current} waiting={waiting}
      recent={recentRequests()} finished={finished.current} workers={workers} lockedFor={lockedFor} stale={stale} lost={lost}
      onUnlock={onUnlock && lockedFor >= FORCE_UNLOCK_MS && !stale ? () => unlock(true) : undefined} onCopy={copy} copied={copied} />}
    <div className="status-strip">
      <span className={`status-light is-${tone}`} role="img" aria-label={`State: ${tone}`} />
      <div className="status-main" aria-live="polite">{headline}</div>
      <div className="status-side">
        {waiting.length > 0 && <span className="status-requests">{waiting.length} waiting</span>}
        {workers && <span className="status-workers">{workers}</span>}
        <button type="button" className="status-toggle" aria-expanded={open} onClick={() => setOpen(value => !value)}>{open ? 'Hide details' : 'Details'}</button>
      </div>
    </div>
  </footer>;
}

function StatusDetails({ now, active, lockSince, items, tracked, waiting, recent, finished, workers, lockedFor, stale, lost, onUnlock, onCopy, copied }: {
  now: number; active: UiLock[]; lockSince: Map<string, number>; items: ActivityItem[]; tracked: Map<string, Tracked>;
  waiting: InflightRequest[]; recent: FinishedRequest[]; finished: FinishedJob[]; workers: string; lockedFor: number;
  stale: boolean; lost: boolean; onUnlock?: () => void; onCopy: () => void; copied: string | null;
}): JSX.Element {
  const history = [
    ...recent.map(r => ({ at: r.startedAt + r.ms, ok: r.ok, text: describeCommand(r.command), note: r.ok ? elapsed(Math.round(r.ms / 1000)) : `failed: ${r.message ?? 'no message'}`, ms: r.ms })),
    ...finished.map(j => ({ at: j.endedAt, ok: true, text: `${KIND[j.kind] ?? j.kind}: ${j.label}`, note: `ended after ${elapsed(j.seconds)}`, ms: j.seconds * 1000 })),
  ].sort((a, b) => b.at - a.at).reduce<Array<{ at: number; ok: boolean; text: string; note: string; ms: number; times: number }>>((rows, entry) => {
    // A poller asks the same thing every second: one line with a count, not a screenful.
    const last = rows[rows.length - 1];
    if (last && last.ok && entry.ok && last.text === entry.text) last.times += 1;
    else rows.push({ ...entry, times: 1 });
    return rows;
  }, []).slice(0, 30);
  return <section className="status-details" aria-label="Activity details">
    <div className="status-section">
      <h3>Sections</h3>
      {active.length === 0
        ? <p className="status-ok">Every section can be opened.</p>
        : <>
          <ul className="status-list">{active.map(lock => <li key={lock.key}><strong>{lock.label}</strong><span>locked for {elapsed(secondsSince(lockSince.get(lock.key) ?? now, now))}</span></li>)}</ul>
          {stale && <p className="status-warn">Nothing is running in the engine and no answer is awaited, so this lock will not release by itself. Unlocking is safe.</p>}
          {!stale && lockedFor < FORCE_UNLOCK_MS && <p>The other sections open again as soon as this finishes.</p>}
          {onUnlock && <p className="status-warn">Still locked after {elapsed(Math.floor(lockedFor / 1000))}. You can unlock the sections now; the work keeps running in the background and finishes on its own. <button type="button" onClick={onUnlock}>Unlock anyway</button></p>}
        </>}
    </div>
    <div className="status-section">
      <h3>Engine {workers && <small>{workers}</small>}</h3>
      {lost ? <p className="status-warn">The engine is not answering the status check. It may be busy with a very heavy step.</p>
        : items.length === 0 ? <p className="status-ok">No jobs running.</p>
          : <ul className="status-list">{items.map((item, i) => {
            const track = tracked.get(keyOf(item));
            const left = track ? remaining(track, item.done, item.total, now) : null;
            const still = track ? now - track.changedAt : 0;
            return <li key={`${keyOf(item)}-${i}`} className={still >= STALL_MS ? 'is-stalled' : undefined}>
              <strong>{KIND[item.kind] ?? item.kind}: {item.label}</strong>
              <span>{item.detail || 'Working'}</span>
              <span>{item.total > 0 ? `${item.done} of ${item.total}` : 'no count'} · running {elapsed(item.seconds)}{left !== null ? ` · about ${elapsed(left)} left` : ''}</span>
              <span>Last change {elapsed(Math.floor(still / 1000))} ago</span>
            </li>;
          })}</ul>}
    </div>
    <div className="status-section">
      <h3>Waiting for answers</h3>
      {waiting.length === 0 ? <p className="status-ok">Nothing awaited.</p>
        : <ul className="status-list">{waiting.map(request => {
          const waited = now - request.startedAt;
          return <li key={request.id} className={waited >= SLOW_REQUEST_MS ? 'is-stalled' : undefined}>
            <strong>{describeCommand(request.command)}</strong><span><code>{request.command}</code> · {elapsed(Math.floor(waited / 1000))}{waited >= SLOW_REQUEST_MS ? ' - slow' : ''}</span>
          </li>;
        })}</ul>}
    </div>
    <div className="status-section">
      <h3>Recent</h3>
      {history.length === 0 ? <p className="status-ok">Nothing yet.</p>
        : <ol className="status-history">{history.map((entry, i) => <li key={i} className={entry.ok ? undefined : 'is-failed'}>
          <time>{clock(entry.at)}</time><span>{entry.ok ? '✓' : '✕'} {entry.text}{entry.times > 1 ? ` ×${entry.times}` : ''}</span><span>{entry.ok && entry.ms < 1000 ? `${Math.round(entry.ms)} ms` : entry.note}</span>
        </li>)}</ol>}
    </div>
    <div className="status-actions">
      <button type="button" onClick={onCopy}>Copy diagnostics</button>
      {copied && <span role="status">{copied}</span>}
      <span className="status-hint">The full log is under Advanced → Activity log. Esc closes this panel.</span>
    </div>
  </section>;
}
