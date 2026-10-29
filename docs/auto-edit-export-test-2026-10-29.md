# The full automatic workflow, with export, on 100 real photographs, 2026-10-29

What was tested: the whole workflow of ADR-0187 - import, automatic cull, the photographer's
confirmation, the ten-stage full edit on every confirmed photo **and the export of the finished
photos** - only by running the AURA app and driving its window, on the folder `G:\auto edit`: 100
Canon CR3 raw files (3.1 GB, 6720 x 4480) of a Nepali wedding - bridal portraits indoors, the
ceremony outdoors, guests and family groups on a stage.

How: `scripts/test-full-auto-gui.py "G:/auto edit" --out I:/aura-auto-edit/run100 --bring-back 2`
on the debug desktop build, isolated catalogue and its own licence folder (`AURA_LICENCE_DIR`), so a
new installation's trial applies and this machine's expired trial is not touched. After the edit the
script presses **Render final output -> Choose output folder -> Export** with the screen's defaults
(the photos just edited, the gallery set: full size JPEG, quality 92, sRGB).

## Result: 18 of 18 checks passed

| Check | Result |
|---|---|
| Every file imported | 100 of 100 |
| The cull ran on its own and covers every photo | 85 best, 15 left out (14 weaker shots of a moment, 1 burst duplicate) |
| Nothing edited before confirming | yes |
| Photos brought back join the best set | 2 -> 87 to edit |
| The window said which stage was running | 9 stage names seen while polling |
| Every confirmed photo edited | 87 of 87 |
| Every edited photo stores its ten-stage report | 87 of 87 |
| Every edited photo renders differently from its original | 87 of 87 (mean change 1.0 to 35.1, median 2.8 of 255) |
| Left-out photos untouched | 13 of 13 |
| A second full edit changes no setting | yes |
| Export offers the edited photos, not the left-out ones | "Render the 87 finished photos ...; the 13 photos the cull left out are not exported." |
| Export finished without an error | yes |
| One file written and read back per edited photo | 87 written, 87 verified |
| Every edited photo in the folder, no left-out photo | 87 files |
| Every file opens | 87 of 87 |
| Every file is full size | 6720 x 4480, or slightly less where straightening cropped |
| Every file matches the edit the app shows | mean difference 1.3 of 255, worst 2.5 |
| Page errors | none |

Time: import and cull **130 s**; full edit **38 min 12 s, 26.3 s a photo** (median 20.9 s, slowest
124.7 s); export **28 min 5 s, 19.4 s a photo**; 524 MB written. No hang, no photo set aside.

## What each stage did, over the 87 photos

| Stage | Applied | Nothing needed | Does not apply |
|---|---|---|---|
| 1 Basic edit | 87 | | |
| 2 Colour correction | 50 | 37 | |
| 3 Calibration | 14 | 73 | |
| 4 Colour grading | 35 | 52 | |
| 5 Crop & straighten | 26 | 61 | |
| 6 Blemishes | 75 | 10 | 2 |
| 7 Skin smoothing | 85 | | 2 |
| 8 Face | 74 | 11 | 2 |
| 9 Body | 78 | 7 | 2 |
| 10 Advanced edit | 87 | | |

## Found and fixed

**Export delivered the cull's rejects.** The Export screen rendered every imported photo, so after
a cull it would have exported the 13 left-out frames beside the 87 finished ones. It now chooses
the photos just edited (or, opened later, the cull's best set) under "Which photos", with "All N
photos in the collection" one click away (`finishedPhotos`, `DeliveryPanel.tsx`; ADR-0187 section 4).

## Looked at by eye

Every exported file was compared, at contact-sheet size, with its original: bridal portraits
brighter and warmer with skin evened and the reds kept; tilted stage groups levelled. One outdoor
ceremony frame against a bright sky (989A8249, the largest change) recovered the sky but left the
people darker than a photographer would likely choose. Nothing was reviewed at 100%.

## What this does not show

* Whether a photographer would pick the same 85 photos or prefer these edits.
* One wedding, one camera body; other cameras and lighting are not covered by this run.
* Export ran on a fresh installation's trial; a licensed installation takes the same path.

Evidence: `I:/aura-auto-edit/run100/` (`report.json`, `final-photos/gallery/` - the 87 exported
JPEGs and `aura-delivery-manifest.json`, `before/`, `after/`, `contact-sheet.jpg`, screenshots).
