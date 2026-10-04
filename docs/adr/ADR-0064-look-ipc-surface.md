# ADR-0064 - The look IPC surface

**Status:** accepted
**Date:** PHASE-31
**Deciders:** CTO, TLC, PM
**Supersedes:** nothing. **Amends:** nothing.

## Context

Ten commands, one panel, and one thing the panel has to say that no other panel in this product has
had to say: *the obvious way to use this feature does not work, here is why, and here is what to do
instead.*

## Decision 1 - The unavailable route is offered, disabled, with the reason beside it

`SOURCE_CHOICES` includes `public_url`. It renders as a radio button, it is disabled, and selecting
it shows `FETCH_UNAVAILABLE` - which says AURA cannot download from a page, says to use a folder or
an Instagram data export instead, and says the pasted link is still recorded.

The alternative is to omit the option, which is tidier and worse. A photographer who came to this
feature because they want to paste a link will look for the setting they think they have missed,
find nothing, and conclude the product is broken or that they are. Phase 03 put the hardware
capability on the wire for this reason and phase 30 put `NETWORK_TRANSPORT_AVAILABLE` there;
`LookStatusDto::networkTransportAvailable` is the same shape, and the panel reads it rather than
hard-coding the fact.

## Decision 2 - `parse_reference` is synchronous and cannot fail

It is called on every keystroke so the address box can show the handle it understood. A command
that went to a thread pool to parse a string would be a spinner on the third character of
`instagram.com/`, and a command that returned an error would make the box unusable while somebody
is halfway through typing into it. An address that will not parse comes back with
`understood: false` and the refusal's own sentence.

**It resolves nothing and contacts nothing**, which is why the panel renders the parsed name and
never a tick. A handle that does not exist parses exactly as well as one that does, and a tick
beside an unchecked claim is the failure ADR-0035 decision 8 named in a different phase.

## Decision 3 - There is no `apply_look`

A look reaches the pixels through `TonePass` and `ColourPass`, which reach them through
`aura_recipe::schema::merge`. Phase 14's rule for the fifth phase running. What `measure_look`
renders, it renders in order to *measure*, and it stores a number rather than a photograph.

The temptation here is real and specific: a look is a `StyleDelta`, `verify::shift` already produces
the recipe it implies, and writing that recipe is one line away. What would come out is AURA
deciding a wedding looks like somebody else's page.
`crates/aura-look/tests/no_recipe_writes.rs` is the grep that keeps it one line away.

## Decision 4 - Strength goes down and never up, in three places

`SetLookStrengthInput::fraction` is clamped in the command, again in `LookOverride::strength`, and
again by migration 31's `CHECK`. There is no `boost`, no `multiplier` and no `intensity` anywhere on
this surface, and no field a later change could put one in without widening a frozen shape.

Three locks rather than one because a slider that goes to 150 % is the single most likely thing a
later change would add, and because it is the difference between matching a look and caricaturing
one. Phase 21's rule: a ceiling can be lowered by a studio and raised by nobody.

## Decision 5 - A match figure is measured or it is absent

`LookBucketDto::afterDe00` and `LookMatchDto` are `null` rather than zero when nothing has been
measured. Phase 17's own comment about `match_de00` being an `Option`, and phase 29's about a
tuple's third element: a zero in a dE00 field is rendered by somebody, once, as a perfect match.

`matchSentence` leads with **how much of the gap closed**, not with whether a threshold was met.
Phase 27's rule: a match that closed nine tenths of a large difference and landed just outside the
ceiling is a match that worked, and one that landed inside because there was nothing to close is not
a result. `MatchLookPanel.test.tsx` asserts both.

## Decision 6 - The panel is first in the sidebar

A photographer who has come to AURA because they want their wedding to look like somebody's page
should meet that on the way in rather than find it under a menu. It renders whatever the project's
state is: a wedding with nothing analysed gets the sentence explaining what has to happen first,
rather than a disabled button with no reason beside it.

`coverageSentence` says both numbers - how many photographs a look can reach and how many there are
- because phase 18's rule is that a denominator goes on the wire, and a look applied over 40 % of a
wedding is a look over 40 % of a wedding.

## Decision 7 - `measure_look` is stoppable, through the command every long job uses

`MeasureLookInput::cancel_id` registers a `CancelToken` with `AppState`, and the existing
`cancel_job` command stops it. A second mechanism here would be a second answer to "is this still
running", which is the shape phase 28 spent a rule avoiding.

The check runs **between axes of the refinement**, not between its sweeps. One sweep is eleven axes
times four steps times eight frames of rendering, so a check that only ran between sweeps would
leave a photographer waiting through hundreds of renders after pressing Stop. It does not run
between individual steps, because the running best is only consistent at an axis boundary.

`MeasureLookDto::cancelled` is a field rather than an error, because stopping is a normal outcome:
nothing was stored, nothing changed, and an error banner would read as a failure.

## Decision 8 - `look_buckets` takes the project as well as the look

A look is stored once and can be measured against several projects, so "what does this look ask for"
and "what did it do here" are different questions. The first is answerable from the look alone; the
second needs a project. Passing the project fills each row's `afterDe00` from that project's match
report, and passing nothing leaves it `null` - which is the honest answer, because what a look did
to one wedding says nothing about another.

## Decision 9 - `look` is its own client namespace

`ui/src/ipc/client.ts` has one namespace per phase and this is not folded into `style`. The two
answer different questions from different evidence, and a caller that could reach either by
autocompleting the wrong function is a caller that will eventually report a look measured off
twenty-four JPEGs as a trained personal profile.

## The ten commands

| Command | Reads or acts | Note |
|---|---|---|
| `look_status` | reads | Carries `networkTransportAvailable`, and both coverage numbers. |
| `list_looks` | reads | Marks a look `stale` when the renderer has moved. |
| `parse_reference` | reads | Synchronous, infallible, resolves nothing. |
| `look_buckets` | reads | The matrix. Ten rows at most, one per kind of light. Takes the project, to fill the measured column. |
| `look_match_report` | reads | `null` until something has been measured. |
| `measure_look` | acts | The only long one. Walks, decodes, renders, refines, stores. Stoppable via `cancel_job`. |
| `select_look` | acts | Refused by the database for a look nobody has measured. |
| `set_look_strength` | acts | Down only. |
| `rename_look` | acts | |
| `forget_look` | acts | A project that had it selected falls back to the baseline. |

## Blast radius

```
crates/aura-app/src/contract/ipc.rs     +11 DTOs
crates/aura-app/src/look_commands.rs    new
crates/aura-app/src/lib.rs              +10 re-exports
ui/src-tauri/src/main.rs                +10 handlers
ui/src/ipc/types.ts                     +11 interfaces  (frozen; re-locked)
ui/src/ipc/client.ts                    +1 namespace, 10 wrappers
ui/src/components/look/MatchLookPanel.tsx   new
ui/src/App.tsx                          mounts it first
```

`scripts/check-ipc-surface.sh` reports 269 = 269 = 269 after this phase.
