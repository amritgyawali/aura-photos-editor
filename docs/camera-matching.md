# Camera matching, in the product's own words

> Your second shooter's files will finally match yours.

Most weddings are photographed by two or three people on different camera brands. Colour science
differs by manufacturer, so identical settings produce different photographs - which is exactly why
matching them by hand eats hours. AURA matches **what the photographs look like** rather than what
the sliders say: the goal is that skin, whites and blacks agree, not that two cameras are set the
same way.

This page is what AURA does about that, when it needs your wedding's own photographs to do it, what
it deliberately will not do, and how to switch it off.

---

## What AURA actually corrects

Each camera at your wedding gets one correction, and it is a **residual**: it moves the answer AURA
already worked out for each photograph rather than replacing it. Nothing about the original file
changes - AURA never writes to a RAW.

| What moves | How far it may ever move | Why |
|---|---|---|
| Colour temperature | 900 K | Twice what AURA will move a single photograph to match the room it was taken in, because a brand difference is systematic and larger than drift within one room. |
| Tint | 20 units | The green-magenta axis is where two manufacturers' colour differs most under fluorescent and LED light, which is most of a modern reception. |
| Exposure | two thirds of a stop | Enough to absorb both a metering difference between two bodies and an exposure habit difference between two people. |
| Individual colour channels | 10 % | The tightest bound in the feature, because this is the one adjustment that can make a photograph look *broken*: pushing red to satisfy a measurement made on somebody's cheek takes the roses, the sari and the exit sign with it. |
| Saturation | 12 units | A saturation move is the one most visible on skin. |
| Contrast shape | 15 % on each of shadows, mids and highlights | The difference between two manufacturers is often a *curve* - how the highlights fall away - rather than an offset. |
| Skin | 0.012 in `u'v'`, 0.04 in brightness | **Deliberately smaller than what AURA will do to skin in a single photograph**, because a camera correction applies to every frame that body shot. An error here is an error four thousand times over. |

**A studio can make every one of those smaller and nobody can make one bigger.** They live in
`crates/aura-brain-gallery/config/camera_match.toml`, they are checked against the frozen contract on
every load, and a file that widens one is refused outright rather than quietly clamped.

---

## Where the correction comes from

This is the part worth understanding, because two cameras corrected by the same amount can rest on
completely different evidence, and the per-camera report always leads with which.

### Best: photographs from your own wedding

AURA looks for **matched pairs** - two photographs, one from each camera, of the same thing under the
same light, taken within ninety seconds of each other during the same part of the day. A ceremony
where both photographers are working the aisle is the ideal source.

It then checks each pair by comparing the **surroundings** rather than the people. That sounds
backwards and is the whole mechanism: two photographs of the same bride's face from two cameras
differ *in exactly the way the feature is trying to measure*, so scoring the pair on her face would
be scoring the thing under test. A wall, a marquee ceiling and a row of chairs were lit by the same
light and are not what either camera was metering for - if they disagree, the two photographs were
not taken in the same conditions and the pair is thrown out.

Twelve verified pairs is what AURA wants before it trusts a correction worked out from your wedding
alone.

### Then: a check against photographs it did not use

A quarter of the verified pairs are **held back** before the correction is worked out, and used only
to check it afterwards. If the correction does not make those unseen pairs match better, it is thrown
away and AURA falls back on what it knows about the brand - and says so.

That check is not decoration. A correction fitted to a handful of photographs can describe those
photographs beautifully and be wrong about the camera, and there is no way to notice that by looking
at the photographs it was fitted on.

### Otherwise: what AURA knows about the brand

When the two cameras never photographed the same thing - one photographer with the bride and one with
the groom, all morning - there is nothing in your wedding to measure from. AURA then applies a general
correction for the manufacturer, and the report says so in those words.

**In this build, those general corrections are not measurements.** They were chosen to be plausible
from published behaviour rather than measured from a photographed colour target, and every file in
`assets/camera_baselines/` says so at the top. Treat a camera matched this way as a starting point.

