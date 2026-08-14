# AURA-SEC-9003 - Biometric erasure could not complete

**Severity / recovery:** see `crates/aura-core/errors.toml` for the registered values.

## Read this first

This is the one failure in the people domain that is **not** soft, and the reason is not technical. A photographer who pressed "erase biometric data" and was told it worked has made a promise to somebody who is not in the room. A half-completed erasure that reported success would break that promise silently, so this code is run-blocking and the message says plainly that some data is still on the computer.

## What the photographer sees

The registered sentence: AURA could not finish deleting this wedding's face data, some of it is still here, nothing else was changed, try again.

## What actually happened

`PeopleStore::erase` does four things in a fixed order, and the order is the design:

1. **Delete the credential-store entry.** First, because without the key everything else is noise - so a crash after step 1 leaves unreadable data rather than readable data. Doing this last would leave a window in which a crash left readable templates and a photographer who believed they were gone.
2. **Delete the sealed crop files** under the project's cache directory.
3. **Delete the catalog rows** - `faces`, `person_boxes`, `identities`, `cooccurrence`, `identity_links`, `face_scan` - in one transaction, and stamp `face_vault.erased_at`.
4. **Verify.** Re-read `v_people_coverage` and confirm that nothing survived. An erasure that reports success without checking is a promise nobody verified.

This code is raised when step 1, 2 or 4 failed. The message says which, and the context carries how many records remain.

## What AURA does automatically

Stops, and reports. It does not retry automatically, because a retry that also half-fails produces two partial erasures and no clear state. Culling decisions, edits and exports are untouched throughout - `erase` touches no table outside migration 6, which is section 6.5's requirement.

## Operator steps

1. **Retry once.** A locked keychain, a file held open by a backup agent or an antivirus scanner, and a transient disk error all clear on a second attempt.
2. If step 1 failed - "the credential store refused to delete the key" - fix the credential store first. On macOS unlock the login keychain; on Linux confirm a Secret Service provider is running. Until the key is gone, the data is still readable.
3. If step 2 failed - "the sealed crop directory could not be read" - find what is holding the directory. On Windows this is usually a sync client or an indexer. The crops are at `<cache root>/<project id>/faces/`.
4. If step 4 failed - "rows survived the delete" - the catalog is the problem, not the crypto. Run `PRAGMA integrity_check`, and check that the disk is not full: a `DELETE` needs journal space.
5. **Manual removal, in this order**, only if the retries fail and the photographer needs the promise kept today:
   - delete the credential-store entry named `aura.people.v1.<project id>`;
   - delete `<cache root>/<project id>/faces/`;
   - in the catalog, `DELETE FROM faces WHERE project_id = ?`, then `person_boxes`, `identities`, `cooccurrence`, `identity_links`, `face_scan`, then `UPDATE face_vault SET erased_at = ...`.
   Do not touch any other table. Everything else in the catalog is the photographer's work.
6. Confirm with `SELECT * FROM v_people_coverage WHERE project_id = ?`: `scanned` and `faces` must both be zero.

## Related

- Error registry: `crates/aura-core/errors.toml`
- Erasure implementation: `crates/aura-people/src/store.rs`
- Key unavailable: `docs/runbooks/AURA-SEC-9001.md`
- Design record and the privacy note: `docs/adr/ADR-0013-people-intelligence-and-the-biometric-store.md`
