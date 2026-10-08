# AURA-REL-12006 - A licence key was refused

**Severity / recovery:** see `crates/aura-core/errors.toml` for the registered values.

## What the photographer sees

That licence key could not be used, with the reason: not an AURA key, cut short or mistyped, for a
different version of AURA, or already ended.

## What actually happened

`aura_licence::decode` checks the key's ed25519 signature against the public key compiled into
this build. A key that does not verify was altered, truncated, or signed with a different vendor
key. Nothing was saved and any licence already on the machine is unchanged.

## What to do

Paste the whole key, starting `AURA1.`; line breaks from an email are ignored. If it still fails,
issue a fresh key with `tools/licence-issue` - it refuses to print a key the shipped build would
not accept.

## Where it comes from

ADR-0105, `docs/adr/ADR-0105-installer-signing-and-licensing.md`.
