# Phase 31 exit report - Matching a look somebody else published

**Status:** implemented conditionally.
**Gate:** `cargo run --package aura-cli -- verify --phase 31` (or `just phase-31-verify`).

## What shipped

`aura-core::contract::look` freezes the three media sources, the reference origin, the seven tone
landmarks, the zone tint, the band reading, the reference reading, the aggregate, the bucket, the
diagnostics, the profile, twenty-two reason codes, the bucket residual, the match report, the
outline, the override and `LookService`.

`aura-look` measures. `source.rs` resolves an address into provenance and a folder into files,
understands an Instagram data export's own layout, refuses the one route this build cannot fetch
through, and keeps one vote per photograph by content hash. `measure.rs` decodes once and takes
eleven readings, in CIELAB for every statistic and in HSV for the band assignment - two spaces
deliberately, because the renderer's HSL stage operates on the second wheel. `light.rs` sorts a
frame into one of ten lights from its pixels alone and never returns `Flash`. `aggregate.rs` folds
with medians and a MAD. `solve.rs` takes the difference, shrinks toward the global lean, bounds
everything, and then walks each parameter through the real renderer until the distance stops
falling. `verify.rs` renders both sides and measures what actually moved. `materialise.rs` writes
the result into phase 17's frozen `StyleProfile`. `store.rs` owns migration 31 and `api.rs` is the
frozen service and the pass.

Migration 31 stores six tables, two views and three triggers. Three error codes ship with runbooks.
Ten IPC commands (ADR-0064) feed a panel mounted **first** in the sidebar. ADR-0063 records the
decisions and `docs/match-a-look.md` says them in the product's own words.

## Measured

| What | Result |
|---|---|
| `tests/eval/look_eval.rs` | 10 tests, all pass |
| `crates/aura-look/tests/source.rs` | 14 tests, all pass |
| `crates/aura-look/tests/store.rs` | 7 tests, all pass |
| `crates/aura-look/tests/render_match.rs` | 3 tests, all pass |
| `crates/aura-look/tests/no_network.rs`, `no_recipe_writes.rs` | 6 tests, all pass |
| `ui/src/components/look/MatchLookPanel.test.tsx` | 15 tests, all pass |
| `aura-cli verify --phase 31` | every check passes on 30 real JPEGs |
| `scripts/check-ipc-surface.sh` | 269 = 269 = 269 |
| `cargo xtask contracts --check` | 83 entries, all locked |

The gate's own run, on a synthetic "light and airy" page written through phase 30's JPEG encoder and
read back: **+0.25 EV, +164 K, blacks at the bound**, no hue rotated in any band, both triggers
refusing with a control each.

`render_match.rs` is the one that matters most: solving a look and applying it **through the real
renderer** closes more than 40 % of the measured gap, and a gallery that already matches its
reference is left exactly alone.

## Conditions

**C1 - Every reference is synthetic. Sev 2.** Every reference photograph in every test and in the
gate is a plate this repository authored, carrying a look applied by an analytic transform in
`fixtures.rs`. No page has been measured, and no consented archive of somebody else's finished work
exists here. What is proved is the measurer, the lighting sort, the fold, the solver, the
refinement, the store and the refusals.

**C2 - The headline claim is unmeasured. Sev 2, and it is the one to close first.** Nobody has sat a
photographer in front of a gallery AURA matched and the page it was matched to and asked whether it
worked. "It looks like that account" is not a number anywhere in this build. `docs/match-a-look.md`
says so to the photographer in those words.

**C3 - Nothing can be fetched.** `MediaSource::PublicUrl` refuses on every call. Two facts, both
recorded in ADR-0063 section 4: this repository allows no socket outside `aura-cloud` and that
transport has no TLS, and a page's media in bulk is the platform's to grant to the account that
owns it. The variant is declared rather than omitted so the refusal is a sentence a photographer
reads.

