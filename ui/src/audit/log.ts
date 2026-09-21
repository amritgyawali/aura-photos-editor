/**
 * The application's own audit log: every click, every section, every command, every error.
 *
 * ## Why this exists in the webview rather than only in the backend
 *
 * The shell already writes `tracing` output to a file, but that records what the engine
 * decided, not what the photographer *did*: which panel they opened, which button they
 * pressed, which command they started and how long it took, and which error they saw on
 * screen. The gap between "the log says the pass ran" and "the user pressed Run at 23:14
 * and watched it fail at 23:19" is exactly the gap a support case lives in.
 *
 * ## How it is complete without being rewritten into every component
 *
 * There are two choke points and both are wrapped rather than instrumented. Every IPC
 * call in the application already goes through the single `invoke` helper in
 * `ipc/client.ts`, so wrapping that logs the whole command surface - 220 names, current
 * and future - without touching a call site. Every interaction the photographer makes is
 * a click or a key, so one capture-phase listener on the document sees them all, before
 * any handler runs. Components do not opt in; there is nothing to forget to opt in.
 *
 * ## What it deliberately is not
 *
 * - Not a second ledger. The decision ledger (phase 13) records what the *product*
 *   decided about photographs; this records what the *front end* experienced. Its rows
 *   are for debugging the window, and they never enter the catalog.
 * - Not durable storage. It keeps a ring buffer in memory mirrored to localStorage so a
 *   crash does not erase the session, and the panel offers copy and download for the
 *   moment a human needs the file. The backend's `logs/aura.log*` holds the same period
 *   engine-side.
 * - Not a place for secrets. Values are redacted on write: anything shaped like an API
 *   key and any detail whose field name says `key`, `token` or `secret` becomes
 *   `[redacted]` before it can reach storage. A log that leaks the thing it is watching
 *   is a worse bug than the one it helps find.
 */

export type AuditKind =
  | 'session'
  | 'click'
  | 'nav'
  | 'ipc'
  | 'ipc-error'
  | 'error'
  | 'warning'
  | 'info';

export type AuditDetail = Record<string, string | number | boolean | null>;

export type AuditEntry = {
  /** Milliseconds since the epoch, so it lines up with the backend's file log. */
  t: number;
  kind: AuditKind;
  /** The headline: a command name, a button label, an error code. */
  text: string;
  detail: AuditDetail | null;
};

const MAX_ENTRIES = 3000;
const STORAGE_KEY = 'aura.audit.v1';
/** ~1.5 MB of JSON: localStorage quotas are per-origin and typically 5-10 MB. */
const MAX_STORED_CHARS = 1_500_000;

/** The most recent entries, oldest first. Replaced, never mutated, for React. */
let entries: AuditEntry[] = [];
let listeners: Set<() => void> = new Set();
let saveTimer: ReturnType<typeof setTimeout> | null = null;

function notify(): void {
  for (const listener of listeners) {
    listener();
  }
}

function trim(): void {
  if (entries.length > MAX_ENTRIES) {
    const dropped = entries.length - MAX_ENTRIES;
    entries = entries.slice(dropped);
    // The relay indexes into this array; dropping the head moves everything it has
    // not yet drained forward, and the cursor has to move with it or lines near the
    // front are skipped forever.
    relayCursor = Math.max(0, relayCursor - dropped);
  }
}

const SECRET_VALUE = /(\b(?:sk|pk|ghp|gho|xox[aprsb]|AKIA)[A-Za-z0-9_-]{6,}\b)/g;
const SECRET_FIELD = /key|token|secret|password|credential/i;

/**
 * Redact before anything is stored or shown: a value that could be a credential, and
 * any field whose *name* says it is one, whichever came in.
 */
function redactText(value: string): string {
  return value.replace(SECRET_VALUE, '[redacted]');
}

function redactDetail(detail: AuditDetail | null): AuditDetail | null {
  if (detail === null) {
    return null;
  }
  const out: AuditDetail = {};
  for (const [field, value] of Object.entries(detail)) {
    if (SECRET_FIELD.test(field)) {
      out[field] = '[redacted]';
    } else if (typeof value === 'string') {
      out[field] = redactText(value);
    } else {
      out[field] = value;
    }
  }
  return out;
}

