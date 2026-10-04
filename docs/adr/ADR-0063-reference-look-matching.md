# ADR-0063 - Matching a look somebody else published

**Status:** accepted
**Date:** PHASE-31
**Deciders:** CTO, TLC, MLL, PM, SEC
**Supersedes:** nothing. **Amends:** nothing.

## Context

A photographer points at an Instagram account and says "make my wedding look like that". It is the
most common thing anybody asks a photo editor to do, and it is the request phase 17 does not
answer: phase 17 learns what *this* photographer does, from pairs of their own originals and their
own delivered finals. It needs an archive, three hundred matched pairs, and a photographer who
already has a look.

This phase answers the other question. The input is a page of finished JPEGs made by somebody else,
and there is no original for any of them.

## Decision 1 - Measure appearance, never recover an edit

Phase 17 is handed a RAW and the JPEG somebody made from it, so it can ask *what did they do* and
recover twelve parameters by coordinate descent. Here there is only the JPEG. Nobody knows what the
reference photographer started with, what camera made it, or how much of what is on the screen is
the edit and how much is the light that afternoon.

So this phase does not try to recover an edit. `ReferenceReading` is eleven **distribution
statistics** over a whole frame or a zone of it - seven tone landmarks, three zone tints, eight hue
bands, two chroma quantiles and a rendered white point - and `LookAggregate` is their robust middle
over a page.

This is phase 26's rule one level up. **Match appearance, never parameters.** There is nothing in
this contract that reads a slider, and there could not be: the reference is a JPEG on somebody's
page and it has no sliders.

## Decision 2 - The look is a residual, and the baseline is measured rather than assumed

`solve::initial` takes the difference between two aggregates: what the page does, and what the
photographer's own photographs look like **after phases 15 and 16 have decided them, rendered**.

That makes a look a residual by construction, which is what keeps phase 17's rule - "a style is a
residual, and the baseline is never re-derived" - true here rather than merely restated. Two
identical aggregates produce `StyleDelta::neutral`, so a reference a gallery already matches asks
for nothing, and there is no state of this system in which switching the feature on makes a
photograph worse than leaving it off.

The alternative - solve against a fixed neutral - is phase 17's own condition C4 repeated: an
absolute edit wearing a residual's shape. A project with no analysed frames gets exactly that, and
it is labelled `LookCode::BaselineAbsent` and cannot be applied, because `project_look_needs_a_match`
refuses a selection without a measurement.

## Decision 3 - No skin term, and the schema cannot express one

Every reading in this phase is whole-frame or whole-zone. `LookProfile` has no skin field, migration
31 has no skin column, and `solve` writes zero into every band's hue.

This is the fifth application of the rule phase 15 wrote and phases 16, 17 and 25 inherited: **a
skin target is measured, never assumed, and the schema cannot express an alternative.** The form the
trap takes here is new and is the hardest to see. Finding skin in a stranger's photograph, with no
face detector that works and no identity to scope it to, means declaring a hue window and calling
what falls inside it skin - which is exactly the fixed skin constant this product has refused four
times, wearing a measurement's clothes.

The defence a look gets instead is the expensive one. A look is applied *before* phase 16's skin
guard, which grades this photographer's own frame's own skin through the real renderer, measures
what actually moved, and attenuates or withdraws the colour half. Phase 17's rule - the shift
happens before the guards, and every guard re-runs after it - inherited unchanged.

The hue rotation is withheld for the same reason and it is worth stating separately. A rotation
solved from a whole-frame band statistic is applied to every pixel in that band, and most of a face
at every skin tone is in the orange band - so the one parameter that would most obviously move
somebody's skin is the one with the least evidence behind it. Saturation and luminance per band are
kept, because both are magnitudes: getting them slightly wrong makes a colour slightly too strong,
and getting a hue wrong makes a person a different colour.

## Decision 4 - Nothing is fetched, and the refusal is a feature rather than a gap

`MediaSource::PublicUrl` is declared and refuses on every call. Two separate facts point the same
way and both are recorded here because a later reader will ask.

**The first is a property of this repository.** `scripts/check-banned.sh` fails the build on an
outbound socket anywhere outside `aura-cloud`, and `aura-cloud`'s transport is a hand-written
HTTP/1.1 client with no TLS - ADR-0009 waived it - so there is no route from this process to an
`https://` host at all, for any purpose. Phase 04's rule is that the gateway is the only crate that
opens a socket, and a client-gallery fetcher is not a model provider. Phase 30 met the same wall
from the other side and recorded it as condition C3.

**The second is about the platform.** Reading a page's media in bulk is something Instagram grants
through its own API to the account that owns the page. The unofficial routes - scraping the web
view, replaying a private endpoint - are against its terms, break without notice, and would put a
photographer's own account at risk to save them a folder drag.

The variant is **declared rather than omitted**, which is the shape `aura_generative::inpaint` uses
for a diffusion tier it refuses on every call: a route the product does not have is a sentence a
photographer can read, and a variant that does not exist is a feature request nobody can see the
shape of. `aura_core::contract::look::refuse_fetch` is the one place that decides what the refusal
says, so the panel, the command and the gate all say the same two things.

**The link is not wasted.** `ReferenceOrigin::parse` validates it and the handle is stored beside
the look and rendered in every report, so "matched to @somebody" is a fact the product knows even
though the bytes arrived by another route. What it never does is claim the account was checked:
nothing resolves anything, a handle that does not exist parses exactly as well as one that does, and
the panel renders no tick. ADR-0035 decision 8's argument, in a different phase.