**C4 - The baseline is placeholder-backed.** A look is a residual from what phases 15 and 16
decided, and every head underneath those phases is untrained. Closes with phase 05's C10 rather
than separately.

**C5 - The scale constants are authored.** `solve::initial`'s mappings were argued for, not fitted.
The refinement measures its way off them through the real renderer, so what ships on a project with
analysed frames is measured - but a project without them gets the authored answer, and it is
labelled `BaselineAbsent` and cannot be selected.

**C6 - No scene axis.** A reference photograph does not say what it is of. A look is about light and
not about subject, every scene group gets the same answer, and `SceneAxisNotLearned` is on every
look.

**C7 - The lighting sort is weaker than phase 15's and is a different measurement.** It reads what a
finished JPEG *renders* at, which is the light and the edit together with no way to separate them. A
photographer who warms every frame by 400 K has shifted every boundary in `light.rs` by 400 K.
Survivable because the bucket is an axis to group along rather than a correction - and arguably more
correct for that purpose - but it is not an illuminant estimate and must never be read as one.

## Five rules phase 31 adds

- **`LookService` is the only way to ask what somebody else's look is.** Twenty-eighth service of
  its kind and the first whose subject is **another person's work**. It is deliberately not
  `StyleService`: a profile fitted from three hundred of a photographer's own matched pairs and a
  look measured off twenty-four of somebody else's JPEGs carry different evidence, and a caller that
  could not tell them apart would report the second with the confidence of the first.
- **A reference is measured, never reverse-engineered.** There is no fitter here and there could not
  be one: the input has no original. Any later phase that finds itself recovering an edit from a
  finished file it did not make has misunderstood what this phase has to work with.
- **A capability the build does not have is a sentence, not an omission.** The unavailable route is
  offered, disabled, and explained, with what to do instead in the same breath. Phase 03 put the
  hardware on the wire and phase 30 put `NETWORK_TRANSPORT_AVAILABLE` there; this is the first time
  the missing capability is the thing a photographer came for.
- **A look is applied before the guards, and every guard re-runs after it.** Phase 17's rule,
  inherited unchanged and load-bearing here: it is the whole of this phase's skin defence, because
  there is no skin term in the look itself to bound.
- **A result that cannot be measured is not produced.** `materialise::into_style` takes the measured
  report as an argument rather than an `Option`, because `ProfileDiagnostics::overall_de00` is an
  `f32` and a look materialised before `verify::measure` ran would put a zero where every panel in
  the product renders a perfect match.

## Three things phase 31 got wrong first

**A handle parser that read `@some.photographer` as a web address.** The first rule was "no dot and
no slash means a handle", and Instagram handles legitimately contain dots - so the most ordinary
thing a photographer types went down the wrong branch. The fix made the rule about *structure*: an
explicit `@` is a handle, an Instagram host is a handle, anything with a scheme or a separator needs
a hostname that looks like one, and bare text is a handle. The same fix closed the other half of the
defect, which was worse: `../../etc/passwd` has a dot and a slash and was being recorded as a web
reference. It is only ever a label and nothing dereferences it, but it is a label a photographer
would have seen.

**A refinement whose cost grew with the size of the wedding.** The probe rendered every frame it was
given, and the search calls it a hundred and thirty-two times - so a thousand-frame project would
have spent a hundred and thirty thousand renders to pin down a median that eight frames already
pin down. `MAX_REFINE_FRAMES` is eight, sampled on a deterministic stride so a wedding that starts
in one room and ends in another is sampled across both. The *measurement* is uncapped, because that
number is about the photographer's gallery rather than about a sample of it.

**A baseline that would have been a measurement of mid grey.** `CatalogFrames` returns a neutral
frame for a photograph whose proxy is not built, which is right for a develop panel that has to open
on the night of a wedding and catastrophic here: every look would have been the difference between a
page and a rectangle. `own_frames` reads the real proxy and **skips** a frame it cannot get, which
is the fourth time this product has chosen a smaller honest answer over a larger silent one.