/** Write one line to the log. Everything below the choke points ends up here. */
export function audit(kind: AuditKind, text: string, detail: AuditDetail | null = null): void {
  entries = [...entries, { t: Date.now(), kind, text: redactText(text), detail: redactDetail(detail) }];
  trim();
  schedulePersist();
  notify();
}

/**
 * A section change. Clicks already record the press; this records the *place*, so
 * the log reads "Develop → Cull" as a movement rather than as one more button.
 */
export function nav(from: string, to: string): void {
  if (from !== to) {
    audit('nav', to, { from });
  }
}

function schedulePersist(): void {
  if (saveTimer !== null) {
    return;
  }
  saveTimer = setTimeout(() => {
    saveTimer = null;
    persistNow();
  }, 1000);
}

export function persistNow(): void {
  try {
    let serialised = JSON.stringify(entries);
    while (serialised.length > MAX_STORED_CHARS && entries.length > 50) {
      const dropped = Math.ceil(entries.length / 4);
      entries = entries.slice(dropped);
      relayCursor = Math.max(0, relayCursor - dropped);
      serialised = JSON.stringify(entries);
    }
    window.localStorage.setItem(STORAGE_KEY, serialised);
  } catch {
    // Storage that will not take it (quota, private mode, a disk in trouble) loses the
    // mirror only. The in-memory ring is the live truth and the panel still shows it.
  }
}

function hydrate(): void {
  try {
    const stored = window.localStorage.getItem(STORAGE_KEY);
    if (stored === null) {
      return;
    }
    const parsed: unknown = JSON.parse(stored);
    if (Array.isArray(parsed)) {
      const restored = parsed.filter(isEntry);
      if (restored.length > 0) {
        // The mirror holds a previous life of the window; the file already has what
        // the relay managed to push, and re-pushing a whole old session at startup
        // would drown the new one. So the cursor sits *past* the restored entries and
        // only a session marker - written here, after it - reaches the file.
        entries = restored;
        relayCursor = restored.length;
        audit('info', 'earlier session mirrored', { lines: restored.length });
      }
    }
  } catch {
    // A corrupt mirror is an empty mirror.
  }
}

function isEntry(candidate: unknown): candidate is AuditEntry {
  if (typeof candidate !== 'object' || candidate === null) {
    return false;
  }
  const value = candidate as Record<string, unknown>;
  return (
    typeof value['t'] === 'number' &&
    typeof value['kind'] === 'string' &&
    typeof value['text'] === 'string'
  );
}

/** Snapshot for `useSyncExternalStore`: a stable reference between writes. */
export function getAuditSnapshot(): AuditEntry[] {
  return entries;
}

export function subscribeAudit(listener: () => void): () => void {
  listeners.add(listener);
  return () => {
    listeners.delete(listener);
  };
}

export function clearAudit(): void {
  entries = [];
  relayCursor = 0;
  try {
    window.localStorage.removeItem(STORAGE_KEY);
  } catch {
    // As above: the mirror, not the truth.
  }
  notify();
}

export function auditAsJsonl(): string {
  return entries.map((entry) => JSON.stringify(entry)).join('\n');
}

// ---------------------------------------------------------------------------
// The file relay. `ipc/client.ts` owns the only pipe to the shell, so the log
// module queues drained batches and the client decides when and how to send.
// ---------------------------------------------------------------------------

let relayCursor = 0;

/** Everything written since the last drain, oldest first. An empty array is not sent. */
export function drainAuditBatch(): AuditEntry[] {
  if (relayCursor >= entries.length) {
    return [];
  }
  const batch = entries.slice(relayCursor);
  relayCursor = entries.length;
  return batch;
}

/**
 * The high-frequency read-only commands.
 *
 * A grid scrolling a four-thousand-frame wedding fires `get_preview` dozens of times a
 * second; an import polls `ingest_progress` every 400 ms. Logging those verbatim would
 * push a day of clicks out of the ring and fill the file with traffic that says nothing -
 * a log nobody can read is not a log. Every *other* command is recorded; failures and
 * slow successes are always recorded, whatever the name.
 */
