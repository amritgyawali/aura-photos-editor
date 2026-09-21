import { describe, expect, it } from 'vitest';

const nativeSources = import.meta.glob('../src-tauri/src/main.rs', {
  query: '?raw', eager: true, import: 'default',
}) as Record<string, string>;
const clients = import.meta.glob('./ipc/client.ts', {
  query: '?raw', eager: true, import: 'default',
}) as Record<string, string>;

describe('desktop command integration', () => {
  it('registers every application command called by the frontend', () => {
    const shell = Object.values(nativeSources).join('\n');
    const handler = /generate_handler!\[([\s\S]*?)\]/.exec(shell)?.[1] ?? '';
    const registered = new Set(handler.match(/\b\w+\b/g));
    const calls = [...Object.values(clients).join('\n').matchAll(
      /invoke(?:<[^;]*?>)?\(\s*['"]([^'"]+)/g,
    )].map(match => match[1] ?? '').filter(name => name !== '' && !name.startsWith('plugin:'));
    expect(calls.length).toBeGreaterThan(100);
    expect(calls.filter(name => !registered.has(name))).toEqual([]);
    expect(registered.has('automatic_start')).toBe(true);
    expect(registered.has('photo_analysis')).toBe(true);
  });
});
