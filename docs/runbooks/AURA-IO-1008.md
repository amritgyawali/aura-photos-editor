# AURA-IO-1008 - Catalog location is inside a cloud-synced folder

**Severity / recovery:** see `crates/aura-core/errors.toml` for the registered values.

## What the photographer sees

The one-sentence message registered for this code in `errors.toml`.

## What actually happened

The chosen catalog folder is managed by Dropbox, iCloud Drive, OneDrive, Google Drive or a similar client. Sync engines copy SQLite WAL files mid-write and corrupt catalogs.

## What AURA does automatically

AURA refuses to create or open the catalog. This is a hard refusal, not a warning.

## Operator steps

1. Choose a folder on a local, non-synced drive; the pictures themselves may stay wherever they are.
2. If the catalog already exists in a synced folder, quit every AURA window, wait for the sync client to finish, then move the whole .aura folder to a local drive and open it from there.
3. Back up catalogs with a versioned backup tool instead of a sync client.

## Related

- Error registry: `crates/aura-core/errors.toml`
- Constructors: `crates/aura-core/src/errors/`