## Decision 5 - The bounds are tighter than phase 17's, and the asymmetry is the point

`MAX_EXPOSURE_DELTA_EV` is 0.5 here against phase 17's 0.67, and `MAX_TEMPERATURE_DELTA_K` is 600
against 800.

A photographer teaching AURA from their own archive is teaching it something they are entitled to be
sure about: it is their work, they made every one of those decisions, and three hundred pairs is a
lot of evidence. Somebody pointing at a page they admire is expressing a *preference about a look*,
from twenty-four JPEGs, about photographs made by somebody else in a place they have never been. A
page that reads bright may be bright because that photographer shoots in Greece.

## Decision 6 - The scale constants are authored, and the refinement is why that is acceptable

`solve::initial` maps a difference between two aggregates onto the recipe with a table of scale
constants. Nobody fitted them; there is no data in this repository to fit them on; they are
documented as the argument that produced each one. On their own they would be exactly the kind of
number this product has refused to ship nine times.

`solve::refine` is why they are acceptable anyway. It renders the photographer's own frames with the
initial guess **through the real renderer**, measures how far the result actually landed from the
reference, and walks each parameter until the distance stops falling. The constants decide only
where the search starts; what ships is measured. It can only improve - the initial delta is scored
first and returned unchanged when no step beats it - so a refinement that finds nothing costs
renders and changes nothing.

When there is nothing to render, `refine` is not run and `initial`'s answer ships with
`LookCode::BaselineAbsent` on the row. That is the honest failure: a guess, labelled as one.

## Decision 7 - One axis, and the replication is honest

A reference photograph does not say whether it is a ceremony or a reception. Phase 07's classifier
needs a catalog row, an embedding and a wedding's own timeline, and a JPEG on a page has none of
them.

So `LookProfile::buckets` is keyed by `LightingBucket` and by nothing else, and there is no scene
axis in this phase and no code path that could invent one. When a look is materialised into a
`StyleProfile` the same lighting-conditioned delta is written into **every** `SceneGroup`, and
`LookCode::SceneAxisNotLearned` is on the diagnostics and in the panel.

The alternative - write only the global lean and leave every leaf empty - is worse and not more
honest. Phase 17's resolution walks bucket, then group, then global, so an empty leaf means a
portrait made in candlelight resolves past the candlelight answer to the page's overall lean, and
the one axis that *was* measured is thrown away at the moment it applies. Replicating says "this is
what the page does in this light, whatever the photograph is of", which is exactly what was
measured.

The lighting sort itself is weaker than phase 15's and says so. Phase 15 asks what colour the light
in the room was, from a wedding's own neutrals across hundreds of frames. This reads what a finished
JPEG *renders* at - the light and the edit together, permanently, with no way to separate them. That
is survivable because the bucket is an axis to group along rather than a correction, and for that
purpose sorting by rendered appearance is arguably the more correct thing to do: the photographer's
treatment is part of what is being grouped. `LightingBucket::Flash` is never returned, because flash
is an EXIF fact rather than a visible one and guessing would put reference photographs in a bucket
the photographer's own frames - which do have EXIF - would rarely land in.

## Decision 8 - A look is a `StyleProfile` to everybody else, and a `LookProfile` to itself

Phase 17's rule is that `StyleService` is the only way to ask what a photographer's look is, and
every consumer - phases 15 and 16 applying it, 25 normalising a gallery, 26 matching a second
camera, 27 explaining, 28 running it unattended - reads a `StyleProfile`. A look that kept its own
shape would need all of them to learn a second one.

So `materialise::into_style` fills phase 17's frozen shape, and `LookService` remains the only way
to ask what a *reference* look is. The two questions are different: "what does that page do" is
answerable from a folder of JPEGs, and "what do you do" needs an archive of pairs. Collapsing them
would report a look measured off twenty-four JPEGs with the confidence of a profile fitted from
three hundred matched pairs.

`into_style` requires the measured report as an argument rather than taking an `Option`.
`ProfileDiagnostics::overall_de00` is an `f32` rather than an `Option<f32>` - phase 17 could always
measure it, so it never needed to express "not measured" - and a look materialised before
`verify::measure` has run would put a `0.0` in the field every panel in the product renders as a
perfect match. Phase 22's rule in the place it would have been easiest to miss.

## Consequences

* A new crate, `aura-look`, depending on `aura-core`, `aura-catalog`, `aura-raw`, `aura-render`,
  `aura-recipe` and `aura-style`. It depends on no cloud crate and on no brain crate.
* Migration 31: six tables, two views, three triggers.
* Three error codes, `AURA-ML-5146` to `AURA-ML-5148`, each with a runbook.
* Ten IPC commands (ADR-0064) and one panel, mounted first in the sidebar.
* Two grep-as-tests: `no_network.rs` and `no_recipe_writes.rs`, the eleventh in this repository.

## Blast radius

```
crates/aura-core/src/contract/look.rs      new frozen contract
crates/aura-core/src/errors/ml.rs          +3 codes
crates/aura-catalog/migrations/0031_look.sql
crates/aura-look/**                        new crate
crates/aura-app/src/look_commands.rs       new
crates/aura-app/src/state.rs               +look_store, +look
crates/aura-cli/src/phase31.rs             new gate
ui/src/components/look/**                  new panel
ui/src/App.tsx                             mounts it
```

Nothing frozen was amended. `StyleDelta`, `StyleProfile`, `LightingBucket` and `SceneGroup` are
phase 17's and are read rather than widened; `add_deltas` is a free function in the new contract
precisely so that adding two of them does not require an `impl Add` on a frozen type.
