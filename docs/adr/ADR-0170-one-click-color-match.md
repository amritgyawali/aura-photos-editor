# ADR-0170: One-click AI Color Match and the look fit

Date: 2026-10-24. Extends ADR-0130, ADR-0131, ADR-0139 and ADR-0160. Number chosen clear of
the 0141-0169 range other branches may take.

## Context

The photographer asked for Evoto's AI Color Match screen in AURA: choose a reference photo
and the open photo takes on its look in one click, with the reference shown beside the
photo, Recommended and My References lists, a preference up to Extreme, Save Preset and
Sync. They then asked for the colours to follow the reference closely, and for the
reference and the photos being edited to be different photographs, never duplicates.
Evoto's internal algorithm is not public and was not reverse-engineered; this ADR copies the
workflow and the observable behaviour, not the implementation.

AURA already had the engine behind most of this, but measured on real photographs
(three Canon CR3 wedding portraits, four finished wedding photos of other couples from
Pexels) it barely moved a photo toward a reference: the existing fit shrinks every
different-scene estimate toward no edit, because it was built not to guess an editor's
settings. A one-click Standard match moved the colour distance by about 11 % and the strict
skin check then held it to about 40 % of that.

## Decision

### One click (`color_match::one_click`, four IPC commands)

1. **`quick_color_match`** analyses when there is no current one-click analysis for that
   reference, then applies. A photo with no saved edit first saves its unedited state with
   `save_first` ("Starting point for reference colour"), so Undo returns to it.
2. **Prediction, then one full check.** The largest amount that passes the three checks on
   the 512 px fitting picture is found first (steps of 10 %), then confirmed at full
   resolution; a refusal there steps down by 20 % (at most six full checks, never below
   15 %). The report names the binding check (`limitedBy`: highlights, skin or distance).
   On a 30-megapixel CR3 in a release build a one-click match takes 6-13 s.
3. **Preference levels** are amounts: Light 50 %, Natural 75 %, Standard 100 %, Strong
   125 %, Extreme 160 %.
4. **A different photograph only.** When the reference's structure matches the photo being
   matched (`content::similarity` >= 0.95, the threshold the matcher already treats as the
   same frame), analysis refuses before anything is stored. This catches the photo itself,
   a copy, an edited copy, and a reference created from this photo. Sync reports such a
   photo as skipped.
5. **`color_reference_preview`** returns a 1280 px JPEG of the reference for the side-by-side
   view, stored on import and capture, deleted with the reference; never used for fitting.
6. **`recommend_color_references`** ranks the library for the open photo by subjects,
   brightness, colourfulness and shape. It is a similarity ranking, not a prediction.
7. **`color_match_presets`** keeps up to 64 named presets (reference, level, strengths).
8. **Sync** runs the one-click command on each selected photo or the whole collection,
   measuring every photo on its own.

### The look fit (`transfer::look`)

One-click analysis fits a transform that makes the photo take on the reference's look, not
its histogram (a literal histogram match darkened a bride's face because the reference was
mostly dark foliage):

* the reference's neutral axis, then its **overall tint** in shadows, midtones and
  highlights, read from the colours that are not strongly saturated (so a red sari or a
  green lawn does not count as the grade);
* its **tone shape** placed around this photo's own middle grey, with a bounded share of
  the brightness difference;
* its colourfulness, blended with the shared-colour ratio, and per-hue colour, hue and
  lightness where both photos hold that colour.

Then two corrections on this photo's own samples:

* **skin**: each person's mean skin lightness may move at most 0.035 (OKLab L) and skin
  keeps 85-130 % of its colourfulness, through a local tone correction around skin
  lightness and a colour gain on the hue sectors the skin occupies;
* **highlights**: when the look clips more of the photo than it already had (+0.4 %),
  the top of the tone curve is pulled back toward the original, the colourfulness curve
  eased, the clipping hues calmed and the highlight tint eased toward neutral, until it
  does not.

A look session is checked with a **look skin guard**: per person, skin brightness within
0.06, colourfulness 70-145 %, and no hue turn over 15 degrees in the midtones or 20 in the
highlights. Warmth along the skin's own hue travels; a face turning green, grey or magenta
does not. The strict guard (5 a\*b\* units per zone) is unchanged for every other session,
and the detailed panel still uses the conservative fit.

### A defect fixed in the recipe hash

