# ADR-0045 - Micro-retouch: reduction rather than removal, measured naturalness, and the one thing a borrow may do

**Status:** accepted · **Date:** 2026-08-20 · **Phase:** 21 · **Supersedes:** nothing

Phase 21 section 4 asks for no ADR by name. It needs two anyway, and this is the first. Section
5 freezes a contract with three types it does not define; section 6.2 asks for a colour locus
that would be a fairness hazard if it were read the obvious way; section 6.3 asks for
cross-frame borrowing, which is the first time this product composites two photographs and
therefore the first time it can produce a delivered image of a moment that did not happen; and
the three detectors section 4 asks for cannot be trained in this repository. The second document
is [ADR-0046](ADR-0046-micro-ipc-surface.md), which covers the wire.

The ADR numbering in this repository is sequential across the whole project rather than aligned
to phase numbers.

## 1. Context

Phase 20 changed what somebody's **skin** looks like and spent most of its design budget on one
guarantee: the skin keeps its own texture, measured through the renderer. Phase 21 is the rest
of the retoucher's small list — hair, teeth, eyes, clothing, glasses glare — and it is a harder
ethical problem for a reason section 1 states plainly:

> They are also where automation most easily looks creepy - whitened teeth, glowing eyes, erased
> hair. Doing them conservatively and identity-aware is the differentiator, not doing them harder.

Three things separate this phase from its predecessor.

**Every operation here is on a feature of a person rather than on a defect.** A blemish is a
thing that was not there last week. Teeth, eyes and hair are permanent. Phase 20's central move —
protect what is permanent, remove what is temporary — has no equivalent here, because everything
this phase touches is permanent. What replaces it is a ceiling on *how much*, and the ceiling has
to be enforced on the pixel rather than on the parameter.

**The failure is invisible at the size a photographer reviews at.** A face at 12 % of frame in a
gallery grid looks fine with fluorescent teeth. The failure shows in the album, at print size,
after delivery. So the gates are measured on the rendered pixels at the region's own scale and
not on the parameter that was solved.

**One operation composites two photographs.** Nothing in the previous twenty phases did that.
Section 6.3 is careful about it and section 2.2 excludes the version everybody asks for first
(borrowing an open eye into a group frame). The rule that makes the distinction principled is in
section 4 of this document.

## 2. Decision: five spellings differ from section 5, and here is each one

Hard rule 8 says frozen contracts are copied in verbatim. Five cannot be, and each substitution
is toward something this workspace already froze or toward something section 5 leaves undefined.

1. **`Box2` is `aura_core::contract::composition::Box2`**, which is already an alias of phase
   09's `CropRect`. Phases 11 and 20 made the same substitution and this inherits it.

2. **`ClothingIssue` is defined here**, because section 5 names the type and never gives it.
   Five variants, taken from section 2.1's own list: `Lint`, `Thread`, `Stain`, `Strap`,
   `Crease`. The last two are the two that section 2.1 marks opt-in, and they are variants rather
   than a flag so that the opt-in matrix is keyed by the same closed set the operator emits.

3. **`GlareMethod` is defined here** as `Reduce` or `BorrowFrom(ImageId)`, which is section 5's
   own comment turned into a type. It is a two-variant enum rather than a nullable field because
   a borrow that lost its source id is an undisclosed composite, and there must be no state in
   which one can exist.

4. **`ColourLocus` is defined here, and it is relative rather than absolute.** See section 3.

5. **`NaturalnessGuard` gains nothing and loses nothing, but `NaturalnessReport` is added.**
   Section 5 freezes the guard's thresholds and says nothing about what the guard *found*. A
   phase whose headline KPI is "judged natural >= 95 %" needs the measurement stored, for the
   reason phase 16 gave and phase 20 repeated: a guarantee a product can only assert is a
   guarantee it has no way to discover it has stopped keeping.

