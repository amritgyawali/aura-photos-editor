# AURA-IO-1002 - Permission denied reading the source folder

**Severity / recovery:** see `crates/aura-core/errors.toml` for the registered values.

## What the photographer sees

The one-sentence message registered for this code in `errors.toml`.

## What actually happened

The operating system refused the read. On macOS this is usually Full Disk Access or a Removable Volumes prompt that was dismissed; on Windows it is usually a network share credential or a folder owned by another user.

## What AURA does automatically

The run stops before any row is written.

## Operator steps

1. macOS: System Settings, Privacy and Security, Files and Folders, enable the removable-volume and folder permissions for AURA, then restart AURA.
2. Windows: open the folder in Explorer as the same user; if it is a network share, re-enter the credentials with Reconnect at sign-in enabled.
3. Re-run the import. No rows were written, so this is a clean retry.

## Related

- Error registry: `crates/aura-core/errors.toml`
- Constructors: `crates/aura-core/src/errors/`
