# AURA-ML-5076 - A style profile bundle was refused

**Severity / recovery:** see `crates/aura-core/errors.toml` for the registered values.

## What the photographer sees

Importing a `.auraprofile` file fails and says the file has been altered since it was signed,
or is not a style profile. Nothing is added to the catalog.

## What actually happened

`aura_style::profile::import` refuses a bundle for one of five reasons, checked in this order
so that the cheapest refusal happens first and nothing large is ever parsed:

1. **Too large.** Above `MAX_BUNDLE_BYTES`. A profile is a few hundred coefficients; a bundle
   at the ceiling is carrying something that is not a profile.
2. **Not a bundle.** The magic string or the schema field is missing.
3. **A future schema.** Written by a newer build. Refused rather than partially read, because a
   partially read profile is a look nobody chose.
4. **The digest does not match** the canonical bytes.
5. **The signature does not verify** against the public key embedded in the bundle.

## What the signature does and does not prove

It proves the document has not changed since it was signed by whoever holds the embedded key.
It does **not** prove who that was: there is no key distribution in this product and nothing to
check a key against (ADR-0035 decision 8). The panel shows the key fingerprint and the words
"unchanged since signing", and deliberately never shows the word "verified".

So a refusal here means corruption or tampering. It does not mean "this came from somebody
untrusted", and it cannot: there is no trust list.

## Operator steps

1. Ask for a fresh export. Transfer corruption - a bundle sent through a chat client that
   re-encoded it, or truncated by a failed upload - is the common cause by a wide margin.
2. Compare the fingerprint the sender's panel shows with the one in the refusal's context.
3. If the fingerprints match and the signature still fails, the bytes changed in transit. Send
   the file inside an archive.
4. Never suggest disabling the check. There is no flag for it and adding one would make the
   only integrity guarantee in the phase optional.
