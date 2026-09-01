# ADR-0056 - The QC IPC surface: nine commands, a queue built for speed, and no button that acts in bulk

**Status:** accepted · **Date:** 2026-08-31 · **Phase:** 27 · **Supersedes:** nothing

The second of phase 27's two ADRs. [ADR-0055](ADR-0055-quality-control-tickets-and-the-re-edit-loop.md)
covers the engine; this covers the wire.

## 1. Context

Section 2.1 asks for two views and section 9 splits them between two engineers: SFE builds "QC report
view, ticket queue with keyboard flow, before/after, category filters" and MFE builds "escalation
review flow, bulk accept/reject, replacement comparison".

The commercial claim behind them is a number: **a photographer can clear forty tickets in minutes.**
That is what makes an autonomous QC agent worth having rather than a second inbox, and it is the
constraint every decision below answers to.

Two things make this surface different from the twenty-six before it.

**It is the first surface whose primary object is a problem.** Every earlier panel shows what AURA
decided about a photograph. This one shows what AURA thinks is *wrong* with what it decided, which
means the reader arrives sceptical and the panel has to earn each finding.

**It is the first surface where the user's answer is a judgement rather than a value.** A
photographer using the tone panel sets a temperature; a photographer using this one says "yes, that
is a problem" or "no, it is not". `TicketStatus::Accepted` and `TicketStatus::Dismissed` are the
whole of what they can write, and the second is a signal phase 30 learns from.

## 2. Decision: nine commands

Five read, one runs the pass, and three record what a photographer decided.

| Command | What it answers |
|---|---|
| `qc_status` | The panel header: how much was checked, how much could not be, how many findings stand |
| `qc_report` | The per-category table and the swaps, as data |
| `qc_report_markdown` | The same report as text a studio archives |
| `qc_queue` | The escalation queue, worst first, optionally one category |
| `qc_tickets` | Every finding on one photograph, for the frame view |
| `qc_rounds` | What was tried on one ticket and what happened |
| `qc_run` | Inspect the gallery; remediate when the caller asks and the bands allow |
| `qc_decide` | One photographer verdict |
| `qc_decide_bulk` | Many verdicts, and **no remedies** - see section 5 |

Deliberately absent: no `qc_apply`, no `qc_set_threshold`, no `qc_force_replace`, and nothing that
takes a strength. The thresholds are a file a studio edits and the loader refuses a widened one; a
surface that could raise a bound would make `docs/how-qc-works.md` a description of the defaults.

## 3. Decision: the queue is ordered by severity and grouped by category, and both are on the wire

`QcQueueDto` carries the tickets already ordered and `QcGroupDto` carries them bucketed. A panel
that ordered them itself would be a second answer to "which of these is worst", and the two would
diverge the first time somebody changed a weight.

The ordering is `QcTicket::queue_order`: severity as a **ratio** of the finding to its own threshold,
then root-cause rank, then the id. The ratio matters because the ten checks are measured in five
units - dE00, EV, a normalised ratio, a frame fraction and a plain count - and a queue ordered by
raw deviation would put every colour finding above every exposure one for no reason but the scale.

Grouping is what makes the minutes claim true. Judging whether AURA is right about skin drift takes
a minute the first time and five seconds the twentieth, because it is the same question; a queue
interleaved by category makes every ticket the first one.

Groups are ordered by their **worst member** rather than by their size. One unsafe crop outranks
forty marginal colour drifts, because a photographer with twenty minutes needs the first thing they
open to be the worst thing there is.

## 4. Decision: the diagnosis is rendered server-side and sent as text

`QcTicketDto.diagnosis` is a sentence, and it is built by `QcTicket::render_diagnosis` from the
stored code, deviation, threshold and evidence rather than read from a column - ADR-0055 section 3.

It is rendered in Rust rather than in the panel for the reason phase 24 rendered its own disclosure
sentences there: the report a studio archives and the queue a photographer reads must say the same
thing, and two renderers is two wordings. `qc_report_markdown` and `qc_queue` go through the same
function.

The numbers travel beside it anyway - `deviation`, `threshold`, `unit`, `severity` - because a panel
that had to parse the sentence to draw a bar would be a panel that breaks when the copy changes.

## 5. Decision: bulk actions record verdicts and never authorise remedies

`qc_decide_bulk` sets `Accepted` or `Dismissed` on many tickets. Its `applyRemedy` is not optional
and not settable: `queue::bulk` writes `false` on every override it produces.

This is the single easiest way to damage a gallery in this product, and the reasoning is worth
stating because the button is obvious and the objection is not. A photographer agreeing that forty
findings are real is a statement about *the findings*. Instructing AURA to act on forty frames
unattended is a statement about *the remedies*, and the two are different judgements a person makes
with different amounts of attention - one by scanning a list, the other by looking at a photograph.

Per-ticket authorisation lives in the frame view, where the before and after are side by side.

## 6. Decision: what could not be checked is on the wire, at the top

`QcStatusDto` carries `inspectionsSkipped` and `imagesUnreached` beside `checksRun`, and
`completeness` is derived on the Rust side so every reader agrees about it.

A QC panel is the one place in this product where an empty result is genuinely ambiguous. Zero
findings means either "AURA looked at everything and it is fine" or "AURA could not look", and the
second is the common case in this build - phase 06's detector finds no faces, phase 18's segmenter
is untrained, so the skin, mask and crop checks skip on most frames.

A panel that had to infer that would eventually render a wedding nobody could check as a clean bill
of health. `detectorTrained` is on the wire for the same reason phase 24 put its own there and phase
25 `skinFieldAvailable`.

## 7. Decision: a swap is shown as two photographs, never as a number

`QcReplacementDto` carries both image handles, both metrics and the sentence. Section 6.4 asks for
"shown side by side in the report", and the panel does that with the two previews the preview
service already serves.

It carries `metricBefore` and `metricAfter` rather than their difference, because a photographer
looking at a swap wants to know what each frame measured. A stored subtraction cannot be read back
as two numbers, and "0.5 better" answers a question nobody asked.

## 8. What is not here, and cannot be added without an ADR

No pixels. `Evidence` has no variant that could hold image bytes - phase 13's rule, inherited
unchanged - so the panel fetches crops through `get_preview` with the rectangle the ticket names.

No thresholds. There is no command that reads or writes `qc_thresholds.toml`, because a surface that
could show them would be one step from a surface that could set them.

No remedy construction. A panel cannot build a `Remedy`; it can only authorise the one the ticket
already carries, which `remedy::validate` produced. The planner's own steps are not on the wire at
all - a proposal that never survived validation is not something a photographer should be shown as
though it were an option.

No identity in the planner's input, which is not a wire decision but is worth recording here because
it is the surface a future feature would be added to: `QcPlanInput` has no field a name or a role
could go in.

## 9. Consequences

A photographer can open the QC panel, read one number that says how much was actually checked, work
a queue that is grouped by question and ordered by severity, clear a category in a few keystrokes,
and export a report their studio keeps. What they cannot do from this surface is make AURA less
careful, act on forty frames at once, or see a proposal the policy refused.

The cost is that the panel is read-mostly and the interesting write - authorising a remedy - is
deliberately slower than it could be. That is the trade this phase exists to make.
