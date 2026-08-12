# AURA-DB-3006 - Catalog statement failed unexpectedly

**Severity / recovery:** see `crates/aura-core/errors.toml` for the registered values.

## What the photographer sees

The one-sentence message registered for this code in `errors.toml`.

## What actually happened

A prepared statement failed for a reason that is a defect rather than a user situation: a constraint violation or a type mismatch.

## What AURA does automatically

The run halts safely. Everything committed before the failing batch is intact.

## Operator steps

1. Create a support bundle; it contains the statement name, the error code and the batch size, with no file paths or image content.
2. Re-open the project; committed rows are still there and the ingest journal allows a clean resume.

## Related

- Error registry: `crates/aura-core/errors.toml`
- Constructors: `crates/aura-core/src/errors/`
