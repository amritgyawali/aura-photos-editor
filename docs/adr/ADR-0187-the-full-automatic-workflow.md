# ADR-0187: The full automatic workflow - import, cull, confirm, then ten stages for every photo

Status: accepted, 2026-10-28.

## Context

The photographer asked for AURA to do everything automatically, the same way every time:
import one photo or a whole folder; for a folder, cull it and separate the best photos from the
rest **and check that with them**; then edit each photo, one by one - basic edit, colour
correction, colour grading, calibration, crop to straighten - retouch it - blemishes, skin
smoothing in the manner of Evoto, the face, the body - and give it an advanced edit.

Most of the pieces existed but were not one process:

* After an import the Start screen ran **Auto enhance** on every photo: light, colour and a face
  retouch. It never culled, never straightened, never retouched the body, never graded and never
  calibrated.
* The cull (ADR-0185) was a separate screen that had to be found and run by hand.
* **Auto advanced retouch** (ADR-0093) - levelling, background, hair, skin cleanup, frequency
  separation, dodge and burn, eyes, clothing, jewellery, background toning, grain, sharpening and
  quality control, face and body - ran only from a button on one photo.
* Nothing in the product set **calibration** automatically, and the advanced retouch's "colour
  grade" stage set vibrance and clarity, not a grade.
* Finish Folder ran a third combination (Auto enhance, or a provider's grade plus the portrait
  retouch).

## Decision

### 1. One process for every photograph (`crates/aura-app/src/full_edit.rs`)

`full_edit::run` is the full automatic edit of one photograph. It always reports the same ten
stages, in the photographer's own words and order:

| Group | Stage | Done by |
|---|---|---|
| Edit | 1 Basic edit | advanced retouch foundation: exposure, contrast, highlights, shadows, whites, blacks, levels, clarity, dehaze, noise |
| Edit | 2 Colour correction | white balance, tint, vibrance, colour fringing |
| Edit | 3 Calibration | **new**, measured (section 2) |
| Edit | 4 Colour grading | **new**, measured (section 3) |
| Edit | 5 Crop & straighten | a horizon the photo's own straight lines measure, cropped so no corner is empty |
| Retouch | 6 Blemishes | spots healed from nearby skin; moles and freckles kept |
| Retouch | 7 Skin smoothing | texture-keeping smoothing, pores restored, lines softened, tone evened |
| Retouch | 8 Face | eyes, under-eyes, teeth, lips, micro and medium dodge and burn, sculpting light |
| Retouch | 9 Body | neck, shoulders and arms matched to the same person's face skin |
| Advanced | 10 Advanced edit | background, hair, clothing, jewellery, background toning, grain, output sharpening, quality control |

Stages 1, 2 and 5 to 10 are `advanced_retouch::run_keeping`: the same measurement as Auto
enhance ("the same guards, in the same order"), plus everything Auto enhance never did. Its
retouch choices are resolved the way Auto enhance resolves them (`full_edit::options_for`):
**the choices a photographer saved for that photo win as they stand**; otherwise the product's
calibrated automatic finish (`fresh_skin_options`, the polished wedding finish - texture-keeping
smoothing, face and body) with the advanced stages' hair, clothing and backdrop amounts added
(`full_edit::default_options`). That default is listed in `has_custom_skin_preferences` as
nobody's choice, so a later Auto enhance does not mistake it for one. The first version used the
advanced retouch's own defaults and ignored saved choices; `tests/one_click.rs` caught it. **There is no second implementation**: the
stage a photographer reads is assembled from what the advanced retouch saved - its global fields
diffed before and after, and its retouch operations classified by name (`category_of`), so a
stage says "applied" only when the recipe really changed.

Every stage ends **applied**, **nothing needed**, **does not apply** (no person in the frame) or
**kept your setting** (a value a person set, or a look they chose), with what it changed or why it
changed nothing. Each stage that changed something is its own history step; the report is stored
in the recipe under `full_edit_v1`.

A **look** changes three stages and nothing else (`full_edit::Look`):

* `none` - every stage measured from the photo.
* `coming` - a reference fit follows and decides the colour: no grade here.
* `applied` - an edit profile or a provider already set light and colour. `run_keeping` leaves
  the foundation and vibrance/clarity alone; calibration runs only if the look left it neutral;
  no grade. Every retouch, levelling, grain and sharpening stage still runs, after the look,
  so its luminance selections see the exposure being exported.

