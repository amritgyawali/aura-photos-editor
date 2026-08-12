# AURA-CLOUD-6009 - The payload builder refused to build an upload

**Severity / recovery:** see `crates/aura-core/errors.toml` for the registered values.

## What the photographer sees

One item set aside in the Problems list with the registered sentence. The rest of the wedding is unaffected.

## What actually happened

The payload builder ran one of its refusals: pixels carrying RAW provenance, an image that could not be reduced to the 768 px limit, a payload over the byte ceiling, a contact sheet asked for with more than twelve tiles, or a face-blur request with no regions supplied while the project requires blur.

## What AURA does automatically

Refuses to build. **Nothing is sent.** The item is quarantined with its reason, so the missing decision is visible rather than silently absent.

## Operator steps

1. Read the `detail` field of the audit row - it names which refusal fired.
2. RAW provenance reaching the builder is a programming error in the calling phase, not a user problem: escalate with the task name.
3. A byte-ceiling refusal on a legitimately large contact sheet means the tile count or the tile size for that task is set too high.

## Related

- Payload builder: `crates/aura-cloud/src/payload.rs`
- Policy ADR: `docs/adr/ADR-0009-cloud-ai-policy.md`
