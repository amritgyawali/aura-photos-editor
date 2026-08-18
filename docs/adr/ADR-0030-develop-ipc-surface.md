# ADR-0030 - The develop IPC surface

**Status:** accepted · **Date:** 2026-08-17 · **Phase:** 14 · **Supersedes:** nothing

## 1. Context

Phase 14 gives the product a renderer and a document. The develop panel needs to read that
document, change one parameter at a time, walk a history, take a snapshot, reset, ask for a
picture, and know which engine drew it. Nine commands, and the interesting question is what
is *not* among them.

## 2. Decision: nine commands, and three things that are not on the wire

| Command | What it does |
|---|---|
| `image_recipe` | One photograph's edit, or the camera's own starting point. |
| `set_param` | Change one parameter, as a person. |
| `history_step` | Undo, redo, reset to original, reset to AI suggestion. |
| `image_history` | The steps, the snapshots, and what is available. |
| `snapshot` | Take or restore a named point. |
| `render_image` | A proxy, at a level, in a space, for a purpose. |
| `render_caps` | The backend, its precision, its ceiling, its degradation. |
| `develop_status` | How much of the wedding carries an edit. |

**No destination.** No input shape has a path. Invariant 1 says the RAW is opened read-only,
and this surface has nowhere to name a file even if a caller wanted one. Phase 30 owns
delivery, and when it arrives it adds a command here rather than a field to one of these.

**No way to overwrite a photographer's parameter.** `set_param` calls
`aura_recipe::schema::merge` with `EditSource::User`, and a person may always overwrite a
person. Every automated pass from phase 15 onward calls the same function with an automated
source and is refused. The protection is not implemented on this surface at all, which is
why no argument on this surface can switch it off.

**No decision.** Nothing here keeps, rejects, delivers or culls, and
`DevelopPanel.test.tsx` greps the rendered panel for those four words. Phase 12 decides
galleries; a develop panel that also decided would be a second answer.

## 3. Decision: pixels travel inline, as base64

`RenderDto::rgb_base64` carries the image rather than a cache path or a URL. Three reasons,
in order of weight:

1. **There is no file.** This phase writes no image to disk, so a path-based surface would
   have required inventing one - and the moment a render has a path, something will read it
   after the recipe has moved on.
2. **A render is a pure function of four inputs**, and `render_hash` travels beside the
   pixels. A caller that wants caching keys on the hash, which is correct at any layer.
3. The cost is a third more bytes over an in-process channel, which is not a cost.

The base64 encoder is twenty lines in `develop_commands` rather than a dependency, and its
test compares against the standard alphabet's own examples.

## 4. Decision: a parameter is a dotted path, and the panel renders a list

`RecipeDto::params` is a flat list of `{path, value, protected, stage}` rather than a nested
mirror of the recipe. The panel picks the controls it shows by path and in its own order, so
adding a parameter to schema v2 makes it appear on the wire without a UI change, and a
control the panel does not know about is invisible rather than broken.

`stage` is on the wire because the panel can then say what a slider costs - and because it is
what a future interactive path uses to decide whether to re-render from a cached buffer.

`protected` is on the wire because section 6.4's promise is worth nothing if a photographer
cannot see it. The panel renders a dot with the sentence "You set this. AURA will not change
it."

## 5. Decision: every render says what it left out

`RenderDto::notes` carries every stage that did not run, with a reason slug and an
`is_caveat` flag the backend computes. "Not requested" and "already done upstream" are not
caveats and the panel does not show them; the six that are - an absent mask generator, an
absent operator, an absent restoration model, an absent geometry model, an absent lens
profile, an absent camera profile, and the interactive path's deliberate skip - are rendered
as one line each in plain words.

The slugs are the backend's vocabulary and the sentences are the panel's. That split is
phase 09's decision about reason codes applied to a different surface: a catalog full of
English cannot be translated, and neither can an IPC payload.

## 6. Consequences

**Good.** The UI cannot break invariant 1, cannot break section 6.4, and cannot render a
photograph without being told which engine drew it. A photographer sees the protection they
were promised and the caveats they would otherwise have to guess at.

**Bad.** Eight commands is eight more `#[tauri::command]` registrations, and `render_image`
is the first command on the surface that can take longer than the 50 ms the module
documentation asks of a command. It is a proxy render on a processor path; the interactive
budget is waived (ADR-0029 section 4) and the panel shows a pending state. When a GPU backend
lands the number falls inside the budget and nothing about the shape changes.
