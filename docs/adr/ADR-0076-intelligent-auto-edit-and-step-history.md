# ADR-0076: Intelligent one-click editing, measured portrait finishing and step history

Accepted, 2026-09-30.

## Context

Auto enhance measured exposure and, for detected faces, added three skin operations
(texture, tone evening, local light) in a single history entry. Photographers asked for a
pass that finishes a photograph without a person — light, colour, noise, sharpening and
the usual portrait finishing (blemishes, eyes, teeth, shine) — while keeping every
decision reversible and editable. A single history entry made it impossible to keep
"the light" and discard "the teeth" without opening the retouch stack.

## Decision

1. **One pass, several saved steps.** `smart_edit::run` plans everything first, then saves
   up to six ordinary history entries in a fixed order: light & colour, sky balance, skin,
   blemishes, eyes, teeth & shine. Each is an AI-sourced merge through `schema::merge`, so a
   field a person set is never overwritten. A step that changes nothing is not saved, and a
   repeat run of an unchanged plan saves nothing (retouch groups are staged so the stack is
   identical at every step on a repeat).
2. **Global decisions are measurements.** Tone comes from the existing histogram
   correction. White balance is a gray-pixel estimate on the renderer's own input frame,
   with faces, hair and neck excluded, applied only partially (65 %, or 50 % for strong
   casts) so a warm room stays warm; it is skipped below 1.5 % neutral coverage or a cast
   under 0.035 in log-ratio units. Vibrance depends on measured saturation and scene;
   clarity is never raised on portraits; dehaze needs a raised dark-channel floor; noise
   reduction needs a measured Immerkaer noise level; sharpening masks skin on portraits.
   A landscape with a bright sky gets one feathered gradient that only touches bright
   pixels above the measured horizon.
3. **Portrait finishing is measured against the same face, never against an ideal.** On the
   2048-px proxy, per retouched face:
   - *Blemishes*: a spot must be **redder** than the skin immediately around it (background
     from skin-coloured pixels only) and small. A mark that is much darker, or darker
     without being redder, is recorded as a possible permanent mark and kept. At most 12
     spots per face; each is an individual texture-aware patch heal with a skin donor.
   - *Eyes*: iris detail only when an open eye is measured (sclera brighter than the
     person's own skin); sclera redness reduced only when measured; flash red-eye only when
     the pupil is dominantly red; under-eye lift only when darker than the same cheek.
   - *Teeth*: whitened only when visible and measurably yellow, with a luminance selection
     that excludes lips; *shine*: softened only when specular skin covers >1.5 %.
   Eyes, brows, nostrils and lips are excluded from blemish search. Faces under 28 px
   between the eyes get skin retouch only.
4. **Step history is navigable.** `history_step` accepts `goto:<seq>` (and `goto:0` for the
   original). It is recorded as one navigation row and replayed like undo/redo, so going
   back discards nothing until a new edit is made. The Develop history list has a
   "Go back to here" button per step.

## Consequences

- No new model is shipped; everything runs offline on the CPU with the bundled YuNet
  detector. The detection policy and weights are unchanged.
- Measurements can be wrong: a red birthmark smaller than the spot limit could be healed,
  a strongly lit sclera could be mistaken for an open eye. Every operation is listed with an
  "Auto (face N)" tag in Retouch and can be disabled or removed individually.
- No frozen contract changes: `HistoryStepInput.action` is already a string and the report
  lives in the existing recipe extension with `serde(default)` fields.
- Not attempted: straightening, subject/sky segmentation, hair/eye semantic masks, body
  reshaping, generative fill. These remain listed as unavailable in the capability report.
