# The full automatic workflow on 100 real photographs, 2026-10-28

What was tested: the whole workflow of ADR-0187 - import, automatic cull, the photographer's
confirmation, then the ten-stage full edit on every confirmed photo - **only by running the AURA
app** and driving its window, on the photographer's folder `G:\to do edit full by AURA`: 100 Canon
CR2 raw files (about 38 MB each, 3.8 GB) of a Nepali wedding, indoor stage under warm lights and
outdoor daylight.

How: `scripts/test-full-auto-gui.py "G:/to do edit full by AURA" --out I:/aura-full-edit/gui100
--bring-back 2`, on the debug desktop build with an isolated catalogue. The script clicks **Start
-> Choose a folder** (the one thing it answers itself is the Windows folder dialog, which a script
cannot click), waits for the cull, brings two left-out photos back, presses **Edit the N best
photos**, waits for the edit, then checks the result through the app's own commands and renders a
before and an after of every edited photo. Machine: the photographer's laptop (i7-8700, 6 cores /
12 threads), files read from G:.

## Result: 11 of 11 checks passed

| Check | Result |
|---|---|
| Every file imported | 100 of 100 |
| The cull ran on its own and covers every photo | 100 culled: 68 best, 32 left out (16 duplicates in a burst, 16 weaker shots of a moment), balanced strictness |
| Nothing edited before confirming | yes |
| Photos brought back join the best set | 2 brought back -> 70 to edit |
| The window said which stage was running | yes, stage by stage |
| Every confirmed photo edited | 70 of 70, one at a time |
| Every edited photo stores its ten-stage report | 70 of 70 |
| Every edited photo renders differently from its original | 70 of 70 (mean pixel change 1.1 to 32.7, median 5.5 of 255) |
| Left-out photos untouched | 30 of 30 |
| A second full edit changes no setting | yes (only report text differs) |
| Page errors | none |

Time: import and cull of the 100 files **60 s**; the full edit of 70 photos **27 min 56 s, 23.9 s a
photo**. No hang, no stall, no photo set aside.

## What each stage did, over the 70 photos

| Stage | Applied | Nothing needed | Does not apply |
|---|---|---|---|
| 1 Basic edit | 70 | | |
| 2 Colour correction | 65 | 5 | |
| 3 Calibration | 49 | 21 | |
| 4 Colour grading | 63 | 7 | |
| 5 Crop & straighten | 30 | 40 | |
| 6 Blemishes | 24 | 41 | 5 |
| 7 Skin smoothing | 65 | | 5 |
| 8 Face | 36 | 29 | 5 |
| 9 Body | 64 | 1 | 5 |
| 10 Advanced edit | 70 | | |

"Does not apply" is the five frames where no face was found - people bowing or turned away, and
close-ups of hands.

Checked by eye on the before/after sheet: exposure evened across the stage and garden frames;
blown faces under the stage lights brought back (5I7A8910, -0.70 EV); frames shot 3 to 5 degrees
off level straightened with a 9 to 11% crop (5I7A8902, 8932, 8934); skin evened with pores kept;
saturated reds of the saris and the stage kept. Every result was reviewed at screen size, not at
full resolution.

## What the testing found and fixed

1. **The app froze after importing 100 CR2 files**, and once had to be force-closed. A hundred
   thumbnails decoded at once (gigabytes of memory, every core busy); each decode then missed its
   5 s deadline and the photo was set aside as unreadable - 40 of them within six seconds.
   Fixed with decode slots and deadlines that start when a decode starts (ADR-0187 section 6).
2. **The whole window locked while the Cull ran**, because the hidden photo editor reported itself
   busy while waiting for its first preview. Now only a save holds the app.
3. **Calibration read a green garland as a green cast** in every photo. Neutral pixels are now
   judged in light, not bytes, and the median is used.
4. **Saved retouch choices were ignored** by the first version of the full edit; Finish Folder's
   test caught it. They now win, as in Auto enhance.
5. The report said "No person in the photograph" for people with their faces turned away; it now
   says no face was found.

## What this does not show

* Export was not tested: this machine's trial licence has expired, so the window refuses to export
  (`AURA-REL-12005`). Every result here is the app's own render at 1200 px.
* Nothing measures whether a photographer would choose the same 68 photos or prefer these edits;
  the cull's limits are in ADR-0185 section 5.
* One wedding, one camera. Other cameras, lighting and skin tones are untested by this run.

Evidence: `I:/aura-full-edit/gui100/` (`report.json`, `contact-sheet.jpg`, `before/`, `after/`,
screenshots), an earlier run of the same test before fix 4 in `I:/aura-full-edit/gui100-run1/`.
