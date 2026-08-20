# The skin fairness statement

*What AURA does about skin colour, how it is measured, and what has not been proven.*

This document exists because phase 15 makes a claim with consequences outside the product,
and a claim like that has to be falsifiable and has to say where it stops.

## The claim

**AURA has no target for what skin should look like, and cannot acquire one without a schema
change and an ADR.**

Not "AURA tries not to be biased". Not "we tested it and it seems fine". The specific, checkable
claim is that there is nowhere in the product to put an ideal skin value:

- no field in the frozen contract (`crates/aura-core/src/contract/tone.rs`);
- no column in the catalog (`crates/aura-catalog/migrations/0015_tone.sql`);
- no key in the settings a release can change
  (`crates/aura-brain-photo/config/exposure_targets.toml`);
- and no skin-tone category anywhere in the product at all.

`aura-cli verify --phase 15` scans the schema for one on every run and fails the build if one
appears.

## Why that specific claim

A white balance has a free parameter, and skin is the most visible surface it acts on.
Whatever target you give a colour solver becomes a target it moves skin toward.

If that target is a constant, the constant is somebody's skin. Everybody else's skin is then
corrected *away from itself and toward that person's* — and the further away it started, the
more it moves. That is what skin lightening looks like when it is implemented by an engineer
who believes they are removing a colour cast. It does not require anybody to intend it, and it
is invisible in testing if the people testing it are the people the constant came from.

The defence is not vigilance. It is that the code has no constant to compare a person against.

## What it does instead

For each person AURA recognises, it accumulates how their skin actually appears across the
frames of **this wedding** where the light was easiest to read, and fits a small region in a
perceptual colour space from those samples. Every candidate white balance is then required to
leave that person's skin inside their own region. A candidate that does not is rejected in the
solve — a hard constraint, not an adjustment applied afterwards.

Three guards keep a weak measurement from being worse than none:

| Guard | Value | Why |
|---|---|---|
| Minimum samples before a region exists | 5 frames | A region fitted to two frames looks like evidence and is noise |
| Only well-solved frames contribute | confidence >= 0.70, face quality >= 0.45 | Otherwise the solver accumulates its own mistakes and then agrees with them |
| The region's size is bounded at both ends | 0.012 to 0.070 | Too tight rejects every honest answer under a second light; too loose constrains nothing |

Below the minimum, there is no region and the product **says so** rather than falling back to
an assumption: `AURA-ML-5065`, a sentence in the panel, and a project-level figure for how much
of the wedding was decided without one.

## How it is measured

Two numbers, not one, because a solver can be uniformly mediocre or selectively good and only
the second is a fairness failure:

| Measure | Gate | What it catches |
|---|---|---|
| Mean skin error across all samples | <= 3.0 dE00 | A solver that is bad at colour |
| Spread between the best and worst tone group | <= 1.0 dE00 | A solver that is good at colour *for some people* |

The measurement groups by skin-tone bucket, because measuring a disparity requires the
grouping. **Those buckets exist only in the test harness.** They are in
`tests/eval/tone_eval.rs` and in `ml/models/tone/eval_tone.py`, they are computed from
synthetic reflectances, and nothing about them ever reaches the catalog, the wire or a log
line — because shipping the grouping into a product database is how a fairness measurement
becomes a demographic record. Phase 06 wrote that rule for faces; this is the same rule.

The evaluation harness refuses to report a fairness pass from a single populated bucket. "We
only had one group" is not a pass.

### What this build measures

On the synthetic wedding in `tone::fixtures` — five reflectances spanning light to dark, each
photographed under two lights, with the illuminant and the subject luminance painted into the
pixels and read back through the real pipeline:

```
mean 0.110 dE00 (gate 3.0), spread 0.159 across 5 tone buckets (gate 1.0)
```

## What has **not** been proven

This is the part that matters most, and it is the part a document like this usually leaves out.

**The numbers above are about a mechanism, not about photographs of people.** The five
reflectances are five points on a line through the region human skin occupies. They are not
five people, they are not sampled from anybody, and passing a spread test on them proves that
the arithmetic does not have a lightness-dependent term in it. It does not prove that AURA is
fair to a real person in a real reception.

What would prove that, and what does not exist in this repository:

1. **RAW files from real weddings** with expert edits, across the eight lighting classes and
   across skin tones. Section 9 of the phase document budgets ten days of a data engineer's
   time for it. There are no camera files here at all.
2. **A photographed colour reference** so that "correct" means correct against a chart rather
   than consistent with itself. Every dE00 figure the product computes is a measure of
   *consistency across one wedding*, which is what it can compute honestly without one.
