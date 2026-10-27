# One-click AI Color Match - verification report, 2026-10-24

What was built, how it was tested on real photographs in the real application, what it
does well, and where it still falls short. Decision record: ADR-0170. User guide:
[AI Color Match](../one-click-color-match.md).

## Summary

- Choosing a reference in the **AI Color Match** panel matches the open photo in one click,
  including a RAW nobody has edited yet. The reference appears beside the photo
  (Reference / Color Match views), with Recommended, My References and Presets lists,
  five preference levels, Save Preset, Sync, Real-time and Quick/Advanced modes.
- A new **look fit** makes the photo follow the reference's look instead of barely moving.
  On 36 real photo/reference pairs, Standard delivers the full look on 33, saves all 36, and
  brings the photo on average 23 % closer to the reference (the previous fit: 1-11 %).
- People and highlights are protected by measurement, not by turning the look down:
  skin brightness and colour are held per person, and new highlight clipping is pulled
  back inside the fit. When a protection still limits a match, the panel says which.
- The reference must be a different photograph: the photo itself, a copy, an edited copy
  or a reference created from it is refused, and Sync skips it.
- A shared defect in the recipe fingerprint was found and fixed; it made every second
  save on some photos fail as "The photo changed while applying edits".
- On a 30-megapixel Canon RAW a one-click match takes 6-13 s in the release build.

Evoto's own algorithm is not public and was not reverse-engineered. The workflow and the
behaviour were rebuilt on AURA's matcher; whether the results look as good as Evoto's to a
photographer has not been measured.

## Test material

| Role | Files | Notes |
|---|---|---|
| Photos to edit | `989A2698.CR3`, `989A2704.CR3`, `989A2708.CR3` | Canon EOS R, 6720 x 4480, the same Nepali bride; 2698 is the photo in the Evoto screenshot |
| References | four finished wedding photos from Pexels (IDs 3361200, 1801263, 1247756, 13470038) | other couples and places: golden field, vintage teal, warm night with bulbs, bright airy studio |

The references are deliberately different photographs of different people. Earlier test
references were graded copies of other frames of the same bride; they were replaced at
the photographer's request.

## 1. The engine on real photographs (release build)

`one_click_real` (ignored test in `color_match/one_click/tests/sweep.rs`) runs the real
one-click command on every photo with every reference, at Standard and at Extreme.

| Photo | Reference | Standard: saved | Extreme: saved | Colour distance (Standard) | Limited by |
|---|---|---|---|---|---|
| 2698 | golden field | 81 % | 77 % | 8.95 -> 7.55 | skin |
| 2704 | golden field | 100 % | 130 % | 10.24 -> 8.74 | - |
| 2708 | golden field | 100 % | 160 % | 11.17 -> 10.14 | - |
| 2698 | vintage teal | 90 % | 85 % | 2.65 -> 2.22 | skin |
| 2704 | vintage teal | 100 % | 117 % | 3.18 -> 2.20 | - |
| 2708 | vintage teal | 81 % | 77 % | 3.30 -> 2.70 | highlights |
| 2698 | warm night | 100 % | 130 % | 7.45 -> 6.83 | - |
| 2704 | warm night | 73 % | 77 % | 9.29 -> 8.74 | highlights |
| 2708 | warm night | refused | refused | - | moved away from the reference |
| 2698 | bright airy | 39 % | 37 % | 3.23 -> 2.73 | skin |
| 2704 | bright airy | 100 % | 94 % | 3.52 -> 2.70 | - |
| 2708 | bright airy | 21 % | 19 % | 3.43 -> 3.15 | highlights |

- 22 of 24 matches saved; 6 of 12 Standard matches at the full look.
- Each match took 6-13 s, with one or two full-resolution checks.
- The refusal is reported as "This reference's look does not suit this photo: even a light
  match moved its colours away from the reference instead of toward it. Nothing was
  changed." Sync lists that photo as unchanged, with the sentence.

### What the look fit changed (same twelve pairs, full strength)

| | previous (conservative) fit | look fit |
|---|---|---|
| distance reduction | 1-11 % | 10-45 % |
| full-strength result passes its skin check | 11 of 12 (strict check; it hardly moves) | 10 of 12 (look check) |
| face lightness change | at most 0.002 | at most 0.045 |

### Problems found and fixed while testing

1. **Faces darkened by a dark reference.** A literal colour-distribution match took the
   photo to 0.95 of the reference's statistics, but darkened the bride's face and blotched
   it red, because the reference was mostly dark foliage. The look fit now keeps the
   photo's own middle grey and corrects on each person's skin samples.
2. **A warm reference made the background cooler.** The reference's white balance was read
   from its most neutral pixels (a white dress); the warmth was in everything else. The fit
   now also follows the overall tint in shadows, midtones and highlights, ignoring strongly
   saturated objects.
3. **Highlights clipped by up to 7.6 %.** Warm and bright looks lifted the saturated red sari
   and veil past white. The fit now measures clipping on its own samples and eases the top
   of the curve, colour in the bright zones, the clipping hues and the highlight tint until
   it stays within 0.4 %. On the worst pair clipping went from +7.6 % to +0.04 %.
4. **The recipe fingerprint had two answers.** A value within half a millionth of a whole
   number was written as `0.000000` and read back as `0`. Every later save on that photo was
   refused. Fixed in `aura_recipe::hash::number`, with a round-trip test.
5. **Slow checks.** Reading skin and tone on the full 30-megapixel render took 17 s per
   check; it now samples on a stride (1 s, identical verdicts), and a 512 px prediction
   picks the level so one full check usually confirms it.

## 1b. Second round: the full look, more often (2026-10-25)