## 3. Decision: the teeth locus is relative to the frame, and there is no absolute target anywhere

Section 6.2 asks for yellow reduction "toward a *natural* locus derived from real teeth
measurements". Read as an absolute chromaticity that is a fairness and a colour-science hazard at
once, and the two failures compound:

- **Colour science.** An absolute target fights the white balance. Under warm tungsten every
  set of teeth in the room is genuinely yellow-ish, correctly, because the light is. Pulling
  them to a daylight-measured locus produces teeth that are neutral in a scene where nothing else
  is, which reads as a cut-out.
- **Fairness.** Any absolute per-feature target is one edit away from being a per-person target,
  and phase 15 wrote the rule this product follows: *a target is measured, never assumed — and
  the schema cannot express an alternative.*

So `ColourLocus` is a bounded region in CIE `u'v'` **relative to the frame's own measured
neutral**, which phase 15's `ToneService` already produces, and the operator is a bounded
*reduction of distance to it* rather than a move toward its centre:

- a chromaticity already inside the locus is not moved at all;
- one outside it is moved toward the boundary by at most `MAX_TEETH_YELLOW` of its excess;
- the move is refused entirely when the frame has no illuminant estimate, because the locus has
  no origin without one.

The same shape covers the sclera. There is no absolute chromaticity constant anywhere in
`micro_retouch.toml`, in migration 22 or in the contract, and the phase gate scans the schema for
one on every run — the check phase 15 introduced, running for the third time.

The luminance half has the same shape. `teeth_max_luma` is a lift in **stops**, capped by
`MAX_TEETH_LUMA_EV`, and the operator additionally refuses to raise the teeth above the
brightest non-specular skin on that person's own face in that frame. "No fluorescent teeth" is
therefore a comparison against the subject rather than against a number.

## 4. Decision: a borrow may only replace pixels that carry no information

This is the load-bearing rule of the phase, and it is what makes section 2.2's exclusion
principled rather than arbitrary.

Section 6.3 permits borrowing a region from a sibling frame to repair glasses glare. Section 2.2
forbids borrowing an open eye into a frame where it was closed. Both are "take pixels from
another photograph of the same person in the same moment", so a rule that permits one and forbids
the other cannot be about the mechanism. It is about what is underneath:

> **A specular sheet has destroyed the record. A closed eye is the record.**

Where a reflection has blown a region to the sensor's ceiling, the photograph contains no
information about what is behind it; substituting a few square millimetres from four hundred
milliseconds earlier changes nothing that was ever recorded. Where the eye is closed, the
photograph *is* of somebody with their eye closed, and replacing it produces an image of a moment
that did not happen.

Four bounds enforce it, and the first is the one this section argues for:

1. `MIN_SPECULAR_FRACTION` of the borrow region must be blown specular **in the target frame**.
   Below that, the conservative highlight reduction runs instead and the plan says so.
2. `MAX_BORROW_AREA` caps the region at a small fraction of the frame. A borrow cannot move a
   head, a hand or an expression.
3. `MIN_ALIGNMENT` is a floor on the normalised cross-correlation of the aligned sibling region
   against the ring of unblown pixels around the target region. Below it the borrow is refused.
4. Every borrow names its source in the operation, in the catalog row, in the Explain panel and
   in the delivery report. `GlareMethod::BorrowFrom` carries an `ImageId` in the type, so there
   is no representable undisclosed borrow.

`docs/retouch-ethics.md` section 3 says the same thing to a photographer.

## 5. Decision: the naturalness guarantee is measured through the renderer, per family

Phase 16 established the pattern for skin colour and phase 20 repeated it for skin texture: a
guarantee about a pixel is enforced on the pixel, by applying the plan through the *real*
renderer and measuring. Phase 21 inherits it and measures three things:

