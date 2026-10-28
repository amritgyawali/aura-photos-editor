# ADR-0185: A hundred checks per photograph, and a cull the photographer reviews

Status: accepted, 2026-10-28.

## Context

The measured cull (ADR-0088) answers one question - is this frame unusable? - with four calibrated
rules: black or blown, out of focus, motion blur, and the duplicates of a burst. On a wedding most
frames are usable, so it said nothing about which of two usable frames is the better photograph, and
a burst's keeper was chosen on edge strength and eye contrast alone.

It also ran only inside Finish Folder, between import and edit. A left-out photograph was named in
`photo-cull.json` in the export folder and nowhere in the app: nobody could see the left-out set,
bring a photograph back, or tell the run to remove nothing after seeing what it would remove.

The photographer asked for both: many more measurements per photograph so the best of a bulk
collection is chosen on evidence, and a review in which the left-out photographs are visible,
restorable one at a time, and restorable all at once.

## 1. One hundred checks, in ten groups of ten

`crates/aura-app/src/cull_checks.rs`. Each check has a stable id, a group, a sentence saying what it
measures, a weight inside its group and a scale that turns its raw value into a score from 0 to 1.

| Group | What it measures |
|---|---|
| Exposure | black frame, blown area, mid level, crushed shadows, per-channel clipping, dynamic range, tonal spread, subject level |
| Focus | strongest and peak edges, fine detail, centre, best region, sharp area, subject separation, thirds points, the main face, rank in the collection |
| Motion and shake | four edge directions and their ratios, the 1/focal-length rule, shutter speed, doubled edges, face smear, fine detail against edge strength, rank |
| Noise and quality | luminance, shadow and colour noise (Immerkaer), ISO, signal to noise, resolution, posterisation, texture, haze (dark channel), contrast |
| Colour | cast on neutral mid-tones, highlights and shadows, saturation, over-saturation, colourfulness (Hasler and Suesstrunk), skin inside the YCbCr skin box, skin saturation, palette, tint |
| Composition | level horizon, placement on a third or the centre, headroom, faces cut by the edge, bright edges, balance, face size, clutter, dark corners, quiet space |
| Faces | detection confidence, size, every face as sharp as the sharpest, exposure, even light, turned to the camera, mouth width against the eyes, teeth showing, inside the frame, head level |
| Eyes | open (contrast against the skin), both found, sharp, catchlight, red-eye with flash, both equally open, whites showing, not shadowed, gaze, reliable positions |
| Light | modelling on the face, hard shadow edges, flash falloff, mixed light, backlit subject, shine, highlight roll-off, face contrast, ambient level, separation |
| Among similar photos | place in its burst, place in its moment, eyes and sharpness against the best similar shot, uniqueness, opens a new scene, exposure and colour against its neighbours, a deliberate shot rather than a spray, rank of its own score in the collection |

Ninety are measured from one photograph - the proxy the edit reads, the faces the portrait detector
found on the thumbnail, and the camera settings in the catalog. The last ten need the frames around
it and are filled by `measured_cull::decide`.

**None of them is a learned judgement.** `faces_smile` measures mouth width against eye distance and
`faces_teeth` bright, unsaturated pixels between the mouth corners; neither says anything about how
somebody felt, and the names and `docs/how-aura-culls.md` say what is measured. A model that guessed
at taste on data this repository does not have would be confidently wrong; a measurement is at worst
imprecise, and every one of these can be read back in the panel with its raw value.

## 2. How the hundred become one score

* **A check that cannot be measured abstains.** A landscape has no eyes, a monochrome frame no cast,
  a JPEG without EXIF no shutter speed. `None` is not a low score, and a group with nothing to say is
  left out of the total.