The first round still turned the whole look down whenever one protection failed. A
per-person diagnosis showed why: the fit corrected a stand-in for skin colour and for clipping,
not what the checks actually read. The fit now checks itself with the checks' own
instruments - the skin readings on the 512 px picture, and clipping on real renders - and fixes
the part that fails instead of turning everything down. Four further changes: a new look is
judged against the photo without the previous look; colours missing from the reference are
turned toward its palette (a wedding red keeps its hue); a cool look no longer drains skin; and
a photo the look fit cannot bring closer gets a cautious version instead of a refusal.
ADR-0170's addendum has the details.

The test set was widened to **nine RAW photos** of the wedding (portraits, full-length, seated,
a group of four, day and evening) with the same four references: 36 pairs.

| Standard, 36 pairs | Round 1 | Round 2 |
|---|---|---|
| full look (100 %) | 28 (first measured on 12 pairs: 6) | **33** |
| saved at all | 35 | **36** |
| limited by skin | 6 | 1 |
| refused | 1 | **0** |
| Extreme saved | 35 | **36** |

- On average a Standard match moved the photo 23 % closer to its reference (0-50 %).
- The three below 100 %: bright-airy on two photos whose light is far from that studio
  reference (28 % and 66 %, where more would move them away from the reference), and
  vintage-teal on one photo, held to 21 % by the skin check.
- On the first three photos, every pair now reaches the full look (12 of 12, against 6 of 12
  at the start of the round).
- Each match needs one full-resolution check. On an otherwise idle machine most matches took
  7.5-14 s in the release build; the final run shared the machine with another build and
  measured a median of 23 s.

## 2. The real application window

`scripts/test-color-match-gui.py` drives the debug desktop build through its own window
(WebView2 DevTools), against a fresh catalogue each run. Debug builds are several times
slower than release, so times here are not the product's.

**20 of 20 steps passed**, with no page errors (final run, 2026-10-24).

| Step | Result |
|---|---|
| Import the four references | each under 1.5 s |
| Recommended lists all four | order for the bride photo: bright airy, warm night, vintage teal, golden field |
| One click on golden field (unedited RAW) | matched; "Reduced to 81 % of the reference look so skin stays natural. Its unedited look was saved first, so Undo returns to it."; distance 8.95 -> 7.55 |
| Reference beside the photo | sharp 1280 px reference preview on the left, matched photo on the right |
| History | "Starting point for reference colour", then the match |
| Light, Natural, Strong, Extreme, Standard | all saved; Light and Natural at their full amount, Strong 82 %, Extreme 77 % (skin) |
| Save Preset | listed under Presets and usable in one click |
| Create Reference from the matched photo, then use it on the same photo | refused: "This reference is the same photo you are matching. Choose a different photo as the reference; nothing was changed." |
| The same created reference on a different photo | matched |
| Sync the whole collection to vintage teal | "Synced 2 of 2 (1 reduced)"; amounts 90 % and 100 % |
| Reference view, then Remove matched look | reference shown alone; "Matched look removed. The photo is back to its edit from before matching." |

A preference change on an analysed photo took about 6 s in the debug build; a first match
or a match on another photo about 60 s (debug), against 6-13 s in release (section 1).

**Rerun after the second round (2026-10-25): 20 of 20 again.** On the bride RAW, Standard now
saves the full look (was 81 %), Strong its full 125 % (was 82 %) and Extreme 130 % (was 77 %);
Sync matched both other photos at 100 % (was 90 % and 100 %). The machine was shared with
another build during this run, so its debug-build times are not representative.

### Problems the window found, all fixed

1. **Out of memory in the side-by-side view.** The editor holds the full-quality preview of a
   30-megapixel file as a ~160 MB bitmap; the Color Match view mounted a second copy and the
   page crashed ("Error code: Out of Memory"). The view now asks for its own 1600 px render.
2. **The comparison pictures had zero height.** The canvas positions images absolutely; the
   figures holding them now fill the canvas.
3. **A remembered, empty Presets tab hid every reference.** The panel now falls back to
   Recommended when there are no presets.
4. Harness fixes: each run starts from an empty catalogue and clears remembered settings;
   it waits longer for a first window.

Screenshots of every step: `.work-checks/color-match-gui/` (not committed; they contain a
client's photographs).

## 3. Automated tests

| Suite | Result |
|---|---|
| UI (vitest), all 112 files | 1,073 of 1,073 passed |
| `ColorMatchStudio` and `PhotoStudio` view tests | all passed |
| `aura-app` colour-match unit tests | all passed except one that already failed before this work |
| `one_click_color_match` integration (unedited photo, step-down, presets/previews/recommendations, duplicate references) | 4 of 4 |
| `aura-recipe` (including the new fingerprint round-trip test) | all passed |
| clippy on the changed crates, `check-banned`, IPC parity (351 = 351 = 351) | clean |

Three tests fail and failed the same way on the starting commit (b1cfc6d), before this
work; they belong to the transfer engine's own accuracy targets and its legacy engine
name, and were left alone.

## 4. Limits

- **Not every reference suits every photo.** A reference shot in very different light
  (a night scene for a bright outdoor portrait) is followed partly or refused, with the
  reason shown. That is the protections working, not a bug.
- **Skin is protected, so a strong colour look does not recolour faces fully.** Skin can
  warm or cool with the look; its brightness moves at most about 0.04 and it keeps most of
  its colour. That is deliberate.
- **"Exactly the same colours" is not possible from a different photo.** A finished photo of
  another scene says how the result should look, not what was done to it; AURA matches its
  character (warmth, tint, contrast, colourfulness, per-colour treatment). For an exact
  match, use an original + edited pair of the *same* photo in Paired reference.
- **Not measured:** whether photographers rate these matches as close as Evoto's. That needs
  side-by-side viewing on real weddings.
