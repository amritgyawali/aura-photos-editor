import assert from 'node:assert/strict';
import { test } from 'node:test';
import { day, issue, vendorKey } from '../lib/licence.js';
import { owedKey } from '../lib/owed.js';

// The same seed and licence as `crates/aura-licence` uses for its cross-format test: the Rust
// test asserts this exact string, so the two sides cannot drift apart.
const SEED = Buffer.alloc(32, 7).toString('base64');
const FIXTURE = { id: 'sub_01crosscheck', name: 'Asha Studio', email: 'asha@example.com', edition: 'pro', issued: '2026-10-08', expires: '2026-11-15' };
export const CROSS_CHECK_KEY = 'AURA1.eyJpZCI6InN1Yl8wMWNyb3NzY2hlY2siLCJuYW1lIjoiQXNoYSBTdHVkaW8iLCJlbWFpbCI6ImFzaGFAZXhhbXBsZS5jb20iLCJlZGl0aW9uIjoicHJvIiwiaXNzdWVkIjoiMjAyNi0xMC0wOCIsImV4cGlyZXMiOiIyMDI2LTExLTE1In0.wQQ-xPBkezUha17sAt21Mp5KUrOYLXH09shvgG4jowttQU_KsPaG3kO_VY5w8Clvk9hjZiCsujEpAc0f3bUlAA';

test('a key has the format the application reads, and is deterministic', () => {
  const key = vendorKey(SEED);
  const a = issue(FIXTURE, key);
  assert.equal(a, issue(FIXTURE, key));
  assert.equal(a, CROSS_CHECK_KEY);
  assert.equal(a.split('.').length, 3);
  assert.equal(Buffer.from(a.split('.')[2], 'base64url').length, 64);
});

test('dates move by whole days in UTC', () => {
  assert.equal(day('2026-10-31T23:30:00Z', 7), '2026-11-07');
  assert.equal(day('2026-02-25T00:00:00.000000Z', 7), '2026-03-04');
});

const api = (overrides = {}) => ({
  transaction: async (id) => ({ txn_01completedorder: { status: 'completed', subscription_id: 'sub_01activesubscr' }, txn_01stillprocessing: { status: 'billed', subscription_id: null } }[id] ?? null),
  subscription: async (id) => ({
    sub_01activesubscr: { id, status: 'active', customer_id: 'ctm_1', started_at: '2026-10-08T10:00:00Z', current_billing_period: { ends_at: '2026-11-08T10:00:00Z' } },
    sub_01cancelledsub: { id, status: 'canceled', customer_id: 'ctm_1', started_at: '2026-01-01T00:00:00Z', current_billing_period: null },
  }[id] ?? null),
  customer: async () => ({ email: 'Asha@Example.com', name: 'Asha Studio' }),
  ...overrides,
});
const deps = { api: api(), key: vendorKey(SEED), graceDays: 7 };

test('an active subscription is owed a key to the end of its period plus grace', async () => {
  const owed = await owedKey({ subscription: 'sub_01activesubscr', email: ' asha@example.com ' }, deps);
  assert.equal(owed.expires, '2026-11-15');
  assert.equal(owed.subscription, 'sub_01activesubscr');
  const payload = JSON.parse(Buffer.from(owed.key.split('.')[1], 'base64url'));
  assert.deepEqual(payload, { id: 'sub_01activesubscr', name: 'Asha Studio', email: 'Asha@Example.com', edition: 'pro', issued: '2026-10-08', expires: '2026-11-15' });
});

test('right after checkout the order number finds the subscription', async () => {
  const owed = await owedKey({ transaction: 'txn_01completedorder', email: 'asha@example.com' }, deps);
  assert.equal(owed.subscription, 'sub_01activesubscr');
  await assert.rejects(owedKey({ transaction: 'txn_01stillprocessing', email: 'asha@example.com' }, deps), { status: 409 });
});

test('a wrong email, an unknown number or a cancelled subscription is refused without saying which', async () => {
  await assert.rejects(owedKey({ subscription: 'sub_01activesubscr', email: 'someone@else.com' }, deps), { status: 404 });
  await assert.rejects(owedKey({ subscription: 'sub_01doesnotexist', email: 'asha@example.com' }, deps), { status: 404 });
  await assert.rejects(owedKey({ subscription: 'sub_01cancelledsub', email: 'asha@example.com' }, deps), { status: 402 });
  await assert.rejects(owedKey({ subscription: 'not-a-sub', email: 'asha@example.com' }, deps), { status: 400 });
  await assert.rejects(owedKey({ subscription: 'sub_01activesubscr', email: '' }, deps), { status: 400 });
});

test('the website shows the same licence agreement the installer does', async () => {
  const { readFile } = await import('node:fs/promises');
  const site = await readFile(new URL('../public/eula.txt', import.meta.url), 'utf8');
  const installer = await readFile(new URL('../../legal/EULA.txt', import.meta.url), 'utf8');
  assert.equal(site, installer, 'copy legal/EULA.txt to shop/public/eula.txt');
});