* **Groups combine as a weighted geometric mean** (phase 12's fusion rule): focus 2.0; exposure,
  motion and eyes 1.2; context 1.1; faces 1.0; composition 0.7; colour 0.6; noise and light 0.5.
* **A failed core group takes the total down with it.** On real photographs a frame three stops
  under scored 78 against 92 as shot, because one failed group in nine costs only its share of a
  mean. Exposure, focus, motion and eyes are groups nothing else makes up for, so when the worst of
  them is below 60 the total is multiplied by `worst / 0.6`.
* **Some checks are ranks within the collection** - fine detail, texture, clutter, overall quality.
  "Best in the bulk" is relative, and an absolute threshold for texture is a statement about one
  camera. A rank never scores below 0.4, so being the softest of a sharp wedding is a small penalty.

## 3. What the cull now decides

The order in `decide`:

1. The four hard rules, unchanged.
2. **Very low score** - a frame whose own ninety checks add up to under 25 is left out (`LowScore`).
3. The implausible-share guard, unchanged and now also covering 2: a collection where the rules
   would reject more than 40% is a style, and every rule is withdrawn.
4. Bursts, unchanged in how they are found; the keeper is now the frame with the best total when
   every frame of the burst has the checks, and sharpness and open eyes otherwise.
5. **Weaker shots of one moment** - similar frames within 20 seconds (difference hash within 12
   bits). The best is kept, so is every frame within 12 points of it up to half the moment, and the
   rest are left out naming the frame delivered instead (`WeakerOfMoment`).
6. **Top picks** - the best-scoring fifth of what is delivered, if they score 60 or more.

Hand-edited and unmeasured frames are never left out, as before.

## 4. The review, and the photographer's word

`crates/aura-app/src/photo_cull.rs`, six commands, and the **Cull** workspace
(`ui/src/components/cull/PhotoCullWorkspace.tsx`), second in the sidebar.

* **A cull of its own.** `start_photo_cull` measures and decides on a background thread and edits
  nothing; `photo_cull_status` and `photo_cull_cancel` follow and stop it.
* **The review is stored in the catalog**, in the `setting` table under `photo_cull.v1.<project>`,
  so it survives a restart and no migration is needed. Finish Folder writes the same row when it
  culls, so a left-out photograph from an unattended run can be brought back afterwards.
* **Choices are phase 12's `cull_override` rows.** "Bring back" is `keep`, "Leave out" is `reject`,
  "Undo my choice" deletes the row. That table is what every cull in the product already obeys, it
  is untouched when a cull is re-run, and so a choice survives a fresh cull. **Keep every photo
  (remove none)** is `keep` for everything left out, and **Undo all my choices** hands every photo
  back to the measured decision.
* **Finish Folder uses the reviewed decision.** When the collection has a stored review covering
  every readable photograph, the run delivers that review with the photographer's choices instead of
  measuring again, and says so in its notes. Otherwise it culls as before and applies the choices.
  `OneClickFinishInput::only_photos` finishes just the photographs named - the Cull screen's
  "Edit & export only the ones brought back" - with no cull.

Nothing on this surface deletes, moves or edits a photograph.

## 5. Calibration, and what it does not prove

`crates/aura-app/tests/cull_checks_photos.rs` (ignored, `AURA_CULL_PHOTOS`, `AURA_CULL_DEGRADE`)
reads a folder of JPEGs the way the cull reads them and prints every check's spread.

The first run over 300 photographs of `F:\AURA-Indian-Wedding-Test\source` - professional, licensed
wedding photographs, so a calibrated check should pass most of them - found eighteen checks failing
between a third and 95% of them. Three causes, all fixed:

* **Colour cast read the scene.** Measured over every pixel, a red and gold mandap is a 40% cast.
  Cast, tint and shadow cast are now read on near-neutral pixels only (saturation under 0.25 and
  0.35); the failing share went from 84% to 21%.
* **Eye boxes on small faces read the analysis grid.** Eyes are measured only when they are at least
  12 analysis pixels apart; red-eye only when the flash fired, because without one the centre of an
  eye reads iris and skin.
* **First-principles thresholds** for smile, teeth, frontal, face size, mixed light, separation,
  roll-off, haze, resolution and level were re-centred on the measured distribution.

After calibration: overall p10 84, p50 91, p90 95; no photograph under the low-score line. With
`AURA_CULL_DEGRADE` each of 60 photographs is also scored damaged, in one collection with the
originals, as a cull would score them:

| The same 60 photographs | p10 | median | p90 |
|---|---|---|---|
| As shot | 85.9 | 92.4 | 94.9 |
| Defocused (Gaussian 6 px at 2048) | 35.9 | 55.5 | 69.3 |
| Motion smear (29 px horizontal box) | 48.2 | 59.3 | 73.6 |
| Three stops under | 50.9 | 61.0 | 70.1 |
| Tripled (blown out) | 48.9 | 65.4 | 82.1 |

Before the core-group rule of section 2 the damaged medians were 73 to 80 - close enough to the
originals that a dark frame could out-score a sharp one on composition. The gap between a photograph
and its damaged copy is now 27 to 37 points at the median, more than twice the 12-point moment
margin, so a damaged frame of a moment loses to its good sibling. None of the damaged copies falls
under the low-score line except one blown-out frame: an unusable frame with no better sibling stays
in the Best list unless a hard rule names it, which is the direction this cull errs in.

What this does **not** prove: that a photographer would pick the same frame. The calibration set is
distinct photographs rather than bursts, so the moment rule and the context checks were exercised on
synthetic frames and on the decision tests, not on a real burst of a real wedding. No agreement study
against a photographer's own picks has been run. The thresholds are a starting point that real
collections should move, and every one of them is in one table in `cull_checks.rs`.

## Blast radius of the contract change

Additive only: no existing field, command or type changed shape.

* `crates/aura-app/src/contract/ipc.rs` - seven new DTOs (`PhotoCull*`, `SetPhotoCullChoiceInput`)
  and `OneClickFinishInput::only_photos`, `#[serde(default)]`, so every existing caller's payload
  still parses.
* `grep -rn "OneClickFinishInput {" crates ui/src-tauri` - five struct literals, all updated:
  `one_click_commands.rs` (`automatic_start`), `tests/one_click.rs` (three), `tests/ingest_recovery.rs`.
* `ui/src/ipc/types.ts` - the same seven types and an optional `onlyPhotos?` on
  `OneClickFinishInput`; `grep -rn "oneClickFinish(" ui/src` - `OneClickRunner.tsx` (unchanged, the
  field is optional) and `PhotoCullWorkspace.tsx`.
* `scripts/check-ipc-surface.sh` - 360 = 360 = 360 with the six new commands.

## Consequences

* `measured_cull::Measure` carries the raw values, `Verdict` the scores, `Report` the check ids;
  `photo-cull.json` grows by about 1 kB a photograph.
* Six IPC commands, seven DTOs and `OneClickFinishInput::only_photos`: a frozen-contract change,
  recorded here and re-locked in `contracts.lock`.
* Measuring the checks adds a few hundred milliseconds a photograph at the dev profile, on the image
  gate's lanes; the preview decode still dominates a cull.
