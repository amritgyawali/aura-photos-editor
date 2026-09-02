# AURA-DLV-10002 - The backup destination or the gallery provider could not be reached

**Severity / recovery:** see `crates/aura-core/errors.toml` for the registered values.

## What the photographer sees

AURA could not reach that place. Everything already sent is remembered, so when it comes back the upload carries on from where it stopped.

## What actually happened

The backup destination or the provider could not be reached.

`retry`, and the retry is cheap: every file's upload state is stored, so a resumed job re-sends the
tail of one file rather than the head of a wedding.

## What to do

Reconnect and press upload again. Nothing is lost and nothing is re-sent that the far end already
acknowledged. If a NAS disappears repeatedly mid-job, the resume machinery will hide it - check
`delivery_upload.resumes`, which is on the panel.

## Where it comes from

PHASE-30. See `docs/adr/ADR-0061-delivery-learning-loop-and-release.md` and
`docs/plan/phases/PHASE-30-DELIVERY-INTEGRATIONS-LEARNING-LOOP.md`.
