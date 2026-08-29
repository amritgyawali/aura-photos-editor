# AURA-ML-5104 - A change to which small fixes are permitted was refused

**Severity / recovery:** see `crates/aura-core/errors.toml` for the registered values.

## What the photographer sees

A setting in the Micro-Retouch panel did not take. Nothing was changed.

## What actually happened

`MicroService::set_matrix` refused the override. There is one cause in the shipped code: the
override set nothing at all - every field was absent - which is refused rather than treated as a
no-op, because a caller that meant to send a change and sent an empty one has a bug that would
otherwise be silent.

`MicroService::accept` emits the same code when the photograph has no plan.

## What to do

1. Check what the panel sent. An override with no operations, no clothing switches and no
   borrowing flag is the refusal above.
2. Remember what this surface deliberately cannot do: **there is no strength field and no ceiling
   field.** A photographer chooses *which* operations run; how far each may go is bounded by the
   contract, and a surface that could raise a ceiling would make `docs/retouch-ethics.md` a
   description of the defaults rather than a promise. A request to "turn the teeth up" is not a
   bug in this code path.
