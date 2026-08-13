# AURA-SEC-9004 - A people query crossed a project boundary and was refused

**Severity / recovery:** see `crates/aura-core/errors.toml` for the registered values.

## Read this first

**This code is designed to be unreachable.** Section 2.2 of the phase 06 document forbids cross-wedding identity persistence outright, and section 6.5 requires it to be impossible rather than merely unlikely. Every people query is scoped by `project_id`, the schema carries the column on `faces` even though `image_id` already implies it, and `crates/aura-people/tests/privacy.rs` asserts there is no path from one project's identity to another's.

So if this fires in the field, something is wrong that continuing would make worse. It halts.

## What the photographer sees

The registered sentence, and a request to report it with the diagnostics bundle. The operation that triggered it did nothing.

## What actually happened

`PeopleStore::assert_same_project` was given two or more identity ids and at least one of them belongs to a different project. The context carries `expected_project` and `found_project`.

The only supported paths that reach it are a merge and a split, and both take ids the UI obtained from a single project's identity list. So the realistic causes are:

1. **A UI state bug**: a stale identity list from a project that was closed, and a merge dialog acting on it after the photographer switched projects.
2. **A scripted or automated caller** assembling ids from two sources.
3. **A hand-edited catalog** in which an `identities` row's `project_id` disagrees with the faces pointing at it.

## What AURA does automatically

Refuses, halts the operation, and writes nothing. No faces move, no identity is created or deleted, and no journal entry is recorded - the refusal happens before any of that.

## Operator steps

1. **Capture the diagnostics bundle before doing anything else.** The two project ids in the context are the whole diagnosis and they are not recoverable afterwards.
2. Check whether the photographer switched projects with the People panel open. That reproduces cause 1 and is a UI fix, not a data problem.
3. Verify the catalog: for each identity in the refused pair, confirm that `identities.project_id` matches the `project_id` on every face pointing at it. A disagreement is cause 3 and needs the identity rebuilt by a regroup rather than repaired by hand.
4. Do **not** work around it by merging within each project separately and calling it done. If the two identities really are the same person at two weddings, the correct answer is that this product does not represent that - by policy, permanently, and for a good reason.
5. Report it. An unreachable code that fired is worth a bug even if the immediate cause turns out to be benign.

## Related

- Error registry: `crates/aura-core/errors.toml`
- The scoping test: `crates/aura-people/tests/privacy.rs`
- Design record: `docs/adr/ADR-0013-people-intelligence-and-the-biometric-store.md`
- Phase document, sections 2.2 and 6.5: `docs/plan/phases/PHASE-06-PEOPLE-INTELLIGENCE.md`
