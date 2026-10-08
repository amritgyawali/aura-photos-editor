// AURA licence keys, byte-for-byte the format `crates/aura-licence` reads. ADR-0105, ADR-0107.
//
// A key is `AURA1.<payload>.<signature>`: the licence as compact JSON with its fields in the order
// the Rust struct declares them, and an ed25519 signature over exactly those bytes, both base64url
// without padding. Ed25519 is deterministic, so the same licence always yields the same key - the
// shop stores nothing and can hand a customer their key again at any time.
import { createPrivateKey, sign } from 'node:crypto';

const PKCS8_ED25519_PREFIX = Buffer.from('302e020100300506032b657004220420', 'hex');

/** The vendor key from its 32-byte seed, given as base64 (standard or url-safe). */
export function vendorKey(seedBase64) {
  const seed = Buffer.from(String(seedBase64 || '').trim(), 'base64');
  if (seed.length !== 32) throw new Error('AURA_VENDOR_KEY must be the 32-byte vendor seed, base64-encoded');
  return createPrivateKey({ key: Buffer.concat([PKCS8_ED25519_PREFIX, seed]), format: 'der', type: 'pkcs8' });
}

const ymd = /^\d{4}-\d{2}-\d{2}$/;

/** Sign a licence: { id, name, email, edition, issued: 'YYYY-MM-DD', expires?: 'YYYY-MM-DD' }. */
export function issue(licence, key) {
  const { id, name, email, edition, issued, expires } = licence;
  if (!name || !edition || !ymd.test(issued) || (expires != null && !ymd.test(expires))) {
    throw new Error('a licence needs a name, an edition and YYYY-MM-DD dates');
  }
  // Field order matters: it is the order serde writes, and the signature covers these bytes.
  const body = { id: id ?? '', name, email: email ?? '', edition, issued };
  if (expires != null) body.expires = expires;
  const payload = Buffer.from(JSON.stringify(body), 'utf8');
  const signature = sign(null, payload, key);
  return `AURA1.${payload.toString('base64url')}.${signature.toString('base64url')}`;
}

/** `YYYY-MM-DD` of an ISO timestamp, optionally moved by whole days. */
export function day(iso, plusDays = 0) {
  const date = new Date(iso);
  if (Number.isNaN(date.getTime())) throw new Error(`not a date: ${iso}`);
  date.setUTCDate(date.getUTCDate() + plusDays);
  return date.toISOString().slice(0, 10);
}
