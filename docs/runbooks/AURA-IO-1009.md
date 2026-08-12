# AURA-IO-1009 - Preview cache entry could not be written or read back

**Severity / recovery:** see `crates/aura-core/errors.toml` for the registered values.

## What the photographer sees

The one-sentence message registered for this code in `errors.toml`. Thumbnails still appear; they are rebuilt each time instead of being read from disk.

## What actually happened

The content-addressed preview cache could not complete an operation: the cache folder is read-only, the disk is full, or an entry read back with a digest that does not match its file name, which means the bytes on disk changed underneath us.

A cache is not truth. Every entry is verified on read, and a failed verification deletes the entry rather than serving corrupt pixels.

## What AURA does automatically

- On a write failure: the preview is still returned to the caller from memory; only the on-disk copy is skipped.
- On a verification failure: the entry is deleted and rebuilt on the next request (self-healing).
- `cache.stats` keeps reporting, so a cache that never gains bytes is visible in settings.

## Operator steps

1. Check free space on the cache drive. The default budget is 40 GB and AURA evicts to stay inside it, but it cannot evict below the space other software needs.
2. Check that the cache folder is writable and is not inside a synced folder such as OneDrive or Dropbox. Sync clients rewrite files under us and cause exactly this error.
3. Move the cache with the setting in the Cache panel, or purge it with one click; nothing is lost, because every entry can be rebuilt from the original file.

## Related

- Error registry: `crates/aura-core/errors.toml`
- Cache: `crates/aura-cache/src/store.rs`
- Preview troubleshooting: `docs/runbooks/previews.md`
