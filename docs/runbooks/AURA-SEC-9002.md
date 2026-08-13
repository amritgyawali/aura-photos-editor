# AURA-SEC-9002 - A sealed biometric record could not be opened

**Severity / recovery:** see `crates/aura-core/errors.toml` for the registered values.

## What the photographer sees

The registered sentence, per affected face, in the Problems panel. The face's box still appears in the People panel; it is simply not part of any identity.

## What actually happened

One sealed record failed a check. The envelope in `aura_people::vault` verifies five things, and the error message says which one failed:

| Check | Message | What it usually means |
|---|---|---|
| Length | "shorter than the ... byte minimum envelope" | a truncated write, or a `NULL` that became an empty blob |
| Magic | "this is not an AURA sealed record" | something wrote a plaintext template into `faces.embed` |
| Version | "envelope version N was written by a different build" | a catalog from a newer build, or a downgrade |
| Kind | "this record is kind N, and a template was asked for" | a centroid blob in a template column, or vice versa |
| Tag | "the authentication tag does not match" | the bytes were altered, sealed for a different record, or sealed with a different key |
| Synthetic nonce | "does not match the recovered contents" | a forgery that defeated the MAC key but not the nonce key |

The tag failure is the interesting one, and it is deliberately ambiguous between three causes. The tag covers the header, **the record's own identifier** and the ciphertext, so moving one face's sealed template onto another face's row fails the same way a bit-flip does. That is intended: an attacker who can rewrite rows should not be able to swap two people's templates, and a support engineer does not need to distinguish the cases before reaching for the same fix.

## What AURA does automatically

Quarantines the one record, logs it with its id, and continues. The row is returned with `template: None`, the face does not vote on identity, and the other nine thousand faces group normally.

Quarantine rather than retry: the bytes will not change, so retrying is a way of doing nothing twice.

## Operator steps

1. Count them. **One** is a disk event or a half-restored backup. **All of them** is a key problem, and the actual code is `AURA-SEC-9001` - check for that first and stop here if it is present.
2. Re-running the face pass replaces the affected rows: face ids are derived from the photograph and the box, so a re-scan rewrites the same row rather than adding a duplicate.
3. Run `PRAGMA integrity_check` on the catalog, or open it and let the migration runner do it. A sealed blob that was truncated by a disk error rarely arrives alone.
4. If the message says "envelope version N was written by a different build", the catalog is from a newer AURA. Do not attempt to read it - `Vault::open` refuses rather than guessing, which is why nothing was silently misinterpreted.
5. If the message is "this is not an AURA sealed record", something bypassed the store. That is a bug worth reporting with the diagnostics bundle, because no supported path can write that column directly.

## Related

- Error registry: `crates/aura-core/errors.toml`
- The envelope format: `crates/aura-people/src/vault.rs`
- Key unavailable: `docs/runbooks/AURA-SEC-9001.md`
- Design record: `docs/adr/ADR-0013-people-intelligence-and-the-biometric-store.md`