3. **A blind expert review** across skin tones, catalogued for systematic bias — section 9's
   QAIQ deliverable, 600 frames. It has not been done.

Until those exist, **no claim about fairness on real photographs should be made on AURA's
behalf**, including by AURA. This is condition C1 in `docs/progress/PHASE-15-EXIT.md` and it is
a Sev 2 trigger.

## The second promise: grading never moves anybody's colour

Everything above is about the *colour of the light*. AURA also grades - contrast, a tone
curve, and per-colour adjustments that pull foliage back from yellow-green or calm down a
fluorescent exit sign. Those adjustments are the ones that, in every product that has shipped
them, eventually turn somebody's skin orange.

**So the second promise is a limit with a number on it.** After AURA has finished grading a
photograph, it looks at the skin in that photograph again and measures how far the grading
moved it:

- at most **2 degrees** of hue, which is below the point where the same face side by side
  reads as a different colour;
- at most **6 %** of colour intensity.

If a grade moves skin further than that, AURA works the colour out again more gently. If even
the gentlest version still moves it too far, **AURA drops the colour adjustments entirely** and
says so - the photograph keeps its contrast and its curve, and its greenery stays as the camera
recorded it. That is the trade the product makes on purpose: slightly flatter decor beats
skin that has moved.

### What it is measured against

**This photograph's own skin, before the colour adjustments.** Not a target, not an ideal, not
another photograph. The limit is on how far grading *moved* it, which is the only definition
under which the promise means exactly the same thing for everybody in the frame and everybody
in the wedding.

Making a photograph lighter does change skin's measured colour intensity - that is what
lightening is - so the measurement starts after the contrast and the curve and covers only the
colour adjustments. That is the honest boundary rather than a convenient one: a limit that
fired on every correctly brightened photograph would be a limit nobody could act on.

### Where the number is

On the photograph. The Tone panel says "skin moved 0.4 degrees of hue and 1% of colour. AURA's
limit is 2 degrees and 6%", and the project header carries the **largest movement anywhere in
the wedding** - because a wedding whose worst frame moved skin two and a half degrees has
broken the promise however good its average is.

When there is nobody in a photograph, the panel says *that*, rather than showing a perfect
score. A photograph of the rings has no skin to protect and no measurement to report, and
those are different things.

### What this measurement is not

The same caution as above, and it is worth repeating because the number looks harder than it
is. The evaluation runs the guarantee across **five skin reflectances spanning light to dark**
and checks two things: that every one of them stays inside both limits, and that AURA does not
have to work *harder* on one than on another - a product whose protection strains on dark skin
is treating it as a special case even when every individual frame passes.

Five reflectances are five points on a line through the region human skin occupies. They are
not five people. Until this has been measured on photographs of real people with their
consent, the honest statement is that **the mechanism is self-referential and per-frame, and
that says nothing yet about a photograph of you.** That is condition C2 in
`docs/progress/PHASE-16-EXIT.md`.

Nothing in the product stores a skin-tone group. The five buckets exist only in the evaluation
code, because measuring a disparity needs the grouping and shipping the grouping into a
catalog is how a measurement becomes a record about people.

## What is structural rather than promised

Three properties hold regardless of what any model learns later, because they are properties of
the shapes rather than of the weights:

- **There is no ideal-skin value and nowhere to put one.** Checked by the gate.
- **The skin constraint is in the solve, not after it.** A candidate that would put somebody's
  skin outside their own region loses to one that would not, whatever its other evidence said.
- **A missing reference is visible, never silent.** The count of people with a usable region is
  stored per frame and reported per project, so "AURA had nothing to check this against" is a
  number somebody can look at rather than a state nobody notices.
- **The grading limit is measured after the fact, on the pixels.** Not derived from the
  settings and not promised by the arithmetic: AURA grades the skin, looks at what happened,
  and works it out again if it moved too far. Every product that has shipped orange skin
  promised it in the settings instead.

## What we will never build

Skin lightening or smoothing that changes a person's tone, body reshaping, face or eye
swapping, and any operation that changes who somebody is. This is a product decision recorded
in `docs/plan/CLAUDE.md` section 11, enforced by guard clauses and CI tests, and it does not
have an exception for a customer who asks nicely.

---

*See also: [Mixed lighting](mixed-lighting.md), [Tone and colour](tone-and-colour.md),
[ADR-0031](adr/ADR-0031-exposure-white-balance-and-skin.md) section 4,
[ADR-0033](adr/ADR-0033-tone-curves-hsl-and-skin-protection.md) decision 1,
`docs/model-cards/white_balance.md` and `docs/model-cards/tone_model.md`.*
