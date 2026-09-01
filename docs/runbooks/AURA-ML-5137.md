# AURA-ML-5137 - A photographer's quality-control verdict could not be recorded, or automation tried to overwrite one

**Severity / recovery:** see `crates/aura-core/errors.toml` for the registered values.

## Two different situations share this code

**A write that failed.** `QcService::decide` could not store what you decided. Nothing about the
photograph changed. Reopen the review queue and try again.

**A write that was refused.** Migration 27's `qc_ticket_keep_user_status` trigger aborted a
statement that would have moved a ticket out of `accepted` or `dismissed`. That is the product
working, and it is the more interesting of the two.

## Why the second one exists

`accepted` and `dismissed` are the two statuses a person owns. Everywhere else in this schema the
protected thing is a *value* a photographer set - a white balance, a crop, a strength. Here it is a
**judgement**: whether a finding was right.

That is the one case where automation is most tempted to be right anyway. A re-run of the pass will
find the same deviation, compute the same threshold and want to file the same ticket, and a build
that let it do so would show a photographer a finding they had already rejected - every week, for as
long as the project exists.

`QcStore::sweep` reads the user-set statuses out before it clears a project and puts them back
afterwards, which is phase 25's mechanism. The trigger is the second layer, because the DELETE guard
alone was not enough in migration 18 and there is no reason to think it would be here.

## Fixing it

If a legitimate caller is hitting the trigger, it is a bug in `QcStore`: the sweep is not preserving
statuses across a re-analysis. The test that covers it is
`a_dismissed_ticket_survives_a_second_pass` in `crates/aura-qc/src/store.rs`.

A photographer who wants to change their own mind about a ticket can. `QcService::decide` performs
the change in one statement that sets both the status and the note, and the trigger only refuses a
write that moves a user-set status *without* going through it.