`aura_recipe::hash::number` decided integrality before rounding to six places, so a value
within half a millionth of an integer (`3e-7`, `0.9999997`) had two canonical forms: the
in-memory recipe hashed as `0.000000`, the same recipe read back hashed as `0`. Every later
save on that photo was refused as a concurrent edit. Integrality is now decided on the
rounded value; a test checks that canonical text reads back to itself across [-3, 3].
Stored bodies were already canonical and read back unchanged.

## Measurements (release build, real photographs)

Three Canon EOS R CR3 portraits (6720 x 4480) of one bride; four finished wedding photos of
other couples (golden field, vintage teal, warm night, bright airy). Distance is the
matcher's colour-and-tone distance to the reference, lower is closer.

| | conservative fit | look fit |
|---|---|---|
| distance reduction, full strength, 12 pairs | 1-11 % | 10-45 % |
| Standard one-click saved at 100 % | - | 6 of 12 |
| Standard one-click saved, any amount | - | 11 of 12 |
| face lightness change | 0.000-0.002 | within 0.045 |

Remaining limits, by design: a reference whose light is far from the photo's is held back
(bright-airy on a photo whose sky is already 5 % blown: 21 %) or refused with a sentence
naming why (warm-night on a bright banquet scene moved colours away from the reference at
every amount).

## Consequences

- No frozen contract, migration, model or dependency changes; storage is new `setting`
  keys only. The IPC `prepare_color_match` is unchanged and conservative.
- `matcher::candidate` and `matcher::measured` are now `pub(super)`.
- The Color Match canvas view asks for its own 1600 px render. Mounting a second copy of the
  full-quality preview of a 30-megapixel file (a ~160 MB bitmap) ran the page out of memory.
- Three tests already failed on b1cfc6d, before this change, and fail the same way after
  it: `color_reference_match.rs::different_dimensions_match_strengths_restore_and_new_manual_edits_are_kept`
  (expects the legacy engine name), `transfer::tests::a_batch_matched_to_one_reference_converges_on_one_look`
  and `reference_grade_transfer.rs::a_new_photograph_takes_an_edited_references_look_through_the_renderer`.
- Not measured: whether photographers judge a one-click match as close to the reference as
  Evoto's. That needs side-by-side viewing on real weddings.

## Addendum: the look fit checks itself with the guard's instruments (2026-10-24)

The first look fit still lost most of its look on half of the real pairs: one check failing
turned the *whole* look down. Diagnosis on every skin region showed the only failing check was
skin colourfulness, and that the fit had been correcting a proxy (pooled samples, a different
colourfulness measure) rather than what the check reads. The highlight check failed for the same
reason: the fit estimated clipping on finished 8-bit pixels, while the renderer applies the look
inside its pipeline, where near-white sky behaves differently. Changes:

1. **Skin, measured as the guard measures it.** After the fit, the look is applied to the
   512 px picture and each person's regions are read with `appearance::readings`, the guard's
   own instrument, against tightened limits (brightness 0.045, colourfulness 80-135 %, hue
   turn 12/16 degrees). The worst region is corrected - colour gain on the sectors that
   person's skin occupies, a local tone correction at their lightness, the renderer's skin
   protection for a turn - until every region passes.
2. **A cool look no longer drains skin.** Where skin loses colour, the look's colour-balance
   shift is eased at the skin's lightness only: the shift ran against the skin's own hue, which
   hue gains cannot undo. The look stays in shadows, highlights and backgrounds.
3. **Highlights, measured on real renders.** The fit renders itself through the real pipeline
   at fitting size and eases the top of the tone curve, bright-zone colour, the clipping hues
   and the highlight tint until it adds at most 0.7 % new clipping, inside the full check's
   1 %. The one-click prediction uses the same 1 % margin as the full check.
4. **A new look is judged against the photo without a look.** Checks for a one-click session
   compared a new reference's look against the photo *with the previous reference's look
   applied*, so switching references was refused for not improving on the old look's
   highlights. `matcher::guard_baseline` removes the earlier reference grade for one-click
   sessions.
5. **Palette pull.** Colour families the photo holds and the reference does not (foliage under a
   golden look) are turned toward the reference's nearest dominant colour, by at most 34
   degrees, scaled by how missing the colour is. Families with median OKLab chroma above 0.05-0.10
   keep their hue, so a wedding red stays red.
6. **Cautious fallback.** When the look fit moves a photo away from the reference at every
   amount, the cautious estimate is tried before refusing, and the panel says a lighter version
   was used.
7. `LookOptions::tint_chroma` and `palette` are options; a test build reads
   `AURA_LOOK_OPTIONS` to compare settings. Production defaults are unchanged by it.
