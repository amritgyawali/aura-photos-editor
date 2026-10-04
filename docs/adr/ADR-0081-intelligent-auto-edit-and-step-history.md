# ADR-0081: Intelligent one-click editing, measured portrait finishing and step history

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
   - *Eyes*: iris detail only when an open eye is measured (whites clearly less
     saturated than the same person's cheek, in display values); sclera redness reduced only when clearly red (above 0.3, where healthy whites
     measure 0.1-0.25), and only on the measured whites beside the iris; flash red-eye only when
     the pupil is dominantly red; under-eye lift only when darker than the same cheek.
   - *Teeth*: whitened only when visible and measurably yellow, with a luminance selection
     that excludes lips; *shine*: softened only when specular skin covers >1.5 %.
   - *Lines & redness*: crow's feet and forehead lines softened (Wrinkle, fine band kept)
     only where line energy exceeds 1.35x the same cheek's; smile lines lifted
     (micro dodge and burn, at most 0.4) only where darker than the cheek beside them;
     redness beside the nose evened toward the person's own cheek (colour match).
   Eyes, brows, nostrils, lips and smile lines are excluded from blemish search. A spot must
   be compact (elongation at most 2.2), ringed by clean skin, and exceed thresholds scaled by
   the face's own texture; more than 15 such marks is treated as freckles and none are healed. Faces under 28 px
   between the eyes get skin retouch only.
4. **Step history is navigable.** `history_step` accepts `goto:<seq>` (and `goto:0` for the
   original). It is recorded as one navigation row and replayed like undo/redo, so going
   back discards nothing until a new edit is made. The Develop history list has a
   "Go back to here" button per step.

5. **Photographer-chosen automatic retouch.** `auto_retouch` takes an intensity (0.25-1.5)
   and switches for blemishes, lines & redness, eyes and teeth. It is an explicit request, so
   it replaces automatic operations even in an edited stack (manual operations are kept) and is
   saved as a user edit. The choice is stored in the report and reused by later passes.
6. **Retouch scope: face, body skin, or both.** `Options.scope` chooses what the automatic
   retouch may change. Body skin is planned from the face's own geometry: a search area from
   just below the chin to six face-heights down and 2.6 face-widths either side, a grid of
   low-variation patches that must match the face's sampled skin chroma (within 0.06) and
   lightness (0.4-2x), and two sampled-skin operations (texture at 0.8x, tone at 0.9x of the
   measured strengths, tolerance 0.06, edge protection 0.9) on a brush mask with the face erased.
   Fewer than four matching patches means the body is covered or out of frame and is skipped
   with a reason. Body operation IDs carry `-body-` and belong to the Skin step.

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
