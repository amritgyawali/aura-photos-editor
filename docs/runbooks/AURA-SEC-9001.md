# AURA-SEC-9001 - The project's biometric key is not available

**Severity / recovery:** see `crates/aura-core/errors.toml` for the registered values.

## What the photographer sees

The registered sentence, and a People panel that will not open. Everything else works: the catalog opens, previews build, culling and editing run, and the wedding exports.

## What actually happened

Every face template and every aligned crop in this product is sealed with a key derived from a per-project secret held in the operating system's credential store - Windows DPAPI, the macOS keychain, or the Secret Service on Linux. `aura_people::vault::account_for` names the account; `face_vault.key_account` records which name was used.

The secret could not be read. Four causes, in descending order of how often they happen:

1. **The catalog was copied to another computer.** This is the design working: a catalog without its keychain entry has no biometric data in it. That is the whole point of sealing it.
2. **The user account changed**, or the machine was reinstalled and the profile restored from a backup that did not include the credential store.
3. **The credential store refused.** A locked keychain on macOS, a Linux session with no Secret Service running, a DPAPI failure after a domain profile change.
4. **The key was deliberately deleted** by an earlier "erase biometric data", and the erasure did not finish deleting the rows. That case reports `AURA-SEC-9003` as well; fix that one first.

## What AURA does automatically

Degrades. The People panel is unavailable, `SubjectHierarchy::coverage` is not reported, and every later phase reads that absence and falls back to its non-people path. Nothing is deleted, and nothing is silently regenerated - regenerating a key would produce a store that cannot read its own rows, which is worse than an honest refusal.

The recovery is `ask_user`, and the question is real: starting a fresh face index discards the identities the photographer may have spent an hour naming.

## Operator steps

1. If the catalog moved machines, this is expected. Offer the photographer the choice: bring the credential-store entry across, or start a fresh face index for this project. There is no third option, by design.
2. On macOS, unlock the login keychain and retry. On Linux, confirm a Secret Service provider is running (`secret-tool` should succeed). On Windows, confirm the user profile is the one that created the project.
3. To start fresh: erase the biometric data for the project, which clears the sealed rows and the receipt, then re-run the face pass. Culling and edit decisions are untouched by erasure - that is section 6.5's requirement and it is enforced by `PeopleStore::erase` touching no table outside migration 6.
4. **Never** hand-edit `face_vault`. The salt there is derived from the project id and is reproducible; the *secret* is not in the catalog and cannot be recovered from it.
5. Do not treat this as a corruption event. The sealed rows are intact and unreadable, which is exactly the state they are supposed to be in without the key.

## Related

- Error registry: `crates/aura-core/errors.toml`
- The envelope and the key derivation: `crates/aura-people/src/vault.rs`
- Erasure: `docs/runbooks/AURA-SEC-9003.md`
- Design record: `docs/adr/ADR-0013-people-intelligence-and-the-biometric-store.md`