| Measurement | What it holds | Section 10.1 line |
|---|---|---|
| `catchlight_ratio` | peak luminance inside the iris after / before | "catchlights preserved (specular pixel test)" |
| `hair_energy_ratio` | edge energy in the hair region after / before | "no bald patches or hairline damage" |
| `teeth_excursion` | largest `u'v'` distance any teeth pixel ended outside the locus | "luminance and chroma stay inside the natural locus" |

**Where phase 21 departs from phase 20 is what happens when a floor is missed.** Phase 20
withdraws the *whole plan*, because skin texture is one measurement over one region and a
partially withdrawn retouch would leave the measurement describing something other than what
shipped. Here the three measurements are over three disjoint regions and map to three disjoint
operation families, so the guard withdraws **the family that failed** and keeps the rest — the
same re-solve at three-quarters strength up to three times first. A frame whose teeth could not
be evened safely still gets its lint removed, and the report names which family was withdrawn.

The rule that does not change: a floor that can be exceeded once is not a floor. Every one of the
three is a hard refusal after the re-solves, never an attenuation.

## 6. Decision: the three detectors are untrained placeholders, and unlike phase 20 they are not replaced by a measurement everywhere

Section 4 asks for `train_flyaway.py`, `train_glare.py` and `train_lint.py`. There is no labelled
corpus of flyaways, glare sheets or lint in this repository, no consented wedding photographs and
no GPU backend, so the three heads ship as signed placeholders and none is consulted:
`FLYAWAY_HEAD_TRAINED`, `GLARE_HEAD_TRAINED` and `LINT_HEAD_TRAINED` are all false.

Phase 20 argued that a *measured* detector should run in place of a refused head, because a
retoucher that consulted nothing would find no marks at all. That argument holds for two of the
three here and not for the third:

- **Glare is a measurement, not a prediction.** A specular sheet is a connected region of
  near-clipped, near-neutral pixels over the eye region. The measurement is the definition, and
  the placeholder head would add nothing to it. It runs.
- **Lint and threads are a measurement.** A small high-frequency anomaly inside the clothing mask
  whose colour differs from the fabric around it. Same shape as phase 20's blemish detector, one
  region up. It runs.
- **Flyaway detection runs, and is deliberately the most conservative of the three.** A thin
  high-contrast structure outside the hair alpha but connected to it, over a background whose own
  detail is below a floor. The background gate is what makes it safe without a learned model: a
  measurement cannot tell a strand from a twig, so where the background is busy the operation is
  skipped rather than guessed.

The consequence, stated once and plainly: **every number in this phase's gates is measured
against synthetic frames whose flyaways, glare sheets, lint and teeth were painted into the
pixels and read back through the real detectors, operators and renderer.** That proves the
arithmetic. It says nothing about a wedding photograph. It is condition C1 of the exit report and
a Sev 2 trigger.

## 7. Decision: the mask port is this phase's own, and it does not extend phase 19's

Phase 19 froze `aura_core::contract::local::MaskField` with a six-kind vocabulary — face,
subject, background, skin, hair, sky — and phase 20 consumed it for skin. Phase 21 needs teeth,
sclera, iris, clothing and dress as well, and there are three ways to get them:

1. **Widen `local::MaskKind`.** Refused. `LocalOutline::gated_histogram` is `[u32; MaskKind::COUNT]`
   inside a frozen contract, so adding a variant changes phase 19's stored shape and its wire
   format for a reason that has nothing to do with phase 19.
2. **Depend on `aura-vision` and use phase 18's real twenty-class `MaskKind`.** Tempting, and it
   is the *first* answer rather than a copy of one. Refused because it would put two mask idioms
   in one crate: phase 20's skin arrives as a `MaskField` and phase 21's teeth would arrive as an
   `aura_vision::Mask`, and the next person to write a solver in `aura-retouch` would have to know
   which. It also drags `aura-index` and `aura-infer` into a decision crate that needs neither.
3. **Freeze `MicroField` with `MicroRegion`, a view onto phase 18's vocabulary.** Chosen.

