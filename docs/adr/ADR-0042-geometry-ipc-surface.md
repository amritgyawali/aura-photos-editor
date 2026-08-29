# ADR-0042 - The geometry IPC surface

**Status:** accepted · **Date:** 2026-08-26 · **Phase:** 23 · **Supersedes:** nothing

The wire half of phase 23. [ADR-0041](ADR-0041-geometry-lens-straightening-and-crop-safety.md)
covers the decisions; this covers what crosses the boundary between `aura-app` and the desktop
shell, and - as with every IPC ADR since phase 08 - most of it is about what deliberately does
not cross it.

## 1. Six commands

| Command | Reads or writes | What it is for |
|---|---|---|
| `geometry_status` | reads | The panel's project header: coverage, the kept-original rate, missing lens profiles. |
| `image_geometry` | reads | One photograph's plan. |
| `geometry_review_queue` | reads | The frames whose geometry is worth a look, weakest first. |
| `plan_geometry` | writes | The resumable pass, and the recipes it writes through the merge. |
| `set_framing` | writes | The framing a photographer chose. Reverting is this command. |
| `accept_geometry` | writes | "Looks right", so the frame leaves the review queue. |

Six is the same count phase 19 needed and for the same reason: three reads, one pass, two
records. There is no seventh for "revert", which is section 4.

## 2. What cannot cross this boundary

**No pixel.** `GeometryPlanDto` has no field that could hold one. The evidence a reason carries
is a `CropRectDto` - the face that would have been cut - rather than a crop of it, which is
phase 13's rule that evidence can never be a pixel, for the eighth phase running. The panel
draws the rectangle over the preview it already has.

**No lens profile table.** A profile is an *input to a decision*. What reaches the wire is the
name of the profile that matched and `lensSynthetic`, which says whether anybody measured it.
Shipping the table would invite the panel to look a lens up itself, and a second lookup is a
second answer.

**No corner fill.** A keystone opens two corners and a rotation opens four. There is no
parameter on this surface for what goes in them, because nothing goes in them - section 2.2
puts filling in phase 24, and a field reserved for it now is a field somebody sets before it
means anything.

**No album choice.** Which crop an album page uses is phase 29's decision.
`GeometryService::variant` is how it will ask, and it is a service method rather than a command
because phase 29 is not a panel.

## 3. The panel's three obligations

**Restraint is rendered first, and separately.** Section 10.1 asks that seventy per cent of
frames keep their framing. `GeometryReasonDto::restraint` is a boolean on every reason so the
panel can put "what AURA left alone" above "what AURA changed" without keeping its own list of
which codes mean which - and a panel that reads as empty on seven frames in ten is a panel a
photographer stops opening.

**A safety check nobody ran is never rendered as one that passed.** `facesChecked` and
`handsChecked` are counts and `isEvidence` is the predicate. A crop over a frame with no
detected faces satisfies the face rule trivially; showing a tick for it would tell a
photographer their crop is provably safe when nothing was checked. On this build `handsChecked`
is zero on every photograph and the panel says so in a sentence rather than in a badge.

**A fabricated profile says so.** `lensSynthetic` reaches the panel because a photographer told
a lens was profiled when it was invented has been misled about their own photographs. Every
profile in this repository sets it.

## 4. Reverting is `set_framing`, not its own command

**Decision.** Reverting is `set_framing` with the whole frame and zero degrees.

A `revert_framing` command would be one line shorter to call and would almost certainly be
implemented as *deleting* the override row - which is a revert the next background pass undoes,
silently, some minutes later. Recorded as an override, a revert is a decision the photographer
made and it survives a re-analysis exactly as any other choice does. `GeometryOverride::revert`
is the same value on the Rust side, and `is_revert` is how the panel labels it.

This is why section 13's "original framing is always one click away" holds without a special
case: `crops[0]` is the frame as shot on every plan, guaranteed by `GeometryPlan::new` being
the only constructor, so the click is selecting an entry the panel is already rendering.

## 5. The pass writes recipes through the merge, and the merge is finer here

`plan_geometry` walks what it planned and writes `lens`, `geometry.crop`, `geometry.rotate` and
`geometry.perspective` through `aura_recipe::schema::merge` with `EditSource::Ai`. Every one of
those is a **scalar** path, so `user_edited_fields` protects them individually: a photographer
who dragged a crop owns `geometry.crop` and still receives a lens correction.

That is *finer* granularity than phase 19 could manage. Its masks are an array, and an array is
atomic in the merge - somebody who touched one mask owns the whole list. Phase 19 said that was
the right side to err on and it was, given the shape. Here the shape is better, and the panel
can say precisely which of the four a person now owns.

## 6. What the pass reads, and what happens when it reads nothing

`build_input` gathers phase 07's scene, phase 11's tilt and distractions and crop hint, and
phase 06's faces. **Every one is optional, and an absent one is not a failure.** A frame nobody
has analysed is planned with no tilt, no regions and no distractions, and is delivered exactly
as it was shot.

That is the correct behaviour rather than a fallback: a phase with a seventy-per-cent restraint
target should leave a photograph it knows nothing about exactly where it found it. It is also
why `plan_geometry` reports `planned` and `keptOriginal` rather than only `planned` - a wedding
at 100 % coverage and 100 % kept-original has either been shot beautifully or been walked past,
and only the second number distinguishes them. Phase 19 wrote that rule about `acted_on`.

## 7. Consequences

- `ui/src/ipc/types.ts` gains ten shapes; it is in `contracts.lock`, so the digest moves.
- `AppState::geometry()` opens the bundled profile table and `crop_rules.toml` on every call.
  That is deliberate and matches `composition()` and `cull()`: the files are small, they are
  read once per command rather than once per photograph, and a service that cached them would
  keep serving a rules file a product manager had just edited.
- `GeometryPanel.tsx` is the fourteenth panel and the first whose primary content is a list of
  things that did not happen.
