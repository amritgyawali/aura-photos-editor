# AURA-REL-12001 - A release or model pack did not verify and was not installed

**Severity / recovery:** see `crates/aura-core/errors.toml` for the registered values.

## What the photographer sees

An update did not pass its signature check, so AURA did not install it. You are still on the version you had this morning.

## What actually happened

A release bundle or a model pack failed its signature or digest check and was not installed.

The check is ed25519 over the manifest, then sha256 per file, then the model card - phase 03's
order, unchanged. A pack that fails any of the three is refused entirely rather than partially
installed.

## What to do

You are on the version you had before. Do not work around this by installing the pack by hand: a
pack that does not verify is either corrupt in transit or is not the pack it claims to be, and only
one of those is harmless. Re-download; if it fails twice, report the version string.

## Where it comes from

PHASE-30. See `docs/adr/ADR-0061-delivery-learning-loop-and-release.md` and
`docs/plan/phases/PHASE-30-DELIVERY-INTEGRATIONS-LEARNING-LOOP.md`.
