# AURA-RENDER-8022 - A written file did not read back the same and the delivery was stopped

**Severity / recovery:** see `crates/aura-core/errors.toml` for the registered values.

## What the photographer sees

One file AURA wrote came back different from what it sent, so the whole delivery has been stopped. Check the drive before sending any of this to a client.

## What actually happened

A file was written, read back, hashed, and the digest did not match the bytes that were sent.

**This stops the whole delivery**, which is the one place in the product where one item's failure
halts a run that could have carried on. The argument is in ADR-0061 section 3 and it is short: a
gallery missing one photograph is a phone call, and a gallery containing one broken photograph is a
photograph nobody notices until the couple opens it.

What causes it, in the order to check:

* a failing SD card, external drive or SSD - by far the most common;
* a NAS or network share that acknowledges a write and drops it;
* a full volume whose filesystem reported success on a short write;
* memory that is corrupting a buffer, which will usually show on more than one file.

## What to do

Do not send any of this delivery to a client. Run the destination drive's own health check, then
export again to a different volume. If the second volume also fails, the fault is upstream of the
disk - test memory before trusting anything this machine writes.

`export_file` rows for the files that *did* verify are kept, so a re-run after the drive is
replaced re-writes rather than re-renders.

## Where it comes from

PHASE-30. See `docs/adr/ADR-0061-delivery-learning-loop-and-release.md` and
`docs/plan/phases/PHASE-30-DELIVERY-INTEGRATIONS-LEARNING-LOOP.md`.