`MicroRegion::as_mask_str` is a **total** mapping onto phase 18's spellings, the same shape
`local::MaskKind::as_recipe_str` uses, so this is a projection of one vocabulary rather than a
second one. The gating arithmetic is not duplicated in spirit: `MicroField::strength_scale` reads
`local::MIN_MASK_CONFIDENCE` and `local::FULL_MASK_CONFIDENCE` — phase 19's constants, which are
the actual decision — and applies the same three-line ramp. One answer about how much a doubtful
mask may do, expressed twice in three lines each, is the trade this makes; `crates/aura-core/tests/micro_contract.rs`
asserts the two agree at the boundaries so a change to one that did not move the other fails the
build.

## 8. Decision: the budget is phase 19's, for the third time

`LocalOp::PRIORITY` gave phase 19 six operations, phase 20 added a seventh and this phase adds
five more. All twelve spend against `local::PERCEPTUAL_BUDGET`, the shared per-image perceptual
allowance, and none of them gets its own. Phase 19's rule, unchanged:

> Six individually defensible adjustments are how a gallery quietly starts looking processed.

What is given up when the allowance is short is decided by an order that puts the micro
operations *below* everything phases 19 and 20 do, for a simple reason: a photographer would
notice an unlit face before they noticed a lint. Within the five, glare is first — it is the only
one repairing damage rather than polishing — and crease is last.

## 9. Consequences

- **`MicroService` is the seventeenth service of its kind**, and no phase may keep its own
  flyaway detector, its own teeth locus or its own idea of what a borrow is. Phase 22 restores
  and sharpens, phase 24 removes objects, phase 25 normalises a gallery of these decisions and
  phase 27 has to be able to say why a face looks worked on.
- **Nothing in this phase can produce an undisclosed composite**, because the only borrowing
  operation carries its source in the type.
- **Nothing in this phase can change geometry**, because no operation in the frozen contract has
  a field that could express a displacement, and `crates/aura-core/tests/micro_contract.rs`
  asserts it.
- **The three untrained heads are visible rather than silent**: every plan carries
  `MicroCode::HeadUntrained` and the confidence is reduced by it, so nothing downstream can read
  this output as learned.
- **Phase 20's rule survives**: this phase does not re-smooth skin. There is no skin operator
  here, `MicroRegion::Skin` is read as evidence and never written, and the guard refuses a plan
  that carries an operation over a region phase 20 already worked.

## 10. What was considered and rejected

**A single `naturalness` score instead of three measurements.** Rejected for phase 18's reason:
two numbers that fail differently and are fixed by different things must not be collapsed. A
photographer whose complaint is "her teeth look odd" and one whose complaint is "the hairline
looks chewed" need to be able to find out which of the three is low.

**Letting a studio raise a ceiling.** Rejected. A studio can switch any operation off and can
lower any ceiling; the loader refuses a file that raises one above the contract's own bound. A
promise a text file can retract is not a promise — phase 20's words, and the same loader shape.

**Borrowing for closed eyes behind a feature flag.** Rejected outright rather than deferred. A
flag is a default waiting to be changed, and section 2.2 calls the exclusion a product-ethics
decision rather than a scope note.

**Making crease removal a strength of zero rather than an off switch.** Rejected. Off and
zero-strength are different answers in a delivery report, which is the same argument
`RetouchPreset::Off` won in phase 20.

## 11. Amendments made during implementation

Four decisions were taken after the contract was frozen and before the phase landed. Each was
forced by a test or a gate, and each is recorded here rather than in a commit message because a
later phase will otherwise re-argue it from scratch.

### 11.1 A local estimate must be computed from the region it describes

Three modules in this phase estimate a "local background" and subtract it: `hair` against what is
behind a strand, `clothing` against the fabric around a mark, `eyes` against the structure inside
an iris. All three were written as a box blur of the whole luminance plane, and **all three were
wrong in the same way**.

