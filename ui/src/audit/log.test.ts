import { beforeEach, describe, expect, it } from 'vitest';

import {
  audit,
  auditAsJsonl,
  clearAudit,
  drainAuditBatch,
  getAuditSnapshot,
  isQuietIpc,
  nav,
  startUiAudit,
} from './log';

describe('the audit log', () => {
  beforeEach(() => {
    clearAudit();
  });

  it('keeps entries oldest first and replaces the array for React', () => {
    audit('info', 'first');
    audit('info', 'second');
    const before = getAuditSnapshot();
    audit('info', 'third');
    expect(getAuditSnapshot()).not.toBe(before);
    expect(getAuditSnapshot().map((entry) => entry.text)).toEqual(['first', 'second', 'third']);
  });

  it('redacts a value that looks like an API key in both text and detail', () => {
    audit('info', 'saved sk-abcdefghijklmnopqrstuvwxyz123456', {
      model: 'glm-5.3-flash',
      apiKey: 'sk-abcdefghijklmnopqrstuvwxyz123456',
    });
    const entry = getAuditSnapshot()[0];
    expect(entry?.text).toContain('[redacted]');
    expect(entry?.text).not.toContain('sk-abc');
    expect(entry?.detail?.['apiKey']).toBe('[redacted]');
    expect(entry?.detail?.['model']).toBe('glm-5.3-flash');
  });

  it('drains each entry to the file relay exactly once, and the cursor survives trimming', () => {
    startUiAudit();
    for (let index = 0; index < 10; index += 1) {
      audit('ipc', `command-${String(index)}`);
    }
    const drained = drainAuditBatch();
    expect(drained.length).toBeGreaterThanOrEqual(10);
    expect(drainAuditBatch()).toEqual([]);
    // The session line from startUiAudit may have been drained too - what must hold
    // is that a second drain never re-sends the first batch.
  });

  it('records section movements only when the section changed', () => {
    nav('develop', 'develop');
    expect(getAuditSnapshot().filter((entry) => entry.kind === 'nav')).toHaveLength(0);
    nav('develop', 'cull');
    const moved = getAuditSnapshot().filter((entry) => entry.kind === 'nav');
    expect(moved).toHaveLength(1);
    expect(moved[0]?.detail?.['from']).toBe('develop');
  });

  it('names the high-frequency reads and nothing else', () => {
    expect(isQuietIpc('get_preview')).toBe(true);
    expect(isQuietIpc('ingest_progress')).toBe(true);
    expect(isQuietIpc('cull_project')).toBe(false);
    expect(isQuietIpc('photo_auto_edit')).toBe(false);
  });

  it('renders itself as one JSON value per line, parseable both ways', () => {
    audit('click', 'Cull', { section: 'workspace-nav' });
    const lines = auditAsJsonl().split('\n');
    expect(lines).toHaveLength(1);
    expect(JSON.parse(lines[0] ?? '{}')).toMatchObject({ kind: 'click', text: 'Cull' });
  });
});
