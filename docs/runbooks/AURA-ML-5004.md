# AURA-ML-5004 - Model transfer was incomplete and will be resumed

**Severity / recovery:** see `crates/aura-core/errors.toml` for the registered values.

## What the photographer sees

The registered sentence and a progress bar that continues from where it stopped rather than from zero.

## What actually happened

The transfer of a model file ended before the expected number of bytes arrived. Transfers are written to a `.part` file next to the destination and are byte-range resumable, so an interruption costs the current chunk and nothing else. The destination file is only replaced atomically after the whole payload has been verified.

## What AURA does automatically

The partial file is kept, the transfer resumes from its length, and the digest is verified over the completed file before the atomic swap. A partial file that fails verification is deleted and fetched again in full. The previously installed version keeps working throughout.

## Operator steps

1. Nothing, if it resumes and completes: this is the mechanism working.
2. If it never completes, check free disk space first - a full disk presents as an endless resume.
3. If the source is an offline bundle on a share, confirm the share supports range reads; a source that cannot resume forces a full re-fetch every time and should be reported.

## Related

- Error registry: `crates/aura-core/errors.toml`
- Model update failures: `docs/runbooks/model-update-failed.md`
