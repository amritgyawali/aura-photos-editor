# ADR-0183: Exact colours give skin the reference's skin, and are the panel's default goal

Date: 2026-10-27. Builds on ADR-0182 (the reference's exact colours). No frozen contract changes and
no migration: the stored `reference_grade_v1` is unchanged; the stored reference colours
(`colour_reference_exact_v1:`) gain an optional field and version 2; `Allowance` gains an optional
field that is omitted when false.

## Context

A photographer matched a bride in a red sari, shot in daylight against red and white curtains
(`989A2746.CR3`), to `warm-cinematic.jpg`, a low-key portrait in amber lamplight, and saw almost no
change: "it is not matching anything. Make it match skin colour, environment colour, background
colour, the same exact look." The panel had opened on Natural look, which by design keeps a
different scene's own colours. Run through every goal on the native pipeline the photograph barely
moved under any of them:

| goal | distance to the reference (engine's measure) | share of the fit kept by the native checks |
|---|---:|---:|
| untouched | 6.06 | - |
| Natural | 4.86 | 87.5 % |
| Palette | 4.30 | 37.5 % |
| Exact | 4.85 | 28 % |

With the native checks bypassed, Exact at 100 % already gave the amber, low-key look. Only one
check refused it: the skin guard. ADR-0182 lets Exact move skin twice the look limits
(`EXACT_SKIN = 2.0`) measured from the photograph's own skin; giving a daylight face the
reference's lamplit skin is a move of 7.3 on that scale, so the fit was cut back until the face
barely changed, and the rest of the look with it.

At 100 % the face was also yellower and duller than the reference's (cheek hue about 51 degrees
against the reference's lit skin at about 37-49, chroma 0.12 against 0.14-0.18). Exact recognised
skin by its colour (`skin_likeness`), and in this reference 38 % of all pixels are skin-coloured:
the amber walls and a yellow curtain. The face was sent to their colours.

## Decision

**Skin may move toward the reference's skin.** For the Exact goal the native skin check accepts a
skin change beyond the allowance when every person's skin in the result is closer to the
reference's measured skin than it was, on the check's own scale (the largest of shadow, mid and
highlight colour, colourfulness and median tone; `appearance::skin_toward`). Each face is measured
against the reference's face skin and each body against its body skin, or against whichever skin
the reference has: a first version compared region names one for one and never applied when the
reference measured a body the photograph did not (989A2704 and a group photograph stayed at 19-28 %
of the look). A person who moves away from the reference's skin is still held to the allowance, a
reference without measured skin gives nothing to move toward, and every other check (bands,
clipping, coming closer to the reference) is unchanged. Where full strength would carry a face past
the reference's skin, the existing strength search keeps the strongest share that does not. The
rule is a flag on `Allowance` (`toward_reference`), so Apply checks a strength by the limits
analysis used.

**Skin is the skin the people detector found**, in a photograph whose people show skin colour. A reference's detected face and body skin is kept
at import (each 1/128 region cell read at sixteen places, up to 8,192 colours; stored version 2).
The photograph's detected skin is read the same way from its fitting render. When both show people,
skin goes to the reference's detected skin as one population, rather than per lightness band to
whatever is skin-coloured. Only colours like the photograph's detected skin take that map: full
within 2.5 standard deviations of its colour in the colour plane, none beyond 4, smoothly between,
so a pink curtain or an orange sari keeps the scene's map and no edge appears. When the detector
finds no one, or the people it finds are grey (a black-and-white portrait), skin is recognised by
its colour exactly as before: deciding "people" by detection there sent a grey studio portrait's
whole scene to the reference's non-skin colours, which tore its gradients (refused for bands, 28 %
of the look kept fell to 6 %), and sent a scene without people from the colour-based rule to
"everything to everything", which clipped more (100 % kept fell to 59 %). A reference imported
before version 2 keeps the colour-based rule until it is uploaded again.

**The panel opens on the reference's full look.** The goals are named for what they do: "Full look
of the reference" (Exact, first and the default), "Softer: palette and tone" (Palette) and "Subtle:
keep own colours" (Natural); while a gentler goal is chosen the panel says that it keeps much of the
photo's own colour. A saved match reopens on the goal it was made with. The photographer read the
gentle goals' honest near-no-change on a different scene as Match a look not working.

Driving the panel in the desktop app (`scripts/test-match-a-look-gui.py`) found two more faults,
both fixed:

* **Matching again after "Remove matched treatment" saved a match at 0 %.** The one-click match
  applied the current strengths, and removal leaves the amount at 0, so the photo stayed as it was
  and the panel said no closer match had passed. A one-click match now restores the reference's
  saved amount, or 100 %, when the amount is 0.
* **Removing the match was reported as a match too slight to see**, with advice to pick another
  goal. A removal now says the photo is back to how it looked before the match; the slight-change
  advice points to the full look.

## Measured

`crates/aura-app/tests/match_look_visual.rs` (ignored) imports a photograph into an isolated
catalogue, runs every goal through `prepare` and `apply` at the default strengths and renders the
result. On the bride with `warm-cinematic.jpg`:

| | distance to the reference | skin's distance to the reference's skin (check scale) | fit kept |
|---|---:|---:|---:|
| before this decision | 6.06 -> 4.85 | not measured | 28 % |
| skin may move toward the reference | 6.06 -> 2.77 | 9.6 -> 5.8 | 100 % |
| + detected skin, unbounded | 6.06 -> 3.75 | 9.6 -> 4.9 | 100 % |
| + detected skin limited to colours like it | 6.06 -> **2.15** | 9.6 -> **5.0** | 100 % |

Without the limit the skin map reached every skin-coloured colour and cast the curtains and sari
red; that variant is rejected.

The same harness against the code before this decision, Exact goal at the default strengths.
Distance is independent of the engine: the sliced Wasserstein distance between the result's and
the reference's `OKLab` colours (64 directions, x100), median over photographs.

| set | untouched | before | after | share of the look kept (median) | closer / further (by > 0.2) |
|---|---:|---:|---:|---:|---:|
| 7 wedding CR3s (`H:\AURA EDIT`) x `warm-cinematic` and `vibrant-teal-orange` | 8.55 | 4.43 | **2.27** | 44 % -> 91 % | 10 / 0 |
| ADR-0181's 20 pairs (six edited references; the monochrome pair 18 is refused before and after) | 10.85 | 5.23 | 5.23 | 75 % -> 88 % | 5 / 0 |

With `warm-cinematic` every wedding photograph now keeps 78-100 % of the look (19-44 % before);
989A2698 went from 11.1 to 1.5, the bride from 6.2 to 0.9. The 20-pair set's warm-punch and
warm-matte studio portraits are still held back by their bands and clipping checks, as in ADR-0182.

The desktop app, debug build, isolated catalogue, the bride `989A2746.CR3` and `warm-cinematic.jpg`
(`scripts/test-match-a-look-gui.py`, run `match-a-look-95ca2bbd`; distance as above, untouched
8.14): Full look in one click 0.91 (100 % kept); 50 % 4.17; removed 8.14 (the original); Softer
7.13; Subtle 6.70; Full look again after removal 0.91; `vibrant-teal-orange` 5.66 -> 3.17 (53 %
kept); Quick mode 0.91; synced to `989A2698.CR3` 13.51 -> 1.48. No page errors; the originals'
bytes unchanged.
