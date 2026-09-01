# AURA-ML-5141 - Stored quality-control findings came from different arithmetic or different thresholds

**Severity / recovery:** see `crates/aura-core/errors.toml` for the registered values.

## What the photographer sees

A note that AURA has improved how it checks finished photographs and is re-checking this wedding in
the background. Anything they accepted or dismissed themselves is kept.

## What actually happened

`qc_ticket.analysis_ver` or `qc_ticket.thresholds_ver` does not match what this build produces.

Two columns, because they invalidate two different things. `analysis_ver` invalidates every
`deviation`: the number was produced by arithmetic this build no longer runs. `thresholds_ver`
invalidates every `threshold` and therefore every severity ordering, without invalidating the
measurements themselves.

This is the latest of the product's version-drift codes, after `AURA-ML-5015`, `5018`, `5022`,
`5028`, `5033`, `5038` and the rest. It exists so a comparison across versions never happens
silently - a stale deviation compared against a current threshold produces a plausible number that
means nothing, and a queue ordered by it looks exactly like a queue nobody has to worry about.

## What survives a re-analysis

`accepted` and `dismissed` tickets. `QcStore::sweep` reads them out before it clears the project and
puts them back afterwards, and `qc_ticket_keep_user_status` refuses the write if anything tries to
move one to a status automation owns. Their `deviation` is re-measured; their status is not.

Everything else is rebuilt: open tickets, rounds, replacements and the run row.

## Fixing it

Nothing to fix. Let the background pass finish. If it does not start, run it from the QC panel or
with `aura-cli`; if it fails, `AURA-ML-5136` will be raised with the underlying cause.
