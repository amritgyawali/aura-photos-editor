# AURA-ML-5003 - Model file digest did not match the manifest

**Severity / recovery:** see `crates/aura-core/errors.toml` for the registered values.

## What the photographer sees

The registered sentence. If a previous version of the model is installed, work continues on it and nothing else changes.

## What actually happened

The manifest signature verified, so the expected sha256 is trustworthy, and the file on disk does not match it. The usual causes are a truncated copy, a failing disk, or antivirus quarantining part of a file. It is not a signing problem: a wrong signature would have failed earlier with AURA-ML-5002.

## What AURA does automatically

The file is not loaded and not deleted - it is left in place for diagnosis and the previous verified version stays active. `model.rejected` is emitted with `reason = "digest"`. If no previous version exists, the feature that needed the model is unavailable and says so.

## Operator steps

1. Re-install the model pack from Settings. A single corrupted transfer is fixed by this.
2. If the same file corrupts twice, check the disk and check whether security software is modifying files in the model directory; add an exclusion for it.
3. Attach the digest pair from the log line to any report. The expected value comes from a signed manifest, so a mismatch is always a local-file question.

## Related

- Error registry: `crates/aura-core/errors.toml`
- Model update failures: `docs/runbooks/model-update-failed.md`
