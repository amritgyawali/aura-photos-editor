import { useCallback, useMemo, useState, useSyncExternalStore } from 'react';

import {
  audit,
  auditAsJsonl,
  clearAudit,
  getAuditSnapshot,
  persistNow,
  subscribeAudit,
  type AuditEntry,
  type AuditKind,
} from '../audit/log';

/**
 * The application's own log, in a panel.
 *
 * Everything the logger collects is useless if the only way to read it is a console
 * the shipping build does not open. This panel is a live tail with the three things
 * a person debugging a session actually reach for: a kind filter, a text search, and
 * "copy everything" / "save file" so the lines can leave the machine.
 *
 * It is deliberately last in the sidebar - it is the panel you open *because* of
 * something another panel did, and the audit records it being opened like anything
 * else, which is the point of an audit log.
 */

const KINDS: ReadonlyArray<{ id: AuditKind | 'all'; label: string }> = [
  { id: 'all', label: 'All' },
  { id: 'click', label: 'Clicks' },
  { id: 'ipc', label: 'Commands' },
  { id: 'ipc-error', label: 'Refusals' },
  { id: 'error', label: 'Errors' },
  { id: 'session', label: 'Sessions' },
];

const KIND_WORD: Record<AuditKind, string> = {
  session: 'session',
  click: 'click',
  nav: 'nav',
  ipc: 'ipc',
  'ipc-error': 'refused',
  error: 'error',
  warning: 'warn',
  info: 'info',
};

function timeOf(entry: AuditEntry): string {
  return new Date(entry.t).toLocaleTimeString(undefined, {
    hour12: false,
    hour: '2-digit',
    minute: '2-digit',
    second: '2-digit',
  });
}

function download(): void {
  // An in-memory object URL and a synthetic anchor: the Tauri shell sandbox blocks
  // `<a download>` for viewers elsewhere, but inside the webview this is the same
  // path the browser uses and it needs no command.
  const blob = new Blob([auditAsJsonl()], { type: 'application/x-ndjson' });
  const url = URL.createObjectURL(blob);
  const anchor = document.createElement('a');
  anchor.href = url;
  anchor.download = `aura-ui-log-${new Date().toISOString().slice(0, 19).replace(/[:T]/g, '-')}.jsonl`;
  anchor.click();
  URL.revokeObjectURL(url);
  audit('info', 'log downloaded', { lines: getAuditSnapshot().length });
}

export function LogPanel(): JSX.Element {
  const entries = useSyncExternalStore(subscribeAudit, getAuditSnapshot, getAuditSnapshot);
  const [kind, setKind] = useState<AuditKind | 'all'>('all');
  const [query, setQuery] = useState('');
  const [copied, setCopied] = useState(false);

  const visible = useMemo(() => {
    const needle = query.trim().toLowerCase();
    return entries
      .filter((entry) => kind === 'all' || entry.kind === kind)
      .filter(
        (entry) =>
          needle === '' ||
          entry.text.toLowerCase().includes(needle) ||
          (entry.detail !== null &&
            Object.entries(entry.detail).some(
              ([field, value]) =>
                field.toLowerCase().includes(needle) ||
                String(value).toLowerCase().includes(needle),
            )),
      )
      .slice(-500)
      .reverse();
  }, [entries, kind, query]);

  const copy = useCallback(async () => {
    try {
      await navigator.clipboard.writeText(auditAsJsonl());
      setCopied(true);
      window.setTimeout(() => {
        setCopied(false);
      }, 2000);
    } catch {
      // A clipboard that will not take it is worth one line in the thing being copied.
      audit('warning', 'clipboard refused the log copy');
    }
  }, []);

  return (
    <section className="panel log-panel" aria-label="Application log">
      <h2>Application log</h2>
      <div className="log-filters" role="group" aria-label="Log filters">
        {KINDS.map((row) => (
          <button
            key={row.id}
            type="button"
            className={kind === row.id ? 'is-open' : undefined}
            onClick={() => {
              setKind(row.id);
            }}
          >
            {row.label}
          </button>
        ))}
      </div>
      <input
        type="search"
        placeholder="Search the log…"
        aria-label="Search the log"
        value={query}
        onChange={(event) => {
          setQuery(event.target.value);
        }}
      />
      <p className="log-count">
        {visible.length.toLocaleString()} shown of {entries.length.toLocaleString()} kept
      </p>
      <ol className="log-lines">
        {visible.map((entry, index) => (
          <li key={`${entry.t}-${index}`} className={`log-line is-${entry.kind}`}>
            <span className="log-time">{timeOf(entry)}</span>
            <span className="log-kind">{KIND_WORD[entry.kind]}</span>
            <span className="log-text">
              {entry.text}
              {entry.detail !== null ? (
                <span className="log-detail">
                  {Object.entries(entry.detail)
                    .map(([field, value]) => `${field}=${String(value)}`)
                    .join(' ')}
                </span>
              ) : null}
            </span>
          </li>
        ))}
        {visible.length === 0 ? <li className="log-empty">Nothing matches.</li> : null}
      </ol>
      <div className="log-actions">
        <button type="button" onClick={() => void copy()}>
          {copied ? 'Copied' : 'Copy all'}
        </button>
        <button type="button" onClick={download}>
          Save file
        </button>
        <button
          type="button"
          className="log-clear"
          onClick={() => {
            persistNow();
            clearAudit();
          }}
        >
          Clear
        </button>
      </div>
    </section>
  );
}
