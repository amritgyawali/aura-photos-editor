# ADR-0092: Acne clear and the blemish brush

Status: accepted
Date: 2026-10-07

## Problem

Run through *Acne only* and *Professional retouch* (ADR-0090, ADR-0091), a real acne
portrait (an Unsplash photograph, 2048 x 3072) still had acne afterwards, and the person
who asked for the work said so:

- **The nose kept every mark.** ADR-0091's nose guard excluded the whole nose from every
  automatic step, mark repair included, and the surface selection had already left the nose
  tip out. The nose was the worst-affected area of the face.
- **Clusters kept most of their marks.** Frequency healing calls a group a mark only when the
  skin on every side of it is clean. Inside a cluster the skin beside a mark is another mark.
- **The forehead between and just above the brows kept its marks.** The orbital guard reaches
  above the brow, and the surface selection cut a band along every brow line.
- **Donor spot repairs left round patches** - a disk of borrowed skin, a little lighter or
  darker than the skin it landed on - once the skin around them had been evened.

The person also asked to be able to see what was retouched and to retouch anything left
themselves.

## Decision

### Acne clear (`Tool::AcneClear`, `aura-render/src/retouch_acne.rs`)

One operation over a skin selection that measures each pixel against *clean skin* rather than
against *the skin next to it*.

1. **Reference.** A local percentile of the selected skin in a window three mark radii wide
   (seven where that window holds under a tenth of selected skin): 60th for luminance, 40th
   for colour, then measured again at the median without the blemish-sized groups the first
   reading flagged. A percentile keeps an edge, so a mark in a shadow is compared with the
   shadow; a group that departs as a whole - a naturally redder nose, the shadowed side of a
   face - is larger than a blemish, is not excluded, and stays its own reference. The
   percentile is measured on a grid by sliding a histogram (`retouch_planes::local_percentile`)
   and softened over a mark radius, so the reference has no steps.
2. **Find.** Departures in three exposure-free directions - darker (log luminance), redder
   (log red over green), browner (log green over blue) - each in units of this selection's
   robust spread. Groups at three tiers. A long band, a group lying along the selection's edge
   that is not a small round spot (skin warms where it turns away from the light), a coloured
   group larger than 45 squared mark radii and a dark-only one larger than 20 are not marks.
   A group that is only darker must be darker than the skin on every side (ADR-0090's
   enclosure test); one that is redder or browner is a mark wherever it is. A small bright
   group ringed by a mark - the white head of a pimple - joins it.
3. **Rebuild.** Tone and colour become the unmarked skin right around the mark (three
   neighbourhoods, then the reference), never brighter or less coloured than the brighter,
   calmer quarter of the skin within two mark radii, and **never darker than the mark was**
   unless it is a ringed bright head, which goes no lower than the local median. Pore detail
   finer than a third of a mark radius stays, its relief limited to 1.5 robust spreads of this
   skin's own pore contrast. Pixels no mark reached keep their exact value.
4. **Even redness** (`preserve_microtexture`). The colour beyond one robust spread of this
   skin's own variation, measured against a four-radius (nine inside a wide blotch) percentile,
   is reduced by 70 %, in proportion, so no patch outline is drawn. A blotch that is also
   darker is lifted by the share of its colour that goes - colour alone would turn a dark red
   mark grey - and skin that is only darker is never lifted. Fades out within four mark radii
   of the selection's edge.

Two passes. Nothing is generated: every value written is this photograph's own skin.

Without a matte the operation is a **brush**: the reference also reads the skin around the
stroke, the threshold is 15 % lower, dark-only spots need no enclosure, and a compact bump
brighter than the skin on every side counts. Hair under a brush - a group more than 45 %
darker than the skin the brush was painted on - must be the size of a mark and enclosed by
skin: the first build of the brush, dabbed beside a brow in the application, faded the end of
the brow.

### Where the automatic pass uses it

- Frequency healing's slot (`frequency_heal` setting, labelled **Acne clear**) now plans
  `AcneClear` over a **heal selection**: the surface selection with the nose tip, bridge and
  sides included and only the nostrils and columella excluded (a capsule 0.11 eye distances
  below the nose landmark), and without the band along the brow line - brow hair is still
  excluded where it actually is.
- ADR-0091's guards apply per tool. Mark repairs (`AcneClear`, `FrequencyHeal`, patch heals)
  and the texture restore are not kept off the nose; smoothing and toning are. Acne clear and
  frequency healing get an orbital guard whose upper half is 0.19 eye distances instead of
  0.30, so the upper lid stays protected and the brow bone can be repaired.
- With acne clear on, no donor spot repairs are planned. The surface finish still runs for
  presets that smooth.
- The texture restore, after acne clear, borrows no donor tiles (`preserve_microtexture` on
  `TextureGraft`) and measures "the texture this skin had" on the skin acne clear left, so the
  photograph's own crusts are not put back.
- *Acne only*, *Deep acne cleanup* and *Professional retouch* turn acne clear on.

### The blemish brush and seeing what was retouched

- **Blemish brush** in the retouch workspace starts an acne clear draft in brush mode with
  live preview. Paint, then *Apply retouch*.
- **Show retouched areas** can show the saved selections (teal, ADR-0091) or the pixels the
  retouch changed (orange), computed in the window from the before and after previews.

## What was tried and removed

- **A re-weighted mean as the reference.** Shadowed skin is darker than the mean, so it was
  weighted out of its own reference and the whole shadow side of the nose became one giant
  "mark" that was then rejected as too large - with every nose mark inside it.
- **Excluding everything the first reading flagged.** A naturally redder nose was flagged as a
  whole, excluded, and compared with the cheeks.
- **Rebuilding from the nearest clean skin at any distance.** In a shadow dense with marks the
  nearest clean skin was a lit cheek away: pale patches.
- **Replacing a coloured mark's luminance.** Marks no darker than the skin were darkened to
  the shadow beside them: dark disks.
- **A thresholded, component-based redness evening, then one with a lift relative to a wide
  mean.** The first drew patch outlines; the second lifted the warm shadow along the face's
  edge into a pale band.

## Limits

- Every threshold was set on two portraits - one generated, one a stock photograph - and on
  painted fixtures. Nobody has compared a result with a retoucher's, and there is no study
  across skin tones on real people.
- Marks inside the orbital guard (the nose bridge beside the inner corner of an eye, the
  under-eye) are left for the brush.
- A blotch wider than about nine mark radii is evened in colour only partly.
- Acne scarring - pitted texture - is relief, not tone, and is not removed.
- Marks are found on the pixels rendered; a preview and a full-size export can differ in which
  faint marks are rebuilt.

## Validation

- `crates/aura-render/tests/acne_clear.rs`: a dense cluster loses over 90 % of its redness;
  isolated red and brown marks are rebuilt to within 20 % on three skin tones at 0.1x, 1x and
  2.5x exposure; the crease, the shadow and its edge keep their exact values; a dark mark is
  kept when asked; nothing outside the selection moves; a brush clears a bump the automatic
  pass leaves; pores under a mark survive; evening reduces a blotch's redness without
  darkening it. `retouch_planes` checks that a local percentile keeps an edge and ignores a
  spot.
- `portrait_features` tests: the heal selection includes the nose and excludes the nostrils;
  repairs reach the nose and the brow bone and never the eyes.
- `crates/aura-app/tests/auto_retouch_photos.rs` (presets `acne_only` and `pro`) on both
  portraits at preview size and the stock photograph at full size, looked at at 100 % and
  200 %. `docs/acne-clear-validation.md` records what was run in the application.
