# AURA-ML-5002 - Model manifest signature did not verify

**Severity / recovery:** see `crates/aura-core/errors.toml` for the registered values.

## What the photographer sees

The registered sentence, and AI features unavailable until it is resolved. This is a security refusal, so it is deliberately not silent and deliberately not automatic.

## What actually happened

`models/manifest.sig` is an ed25519 signature over the exact bytes of `models/models.lock`, made offline with a release key that never enters the repository or CI. Verification failed, which means one of: the lock file was edited after signing, the signature file belongs to a different lock file, or the file was tampered with in transit.

## What AURA does automatically

Nothing is loaded. Verification order is signature, then digest, then operator support, then load, and the first failure stops the chain - `model.rejected` is emitted with the reason. Models already resident and verified in a previous session are not affected until the next verification pass, so a running job finishes.

## Operator steps

1. Treat this as a security event until proven otherwise. Do not "fix" it by re-signing on the customer's machine.
2. Re-install the model pack from Settings, which fetches both files together.
3. If it fails again on a second machine with the same pack, the published pack is wrong: escalate to SEC and MLOPS, and pull the pack from distribution.
4. Verify the release public key fingerprint in the build matches the one used to sign the pack - a key rotation without a co-ordinated release presents exactly this way.

## Related

- Error registry: `crates/aura-core/errors.toml`
- Runtime ADR: `docs/adr/ADR-0007-inference-runtime.md`
- Model update failures: `docs/runbooks/model-update-failed.md`