| Module | What the naive blur did | What it looked like |
|---|---|---|
| `hair` | averaged across the edge of the hair mass | the boundary of every well-matted head read as a column of flyaways |
| `clothing` | let a bright lint raise its own neighbourhood's mean | a ring-shaped "stain" around every speck, large enough to be refused as an object |
| `eyes` | reached out of a twelve-pixel iris into a sclera three times its luminance | a perfectly flat iris read as full of detail, so no frame ever got clarity |

The fix is the same idea three times, and it is worth stating as a rule because it generalises:
**an estimate of what a pixel is sitting on must not be computed from pixels that are not that.**
`hair::background_estimate` replaces the hair mass with the background's own median before
blurring; `clothing::robust_local` runs a second blur over a plane in which outliers have been
replaced by the first blur's value; `eyes::measure` fills everything outside the iris, and the
catchlight inside it, with the iris median.

This is the same family as the defect phase 18 found in its resampler - arithmetic that reads
outside the region it is describing - and phase 19's rule that a weight must read the input. It
was found here three separate times by tests that expected an obvious detection and got a refusal,
which is the argument for writing tests that assert something *happens* rather than only that
nothing bad does.

### 11.2 The teeth guarantee measures the change, not the distance

`NaturalnessReport::teeth_excursion` was frozen as "the largest `u'v'` distance any teeth pixel
ended outside the locus", held below `TEETH_EXCURSION_CEILING` of 0.003. That is unreachable by
construction and the end-to-end fixture is what showed it: `MAX_TEETH_YELLOW` removes about a
third of the excess, so a strongly yellow set of teeth is *still outside the locus* afterwards -
which is the design, not a failure - and the guard withdrew the teeth family on exactly the
photographs the operator exists for.

The field now carries `max(0, after - before)`: **how much further outside the locus the plan
pushed the teeth.** Zero for every plan the solver intends to produce, because the operator only
ever reduces the excess. What it catches is the two things that must never happen - a correction
that moves a tooth further from natural, and one that overshoots past the locus and out the other
side.

The general form is worth keeping: *a guarantee about an operator must be expressed in terms of
what the operator can control.* A bound on an absolute quantity that the operator is deliberately
forbidden from reaching is not a guarantee, it is a permanent refusal wearing one's clothes.

### 11.3 A borrow refused for size gets its own reason code

`MicroCode` gained a thirty-third variant, `BorrowRefusedTooLarge`. The frozen set had two borrow
refusals - the region still carries information, and no sibling aligned - and neither describes
the third case: the record *is* destroyed, a sibling *does* align, and the region is larger than
`MAX_BORROW_AREA`. Without a code for it the frame recorded a reduction and said nothing about the
composite it declined to make, which is the one thing in this phase that must never be silent.

The two refusals stay separate rather than merging into one "borrow refused", because a
photographer reads them differently: the first says the photograph still holds the eye and a
reduction is the *better* repair, and the second says rebuilding this much of a face would be a
composite rather than a patch.

### 11.4 The plan-wide resolve ceiling is a constant, not a product

`NaturalnessReport::problem` bounded the resolve counter at `NATURALNESS_MAX_RESOLVES *
OpFamily::COUNT as u8`, and that cast is a narrowing conversion inside a frozen contract - the
kind the workspace lint block denies, and the kind that would be a silent wrap if the family
vocabulary ever grew past 255. The contract now carries `NATURALNESS_MAX_RESOLVES_TOTAL` as its
own `u8`, with a `const` assertion beside it that fails the build if `OpFamily::COUNT` stops being
three.

The general form: **a contract should not compute a bound from a value of a different width.** The
two facts - three families, three attempts each - are both constants, and writing the product as
one is what lets the assertion keep them in step. A fourth family added without touching this
number would otherwise leave a plan able to spend twelve renders while the check still allowed
nine, which is a budget overrun the guard would never report.
