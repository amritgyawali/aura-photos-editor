# ADR-0060 - The curation IPC surface

- **Status:** accepted
- **Phase:** 29
- **Related:** ADR-0059 (curation selection and album composition), ADR-0058 (the autopilot IPC
  surface), ADR-0056 (the QC IPC surface), ADR-0026 (the cull IPC surface), ADR-0042 (geometry, and
  the aspect variants this surface hands out)

## 1. Context

Every command surface before this one answers a question about work the product has already done to
a photograph. This one answers a different kind of question: *what should I do with these?* - and the
reader is not checking a measurement, they are making a decision about their own portfolio, their
own album and their own feed.

That changes two things. The shapes have to carry enough for somebody to disagree with a pick
quickly, and every command that records a disagreement has to be as cheap as the one that produced
the suggestion. A curation panel where accepting is one click and rejecting is a modal is a panel
that measures agreement it did not earn.

## 2. The eleven commands

| Command | What it is for |
|---|---|
| `curate_status` | The project header: what has been curated, from how many keepers, at what versions |
| `curate_project` | Run the whole curation pass for a project and return its outline |
| `curate_bw` | The B&W picks, best first, each with its eight-band mix and its reasons |
| `curate_heroes` | The hero grid, in rank order, each with its score, its chapter and its reasons |
| `curate_album` | The album plan: spreads, chapter spans, coverage, rhythm and pairing |
| `curate_spread` | One spread in detail: both frames, the pairing terms, why they are together |
| `curate_social` | The grid set, the story set, the hero, their aspect variants and their captions |
| `curate_teaser` | The teaser set |
| `curate_set_order` | Record a photographer's album order |
| `curate_decide` | Record accept or reject on one pick - a hero, a B&W frame, a social slot or a teaser |
| `curate_export` | The album or social specification as JSON, CSV or a layer list |

Eleven rather than section 4's five panels, for the reason every surface since phase 15 has had more
commands than panels: a panel that fetched an album to show one spread would send 120 spreads to
draw two frames, and the spread view is the screen a photographer spends the most time on.

`curate_project` returns the outline rather than the whole `CurationResult`. A 120-spread album with
its coverage report, twenty heroes, two hundred B&W picks and three social sets is a large payload to
send when the panel that asked is about to render a header, and the four read commands fetch what
each panel actually draws.

## 3. Four shapes that carry more than they look like they need to

**`CurateBwDto.mix` is eight named bands, never a preset name.** Section 13's second acceptance
criterion is that B&W suggestions come with per-frame mixes rather than a single preset, and a
surface that sent `"grade": "high_contrast"` would make that criterion unfalsifiable from the panel.
The eight values are on the wire, the panel renders them as a small bar chart, and a photographer can
see that two frames got different answers.

**`CurateHeroDto.binding` beside `score`.** Which diversity constraint was binding when this frame was
chosen - its chapter quota, its moment, its shot scale, or none of them. "Why is this one a hero and
that one not" is answered by the constraint far more often than by the score, and a panel that showed
only the score would leave a photographer comparing two numbers that differ by 0.01.

**`CurateSpreadDto.facingKnown` beside `facingScore`.** A spread whose subjects' facing could not be
measured is not a spread whose subjects face inward. Phase 27 made the same distinction between clean
and skipped, phase 24 between blocked and unknown, and on this build - where phase 06's detector finds
no faces - `facingKnown` is false almost everywhere. A panel that rendered a zero facing score as a
failed pairing would be reporting a defect in every spread of every album.

**`CurateAlbumDto.rhythmMeasurable` beside `rhythmScore`.** The same shape one level up: a rhythm
score of 1.000 measured over 8 % of an album is not a claim about the album. Both numbers travel
together and the panel renders the score in grey below a threshold.

## 4. Why `curate_set_order` takes a whole order and `curate_decide` takes one pick

Two different operations with two different failure modes.

An album reorder is a drag: the panel already holds the sequence, the photographer moved one frame,
and the cheapest correct thing is to send the sequence back. Sending a move instead would make the
server reconstruct a state the client already has, and a dropped or reordered message would leave
the two disagreeing about an album silently. The whole order is idempotent and self-describing, and
`curate_set_order` refuses one that reorders chapters, that contains an image outside the gallery, or
that repeats one - which is the validation an out-of-order move could not have.

A decision on a pick is a click on one card and it stays one row. There is no bulk decide on this
surface at all: phase 27 established that agreeing with forty findings and authorising forty actions
are different judgements, and here the equivalent is that accepting twenty heroes one at a time is
the photographer *looking at* twenty photographs, which is the whole point of the panel.

## 5. Why there is no `curate_apply`

There is no command on this surface that changes a photograph, writes a recipe, exports a file or
alters the delivered gallery. ADR-0059 section 3 has the argument; this is where it is enforced on
the wire.

The one shape that comes close is `curate_export`, and it produces a **specification** - JSON, CSV or
a PSD-ready layer list naming images and positions - returned as a string for the shell to save.
Nothing in `aura-curate` opens a file. Section 2.2 puts album page rendering out of scope and phase
30 owns delivery; a curation surface that could write a JPEG would be a second export path, and two
export paths is two answers to what was delivered.

## 6. What is deliberately absent

No threshold read or write. `curation.toml` is a product manager's file that ships with the build; a
studio may tighten it on disk, and there is no command that lets a panel widen a bound the contract
owns. Phase 21 wrote that rule and phases 22, 24 and 27 inherited it.

No B&W strength. The mix is solved per frame and a photographer either takes it, edits the eight
bands in the develop panel that already exists, or does not. A `strength` slider on this surface
would make the mix a preset scaled by a number, which is the thing section 6.1 says not to build.

No caption free-text field on the *generation* path. `curate_decide` records that a caption was
rejected; editing one is a photographer typing into their own scheduler, and a caption the product
stored after a person wrote it would be a caption the grounding check never saw and the delivery
report would nonetheless attribute to AURA.

No hero count, no album size and no teaser size as command arguments beyond the album's, because the
other three are fixed by section 6.4 and by the KPI. `curate_album` takes a target spread count so
that "make it a 60-image album instead" is one command; it is bounded by `ALBUM_MIN` and `ALBUM_MAX`
and refuses anything else.

## 7. Consequences

- Eleven commands, eleven `#[tauri::command]` definitions, eleven typed client wrappers. The
  three-way count phase 27 made a gate now reads 231 = 231 = 231.
- Five panels mount under `ui/src/components/curate/`, and `CuratePanel` is the container that fetches;
  the five views are pure and take props, which is the split phase 25 established and the reason they
  are testable without a window.
- `curate_export`'s three formats are exercised by the phase gate rather than only by a unit test,
  because "exports cleanly to external tools" is section 13's fifth acceptance criterion and a format
  nobody parsed is a format nobody has checked.