And when AURA has no measurements for a manufacturer at all, it **changes nothing** rather than
guessing that an unrecognised body behaves like a Canon.

---

## Flash and available light are matched separately

A camera's colour behaviour under a strobe is not the same as its behaviour under a room, and the
difference between two manufacturers is amplified under flash. So every camera at your wedding gets
two corrections rather than one, and a matched pair is never formed between a flash photograph and an
available-light one.

If one of the two halves has too few photographs to measure, that half falls back on the brand and the
report says which.

---

## The second shooter

Two photographers expose differently, consistently, and it is a real part of how somebody works. AURA
measures the difference as a **median offset in the brightness of the subject, per kind of
photograph** - a ceremony and a reception are measured separately, because somebody who works darker
during one may not during the other.

Then it corrects **sixty per cent of it, and never more than a third of a stop**.

That number is a decision rather than a limitation. Correcting all of it produces a gallery that is
perfectly uniform and in which the second photographer has disappeared from their own work.
Correcting none of it produces a gallery where every fourth photograph is a third of a stop darker
than the ones either side. Sixty per cent is where a gallery stops looking like two weddings and a
second shooter can still be picked out of a contact sheet by somebody who knows their work.

The report always tells you what was measured *and* what was applied, so you can see the difference
you are being protected from.

Some kinds of photograph are never treated this way at all, because there the exposure **is** the
photograph: a backlit couple session, a sparkler exit, a first dance under a moving spotlight.

---

## Where this sits in the order of things

Camera matching runs **before** AURA matches a wedding to itself scene by scene. That order matters:
if it ran the other way, each part of the day would be normalised toward the average of two
manufacturers' colour science, which is a look neither camera can produce.

It also means the two work together. A single temperature correction in kelvin cannot be perfect in a
candle-lit room and a daylight one at the same time - the same number of kelvin is a bigger visual
difference in warm light than in cool. What is left over is handled scene by scene afterwards.

---

## Switching it off

Per camera, at any time, and it stays off. AURA re-runs matching whenever it improves the way it
works; a camera you switched off stays off, and a correction you set by hand is never overwritten.

There is no strength slider, and that is deliberate: the bounds above are promises the product makes,
and a slider that could exceed them would make them descriptions of a default instead.

---

## What AURA will not do

- It will not change your original files. Every decision is a row in a database plus an edit recipe.
- It will not move a camera further than the bounds above, whatever the measurement says.
- It will not push skin to a colour skin does not come in. That constraint is measured against each
  person's own photographs, and it stops the correction rather than being weighed against it.
- It will not remove a second photographer's way of working, only reduce the distance to yours.
- It will not send anything anywhere. Camera matching runs entirely on your machine, and works with
  the network cable unplugged.

---

## Every reason AURA can give

Thirty-two, and fifteen of them **withdraw a claim** rather than making one - they are the ones that
say AURA declined to correct, or corrected on less evidence than it wanted. That distinction is the
most valuable thing on this page: a camera corrected by 300 K from twenty pairs of your own ceremony
and a camera corrected by 300 K from a general brand setting are the same arithmetic and completely
different claims.