### 2. Calibration, measured - corrections only

Read on a render of the edited photo without any automatic calibration or grade (so a repeat run
decides the same):

* **Shadow tint.** The median green excess of neutral shadow pixels against the median of
  neutral midtones. "Neutral" is judged **in light**: channels within 25% of the brightest. The
  first version judged it in bytes, and in the shadows a few bytes are a large share of the light:
  every photo of the test wedding read 10 to 20% greener, mostly from a dark green garland. A
  residual of 2% or more sets `shadows_tint` (positive is magenta), at most +/-12.
* **A primary clipping on its own.** When 0.3% or more of the frame has red at 250 or more while
  green and blue are under 200 - stage lights on a red sari - the red primary's saturation comes
  down 6 to 18 so the reds keep detail; the same for blue. A camera calibration that pushed skin
  toward some ideal colour is exactly what this repository refuses, so none of the primaries' hues
  move.

Then **checked on the render of the proposal**: a shadow tint that did not bring the shadows
nearer the midtones, or a primary cut that did not recover at least 10% of the clipping, is
withdrawn; when the faces' rg chromaticity moves more than 0.010 the whole calibration is halved
and checked again, then withdrawn.

### 3. Colour grading, measured and restrained

A split tone from the frame's own light: the highlights wheel takes the hue of the photo's own
highlights when that is a warm light (15 to 75 degrees) and a warm light's hue (40) otherwise, at
5% with people and 6% without; the shadows take its complement at 4% - **except with people in
the frame**, where the shadows stay untinted so no complexion turns cool. A black and white frame,
a frame more than 35% strongly coloured and a night frame are not graded, and highlights or
shadows already carrying colour are left as they are. Checked on the render: a grade that moves
the faces' colour more than 0.010 or clips more of the frame is halved, then withdrawn. It is a
style, and the report says so; every wheel is an ordinary editable control.

### 4. Import, cull, confirm, edit (`App.tsx`, `PhotoCullWorkspace.tsx`, `AutopilotPanel.tsx`)

* **One photo** imported: the full automatic edit runs on it straight away.
* **Several photos**: the Cull screen opens and culls on its own. A bar says what it chose
  ("AURA chose 62 best photos and left 38 out") and **nothing is edited until the photographer
  presses "Edit the N best photos"** - after looking at both lists, bringing back or leaving out
  anything, or changing the strictness. The confirmed set, in shooting order, is what the edit
  receives.
* The **Auto edit** screen then works through the set **one photo at a time**, saying which stage
  of which photo is running ("Editing photo 12 of 62: 5I7A8901.CR2 · stage 6 of 10: Blemishes"),
  and lists every photo's ten stages with their outcomes and the seconds it took.
* **Render final output** opens Export with **the photos just edited** chosen ("Which photos"),
  not the whole collection: the cull's left-out frames are not finished photographs and are
  exported only when the photographer picks "All N photos in the collection". Opened later, with
  nothing just edited, the Export screen offers the cull's best set the same way
  (`finishedPhotos` in `DeliveryPanel.tsx`). Before this, Export always rendered every imported
  photo, so a culled wedding delivered its rejects beside its keepers.

### 5. Finish Folder uses the same process

`one_click_commands::edit_one` calls `full_edit::run` on every path: `none` without a look,
`coming` before a reference fit, `applied` after a profile or a provider's grade. A wedding
finished unattended and a photo edited from the Start screen get the same ten stages.

### 6. A large import must not freeze the window

The photographer's first run of 100 Canon CR2 files hung the app and once had to be force-closed.
The engine log said why: within six seconds of the import, forty photos were "set aside" with
`AURA-RAW-2004 tier1 exceeded its 5000 ms deadline`. Two causes, both fixed:

