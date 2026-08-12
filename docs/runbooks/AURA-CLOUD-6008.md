# AURA-CLOUD-6008 - No consent for this class of data to leave the machine

**Severity / recovery:** see `crates/aura-core/errors.toml` for the registered values.

## What the photographer sees

The registered sentence, and a per-project consent dialogue they can open from Settings.

## What actually happened

`ProjectConsent` denies the data class the task needed. Consent defaults to all-false when a project is created, and there is no code path that grants it other than an explicit photographer action - that is a frozen contract from phase 01.

## What AURA does automatically

Refuses before the payload builder runs. `cloud_metadata` gates EXIF summaries and counts; `cloud_derived_imagery` gates thumbnails and crops. `cloud_full_imagery` is never requested by any task in the product and the gateway refuses it unconditionally.

## Operator steps

1. This is working as designed. Do not add a "grant all" shortcut.
2. Walk the user through Settings > Project > Cloud AI, where the two grants are separate checkboxes with plain-language descriptions of what leaves the machine.
3. A grant can carry an expiry. A grant that expired mid-wedding presents as this code.

## Related

- Frozen consent contract: `crates/aura-core/src/contract/consent.rs`
- Privacy: `docs/plan/24-PRIVACY-CONSENT-AND-LEGAL.md`
