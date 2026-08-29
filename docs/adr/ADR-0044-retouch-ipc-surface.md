# ADR-0044 - The retouch IPC surface

**Status:** accepted · **Date:** 2026-08-20 · **Phase:** 20 · **Supersedes:** nothing

The second of phase 20's two ADRs. [ADR-0043](ADR-0043-portrait-retouch-and-texture-protection.md)
covers the decisions; this covers the wire, which for this phase is not a formality: it is the
first command surface in the product whose subject is a **person** rather than a photograph, and
the first where a wrongly shaped field would be a feature this product has promised never to
build.

## 1. Context

Phases 15 to 19 each added a panel that shows what AURA decided about one frame and lets a
photographer disagree. This one has three differences:

- a strength belongs to a **person across the gallery**, so setting one on a frame writes a row
  that changes four hundred other frames;
- a protect row is a claim about somebody's face that survives every re-analysis, and one kind of
  it can never be withdrawn;
- the phase's headline guarantee is a **measurement**, so it has to reach the panel as a number
  rather than as a badge.

## 2. Decision: eight commands, and what each of them may touch

| Command | Reads | Writes |
|---|---|---|
| `retouch_status` | the project outline | nothing |
| `image_retouch` | one plan | nothing |
| `protected_features` | one person's protect set | nothing |
| `retouch_review_queue` | the weakest plans | nothing |
| `accept_retouch` | one plan | `reviewed` |
| `set_retouch` | one plan | `preset` on the frame, or one person's gallery strength, and `recipe.retouch[]` |
| `set_protection` | one face's landmarks | one `retouch_protected` row |
| `retouch_pass` | proxies, faces, scenes | every plan, then every recipe |

`set_retouch` is the only one that reaches a recipe, and it can reach exactly one field:
`recipe.retouch[]`. There is no path from this surface to the global exposure, to the curve, to a
mask or to the restoration block, which is what keeps phases 15, 16, 19 and 20's boundaries
structural rather than remembered. Phase 19's `local_commands` made the same choice about
`recipe.masks[]` and the argument is unchanged.

## 3. Decision: a strength is gallery-wide on the wire, and says so

`SetRetouchInput` carries a `photoId` **and** an `identityId`, and the strength applies to the
identity rather than to the photograph. That reads oddly until the alternative is written down: a
per-frame strength would let a photographer set the bride to 0.8 on the ceremony frames and leave
her at 0.6 on the reception ones, which is precisely the failure section 6.4 exists to prevent -
a person whose skin changes character halfway through their own wedding.

The `photoId` is still on the input because the panel is looking at a photograph and the response
carries that photograph's plan back. `RetouchPanel.tsx` labels the control "This person,
everywhere in this wedding", and a test asserts the sentence.

## 4. Decision: the protect rectangle arrives in frame coordinates and is stored face-normalised

A photographer draws on a photograph, so the wire takes what they drew. The backend projects it
through that frame's own eye landmarks into the face frame - origin between the eyes, x along the
eye-to-eye line, unit the inter-ocular distance - because that is what makes the protection follow
the person.

A face with no landmarks is **refused** rather than approximated, with `AURA-ML-5097`. Phase 09's
rule is that `[[0,0],[0,0]]` means unknown and must never be read as the top-left corner, and a
protect row written in a coordinate system nobody can reproduce would protect a random part of
every other photograph of that person - which is worse than not protecting anything, because it
is invisible.

## 5. Decision: an absolute protection has no control, rather than a disabled one

`ProtectedFeatureDto::absolute` is on the wire even though the panel could derive it from
`kind == "tattoo"`. Two reasons, and the second is the real one:

- the panel should not have to know the vocabulary to render it safely;
- a **disabled** control invites somebody to look for the setting that enables it, and there is
  not one. The panel renders the sentence "AURA never alters tattoos" where the button would be,
  and `RetouchPanel.test.tsx` asserts that no clear button exists for an absolute feature.

The refusal itself lives in three places - the type, the service and a database trigger - because
section 10.1 gates tattoo removal at **zero** rather than at a small number, and a promise
enforced in one layer is a promise until somebody writes a second caller.

## 6. Decision: the texture measurement is on the wire, with its sample count

`TextureReportDto` carries `bandRatio`, `floor`, `resolves`, `withdrawn` **and** `measuredOn`.
The last is the one a reviewer would cut, and it is the one that keeps the rest honest: a ratio
measured over eleven samples of skin is arithmetic rather than evidence, and a panel that printed
`0.94` either way would be presenting a guess with three decimal places.

`withdrawn` is separate from `passed` for the reason ADR-0043 section 2 gives: "we re-solved twice
and got there" and "we gave up and applied nothing" are two different outcomes and a photographer
needs to be told which one happened.

## 7. Decision: no command returns a crop of anybody's skin

Every rectangle on this surface is a coordinate. There is no field in any DTO that could hold
image data, so "no face crop leaves the device for retouching" - section 9's SEC task - is a
property of the shapes rather than of an exporter. The panel draws on the pixels it already has.

## 8. Consequences

- `AppState::retouch(project)` is the first service accessor in the product that takes a project,
  because two of `RetouchService`'s frozen methods are about a person within a wedding. A service
  that had to be handed a project on every call would let a caller mix two weddings in one answer.
- `AppState::project_of(photo)` is new and small, and exists because the panel asks about one
  photograph at a time while the service is project-scoped.
- The desktop shell registers eight new commands. It does not build in this repository - the
  Tauri build script needs an icon that is not checked in - which is a pre-existing condition
  rather than one this phase introduced, and it is recorded in the exit report rather than
  papered over.