| Code | Kind | What it means |
|---|---|---|
| `fingerprinted` | reports what happened | AURA measured how this camera renders colour, using this wedding's own photographs |
| `fingerprint_thin` | withdraws a claim | this camera shot only a few photographs here, so AURA is less certain about how it renders colour |
| `fingerprint_absent` | withdraws a claim | this camera shot too few photographs to measure, so AURA has matched it from what it knows about the brand instead |
| `flash_separated` | reports what happened | flash and available-light photographs from this camera were matched separately, because a camera behaves differently under each |
| `flash_population_thin` | withdraws a claim | there were too few photographs of one kind - flash or available light - from this camera, so that half was matched from the brand instead |
| `is_reference` | reports what happened | this is the camera everything else is matched to, so nothing about it was changed |
| `reference_by_shooter` | reports what happened | this is the main photographer's camera |
| `reference_by_frame_count` | reports what happened | this camera shot most of the wedding |
| `reference_by_user` | reports what happened | you chose this camera as the one to match everything else to |
| `pairs_found` | reports what happened | AURA found photographs where both cameras were shooting the same thing under the same light, and matched them from those |
| `pair_background_verified` | reports what happened | the surroundings in both photographs agree, so the two cameras really were in the same light |
| `pair_rejected_background` | withdraws a claim | two photographs looked like a pair but their surroundings did not agree, so AURA did not use them - the light had changed between them |
| `pairs_insufficient` | withdraws a claim | there were not many photographs where both cameras shot the same thing, so AURA leaned partly on what it knows about the brand |
| `pairs_absent` | withdraws a claim | these two cameras never photographed the same thing under the same light, so AURA matched this one from what it knows about the brand |
| `solved_from_pairs` | reports what happened | the correction comes from this wedding's own photographs rather than from a general setting |
| `blended_with_baseline` | withdraws a claim | the correction is part what AURA measured here and part what it knows about the brand, weighted by how much evidence there was |
| `baseline_only` | withdraws a claim | the correction is what AURA knows about this brand, because there was nothing in this wedding to measure from |
| `baseline_unknown_brand` | withdraws a claim | AURA has no measurements for this camera's manufacturer, so it has changed nothing rather than guess |
| `held_out_improved` | reports what happened | AURA checked the correction against photographs it had not used to work it out, and they matched better afterwards |
| `held_out_failed` | withdraws a claim | the correction did not hold up when AURA checked it against photographs it had not used, so it fell back on what it knows about the brand |
| `bounded_by_policy` | withdraws a claim | the correction reached the furthest AURA will move a camera, so it went that far and no further |
| `skin_locus_refused` | withdraws a claim | going further would have pushed skin to a colour skin does not come in, so AURA stopped |
| `skin_matched` | reports what happened | skin from this camera now matches skin from the main camera |
| `white_point_matched` | reports what happened | whites and greys from the two cameras now agree |
| `grade_matched` | reports what happened | colour richness and contrast from this camera now match the main camera's |
| `already_matched` | reports what happened | these two cameras already agreed, so AURA changed nothing |
| `shooter_bias_corrected` | reports what happened | this photographer consistently exposes differently from the main photographer, and AURA has brought them partly into line |
| `shooter_bias_capped` | withdraws a claim | AURA brought this photographer's exposure partly toward the main photographer's and deliberately stopped short, so their own way of working is still visible |
| `shooter_bias_absent` | withdraws a claim | there were not enough photographs of this kind from this photographer to tell whether they expose differently |
| `shooter_style_preserved` | reports what happened | this photographer's exposure is close enough to the main photographer's that AURA left it exactly as it was |
| `disabled` | withdraws a claim | you switched matching off for this camera |
| `user_edited` | reports what happened | you set this camera's correction yourself |

---

## What this build cannot claim

Everything above describes what the feature does. What follows is what has and has not been measured,
because a page that only said the first would be marketing.

- **There are no multi-camera weddings in this repository.** Every quality figure was measured on
  synthetic weddings whose per-brand colour response was chosen, applied to authored readings and
  recovered through the real pipeline. That proves the fingerprinting, the pairing, the background
  verification, the metric, the solver, the bounds, the held-out check and the ordering. It is not
  evidence about a photograph.
- **Every bundled brand baseline was fabricated.** No camera was measured and no colour target was
  photographed. The fallback path is proved to run and to report itself honestly; nothing is proved
  about the numbers it falls back on.
- **The skin figures were measured on a chromaticity, not on a person.** AURA cannot yet read a
  per-person skin region from a photograph in this build, so the skin half of the promise was
  exercised against authored readings. `docs/skin-fairness.md` says the same thing about every other
  place skin is measured.
- **The blind study did not happen.** The question this feature is ultimately judged on - can a
  photographer pick out the second camera after matching - has not been asked of a person. No claim
  about it is made from this build.

`docs/progress/PHASE-26-EXIT.md` carries the full list with severities.
