# ADR-0062 - The delivery, learning and diagnostics IPC surface

**Status:** accepted
**Date:** PHASE-30
**Deciders:** CTO, SFE, TLC, MLOPS, SEC, PM

## Context

Phase 30 adds three panels that no earlier phase has an equivalent of: an export dialog whose
button writes files, a delivery screen that talks to a network, and a learning review that changes
what the product will do next time. Plus a diagnostics screen, which is the first screen in the
product whose subject is the product.

Every earlier surface answers "what did AURA decide about this photograph". These four answer "what
is AURA about to do to my week".

## Decision 1 - Sixteen commands, and the split is by what they cost

Seven read, five act, four decide.

| Command | Kind | What |
|---|---|---|
| `export_status` | read | The outline: three denominators, verified share, whether a manifest is sealed |
| `export_preview_names` | read | Every name a job would produce, **without writing anything** |
| `export_files` | read | What the last job wrote, per file, with its reasons |
| `export_manifest` | read | The last sealed manifest |
| `export_run` | act | Render, write, read back, hash, seal |
| `delivery_status` | read | Backups and uploads |
| `delivery_providers` | read | Which providers are configured, and which have a credential |
| `delivery_backup` | act | Copy a sealed delivery to a destination, verifying every file |
| `delivery_upload` | act | Start or resume an upload |
| `delivery_items` | read | Per-file upload state |
| `learn_status` | read | What the loop has seen, and the attribution rate |
| `learn_buckets` | read | Every bucket, aggregated, with what was dropped |
| `learn_compare` | read | The A/B, both sides on the same held-out corrections |
| `learn_adopt` | decide | **The only way a profile moves forward** |
| `learn_roll_back` | decide | The only way it moves back |
| `learn_set_consent` | decide | What a project agrees to |
| `diagnostics_report` | read | Versions, flags, capabilities, the last three errors |

That is seventeen; `export_preview_names` is the one worth arguing about and section 3 does.

## Decision 2 - `export_preview_names` exists, and it is not a debug command

Section 10.1 asks for collision-free names across 4,000 files including duplicate original names
from two cameras. A photographer should be able to see that answer *before* the wedding is written,
not by reading a manifest afterwards.

It is a dry run over the whole job: it substitutes every token, resolves every collision, and
returns the list. It writes nothing and it renders nothing, so it costs a catalog read.

Without it the export dialog's naming field is a text box whose consequences are invisible until
1,400 files have been written under the wrong scheme.

## Decision 3 - There is no `export_cancel_and_keep`

Cancelling an export leaves the files that were written and does **not** seal a manifest.
`DeliveryCode::Cancelled` is on the run and the panel says how many files are on disk.

The alternative - seal a partial manifest so the photographer can at least send what is there - was
rejected. A manifest is the document that says "this is the delivery", and a partial one is a
document that says a wedding was delivered when four chapters of it were not. The recovery is to
re-run the job: files that already verified are re-written rather than re-rendered, which is the
expensive half.

## Decision 4 - `learn_adopt` takes a profile and no update

The candidate is computed by the service and stored; `adopt` names the profile and takes whatever
candidate is current.

An `adopt(update)` signature would let a caller adopt an update that was computed against a
different baseline - a photographer who left the panel open over a weekend of corrections and then
clicked adopt would adopt a fit measured against a profile that had moved. The service re-checks
`is_offerable` at adoption time, and an adoption of a stale candidate is refused rather than
silently recomputed.

## Decision 5 - There is no bulk anything on this surface

No `export_all_projects`, no `delivery_upload_all`, no `learn_adopt_all`.

Phase 27 wrote the rule from the other side: agreeing that forty findings are real is a statement
about the findings, and instructing AURA to act on forty frames unattended is a statement about the
remedies. Here every action already *is* an action on a whole wedding, so a bulk version would be an
action on somebody's whole year - and adopting a profile update across eleven weddings is not a
thing anybody should be able to do with one click.

## Decision 6 - What is deliberately absent

* **No `export_set_quality` or any other threshold write.** The export dialog builds an `ExportJob`
  and sends it whole. A surface with per-field setters is a surface where a job can be half
  configured, and `ExportJob::validate` would then run against something nobody assembled.
* **No credential on the wire, ever.** `delivery_providers` returns `(ProviderId, bool)` - the name
  and whether a credential exists. The secret is set by a command that takes it on stdin, which is
  phase 04's mechanism, unchanged.
* **No `learn_capture` command.** Corrections are captured by the panels that already own the
  override - the develop panel, the cull panel, the curation panel - through `LearnService::capture`
  inside the command that records the override. A capture command on the wire would be a second
  route into the correction table with no ledger decision behind it, which is exactly what
  `AURA-LRN-11004` refuses.
* **No telemetry command.** Telemetry is written locally by the code that does the work and is read
  by the diagnostics screen. A command that *sends* it would be the only outbound call in a phase
  whose section 7 says there are none.
* **No `flags_set`.** Kill switches are read from `ops/flags/flags.toml` at start-up. A running
  application that could switch its own AI stages on is an application whose diagnostics do not
  describe the run they came from.

## Decision 7 - The diagnostics screen is read-only and lists what is *not* working

`diagnostics_report` returns the app version, the render backend and its degradation, the model set
digest, which flags are off, which providers are configured, and the last three errors with their
codes.

It leads with the caveats rather than the capabilities, for the same reason phase 27's QC report
leads with what was checked: a support call starts with somebody reading this screen down a
telephone, and the useful half is the half that says what this machine cannot do.
