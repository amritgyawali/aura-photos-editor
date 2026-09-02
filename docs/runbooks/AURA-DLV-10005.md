# AURA-DLV-10005 - The gallery provider reported a different checksum from the one sent

**Severity / recovery:** see `crates/aura-core/errors.toml` for the registered values.

## What the photographer sees

The gallery received something different from what AURA sent, so that photograph will be sent again.

## What actually happened

The provider accepted a file and reported a digest different from the one that was sent.

The file is marked `UploadState::Corrupt` rather than `Failed`, because those are different
situations: a failure is a file that did not arrive, and this is a file that arrived wrong. Only the
second is worth re-sending immediately.

## What to do

Nothing to do - the next upload pass re-sends it. If the same file is reported corrupt three times,
the local file is the suspect and re-exporting it is the fix.

## Where it comes from

PHASE-30. See `docs/adr/ADR-0061-delivery-learning-loop-and-release.md` and
`docs/plan/phases/PHASE-30-DELIVERY-INTEGRATIONS-LEARNING-LOOP.md`.