* **Decodes had no limit.** A `get` decodes on the calling thread, and the grid, the cull and the
  editor each asked for previews of every new photo at once: up to a hundred 20-megapixel decodes
  side by side - gigabytes of memory and every core busy, so the whole machine stalled. Each
  decode's watchdog counted the time it spent queued for the CPU, so each one "timed out", was
  quarantined as unreadable, and kept running in the background. `DecodeSlots`
  (`aura-preview/src/service.rs`) now lets half the logical cores' worth (two to six) decode at
  once; the deadline starts when a decode starts; a request that waited first looks again in the
  memory and disk caches, so the same photo is not decoded twice; and batch work can never take
  the last slot, so the photo on screen is not queued behind a collection's prefetch. A camera
  file's deadline also grows with its size (`DecodeLimits::for_file`, 400 ms a megabyte, at most a
  minute): five seconds for a 38 MB CR2 measured a busy laptop, not a broken file.
* **A hidden editor locked the app.** The photo editor stays mounted behind every screen
  (ADR-0097) and reported "busy" while waiting for its first preview. After an import it started
  rendering the first CR2, and for as long as that took every sidebar button was disabled -
  including while the Cull screen was working. Waiting for a preview now disables only the
  editor's own controls; only a save in flight holds the rest of the app.

## Measured

Unit tests: the ten stages and their order; every advanced stage lands in one of them; every
automatic operation name lands in the right stage; green shadows get a magenta tint and neutral
ones none; a dark green object among neutral shadows is not a tint; clipped reds lower the red
primary and unclipped ones do not; a grade warms highlights and cools shadows without people and
leaves the shadows alone with people; monochrome, strongly coloured and night frames are not
graded; the face colour shift is measured inside the face boxes.

Real photographs: `crates/aura-app/tests/full_edit_photos.rs` (ignored, `AURA_FULL_EDIT_PHOTOS`,
`AURA_FULL_EDIT_LIMIT`, `AURA_FULL_EDIT_OUT`, with `AURA_PERF=1` the time of each part), and the
whole workflow in the real window, `scripts/test-full-auto-gui.py`. Results of the 100-photo run
are in `docs/full-auto-test-2026-10-28.md`.

## Consequences

* The default automatic edit after an import is now the full ten stages, not Auto enhance. It is
  slower - about a minute a photograph on the reference laptop for a CR2 with faces, most of it in
  the advanced retouch's planning, its stage saves and quality control - and it does much more.
* One new IPC command, `full_edit_photo`, streaming `full-edit` progress events. Its types live in
  `full_edit.rs` and `ui/src/ipc/fullEdit.ts`, beside the advanced retouch's own; no frozen
  contract file changed.
* `advanced_retouch::run_keeping` is the advanced retouch with a look's light and colour kept;
  `run` is unchanged.

## Addendum, 2026-10-29: watching each photo being edited

The photographer asked to see the photograph being edited, not only the name of the stage: the
original, then the photograph after each step, from the worst to the best, for every photo.

* `FullEditInput.preview` (default off). When on, `full_edit::run` sends `full-edit` events with
  `state: "picture"` and a `StagePicture` - a JPEG data URL of the saved edit at 960 px, rendered
  with `RenderPurpose::Analysis` so heavy retouch is not skipped, with crop, effects and
  sharpening as the photographer will see them. One for the original (stage 0), one after every
  advanced retouch step whose outcome was **Applied** (labelled with the stage and the step), one
  after calibration and one after the grade when they applied. A step that changed nothing makes
  no picture. A picture that fails to render is logged and left out; the edit never depends on it.
* `full_edit_picture` renders the saved edit as one more `StagePicture`: the last picture when a
  reference look is matched after the ten stages.
* The window asks for pictures only while the Auto edit screen is on screen, decided per
  photograph. Finish Folder, the harness and `one_click_commands` pass `preview: false`, so an
  unattended run renders nothing extra. `AURA_FULL_EDIT_PICTURES=1` makes the real-photograph
  harness edit as a watched window does, to time the cost (`full_edit.picture` with `AURA_PERF=1`).
* `LiveEditPreview` shows the photograph being edited, every step as a thumbnail, Before | After,
  and a replay of a finished edit. The last twelve photographs keep their pictures (a dozen
  screen-sized JPEGs each); older ones are dropped, because a thousand-photo run cannot keep them
  all in the window's memory.

Tested by `crates/aura-app/tests/full_edit_pictures.rs` (a watched edit sends the original first
and every picture after its stage started; an unwatched one sends none), `prepareCollection.test.ts`
(pictures only while watched; the matched look is the last picture) and `LiveEditPreview.test.tsx`.