const QUIET_IPC: ReadonlySet<string> = new Set([
  'get_preview',
  'render_image',
  'ingest_progress',
  'autopilot_progress',
  'list_images',
  'ai_setup_status',
]);

export function isQuietIpc(name: string): boolean {
  return QUIET_IPC.has(name);
}

// ---------------------------------------------------------------------------
// The document-level listeners. One file install called from `main.tsx` before
// React renders, so a failure during the first paint is already a logged event.
// ---------------------------------------------------------------------------

/** The shortest human name for an element: what it says, or what it is labelled. */
function labelFor(element: Element): string {
  const aria = element.getAttribute('aria-label');
  if (aria !== null && aria.trim() !== '') {
    return aria.trim().slice(0, 80);
  }
  const text = (element.textContent ?? '').trim().replace(/\s+/g, ' ');
  if (text !== '') {
    return text.slice(0, 80);
  }
  const placeholder = element.getAttribute('placeholder');
  if (placeholder !== null && placeholder !== '') {
    return placeholder.slice(0, 80);
  }
  const id = element.id;
  if (id !== '') {
    return `#${id}`;
  }
  return `<${element.tagName.toLowerCase()}>`;
}

const INTERACTIVE = 'button, a[href], input, select, textarea, [role="button"], [role="link"], [role="menuitem"], [role="tab"]';

function onDocumentClick(event: MouseEvent): void {
  const target = event.target;
  if (!(target instanceof Element)) {
    return;
  }
  const interactive = target.closest(INTERACTIVE);
  if (interactive === null) {
    // Clicks on empty surface are noise; a photograph cell is an interactive element,
    // and a click that did nothing to anything interactive is not a user action.
    return;
  }
  audit('click', labelFor(interactive), {
    tag: interactive.tagName.toLowerCase(),
    section: sectionOf(interactive),
  });
}

/** Which part of the application a click happened in, from the nearest landmark. */
function sectionOf(element: Element): string {
  const named = element.closest('[class]');
  const order: string[] = [
    'welcome',
    'step-bar',
    'stage-tabs',
    'stage',
    'main',
    'topbar',
    'photo-editor',
    'develop-workspace',
  ];
  for (const marker of order) {
    if (element.closest(`.${marker}`) !== null) {
      return marker;
    }
  }
  return named !== null ? (named.className.split(' ')[0] ?? 'document') : 'document';
}

function onWindowError(event: ErrorEvent): void {
  audit('error', event.message ?? 'unknown window error', {
    source: event.filename ?? '',
    line: event.lineno,
    col: event.colno,
  });
}

function onUnhandledRejection(event: PromiseRejectionEvent): void {
  const reason = event.reason;
  let text = 'unhandled rejection';
  const detail: AuditDetail = {};
  if (typeof reason === 'object' && reason !== null) {
    const record = reason as Record<string, unknown>;
    const code = record['code'];
    const message = record['message'];
    if (typeof code === 'string') {
      text = `${code}: ${String(message ?? 'no message')}`.slice(0, 200);
    } else if (typeof message === 'string') {
      text = message.slice(0, 200);
    }
    if (typeof record['runbookUrl'] === 'string') {
      detail['runbook'] = record['runbookUrl'];
    }
  } else if (typeof reason === 'string') {
    text = reason.slice(0, 200);
  }
  audit('error', text, Object.keys(detail).length > 0 ? detail : null);
}

let installed = false;

/** Install every global listener. Idempotent: React StrictMode runs effects twice. */
export function startUiAudit(): void {
  if (installed) {
    return;
  }
  installed = true;
  hydrate();
  audit('session', 'AURA session started', {
    ua: navigator.userAgent.slice(0, 120),
    restoredEntries: Math.max(0, entries.length - 1),
  });
  document.addEventListener('click', onDocumentClick, true);
  window.addEventListener('error', onWindowError);
  window.addEventListener('unhandledrejection', onUnhandledRejection);
  document.addEventListener('visibilitychange', () => {
    audit('info', document.hidden ? 'window hidden' : 'window shown');
    if (document.hidden) {
      persistNow();
    }
  });
  window.addEventListener('pagehide', persistNow);
}
