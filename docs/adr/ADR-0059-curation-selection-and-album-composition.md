# ADR-0059 - Curation: B&W suitability, hero selection, album composition and the grounded caption

- **Status:** accepted
- **Phase:** 29
- **Supersedes:** nothing
- **Amends:** nothing frozen. Section 5's interfaces are implemented with six extensions, all
  recorded in section 4 below.
- **Related:** ADR-0011 (the similarity index), ADR-0025 (culling, coverage and gallery sizing),
  ADR-0027 (the decision ledger and confidence), ADR-0029 (the render pipeline, and the `Bw` block
  already in the recipe), ADR-0031 (the measured skin locus), ADR-0033 (the eight HSL bands),
  ADR-0041 (crop safety and the aspect variants), ADR-0049 (a cloud answer that can only make the
  product do less), ADR-0051 (gallery consistency), ADR-0055 (QC), ADR-0060 (the curation IPC
  surface)

## 1. Context

Twenty-eight phases produced a gallery. This one produces the things a photographer sells out of
it: a portfolio set, an album draft, a set of posts and a teaser to send on the wedding night.

Nothing here is new capability in the machine-learning sense. Every input already exists - phase 05's
vectors and descriptors, phase 06's faces, phase 07's chapters, phase 08's moments, phases 09 to 11's
quality readings, phase 12's gallery and coverage engine, phase 15's measured skin loci, phase 23's
aspect variants, phase 25's node targets. The phase document is right that curation is nearly free
capability.

What is *not* free is the thing that makes curation different from the twenty-eight phases before it:
**it is the first phase whose output is a matter of taste.** A white balance is right or wrong against
a measured illuminant. A closed eye is a closed eye. Whether the second-best photograph of the first
dance belongs on the left-hand page of spread eleven is a judgement two competent photographers can
disagree about all afternoon, and the product has no way to be *correct* about it.

Everything in this ADR follows from taking that seriously.

## 2. Decision

Build curation as a **proposer that owns no output**: five selectors and one sequencer, all
deterministic, all explained, all reversible, none of which changes a photograph, writes a recipe,
delivers a file or removes anything from the gallery.

Concretely:

1. **B&W suitability** scores a keeper on five measured terms and solves a per-frame eight-band
   luminance mix out of the frame's own stored histogram and **the people in it, using each
   person's own measured skin locus** rather than an assumed skin hue.
2. **Hero selection** applies a technical veto, ranks the survivors on a weighted blend, and then
   picks greedily under three diversity constraints.
3. **Album composition** allocates spreads per chapter, places the coverage guarantees *first as a
   filter*, fills the rest by value, then improves rhythm and pairing by bounded local search that
   can never move a frame out of its chapter.
4. **Social and teaser** sets are slot-filling selectors over the same candidates with a legibility
   term, and they carry captions from a **closed vocabulary**.
5. **The cloud can only be agreed with.** A proposed move is applied only when the local objective
   says it is an improvement; a proposed caption only when it passes the same grounding check the
   local caption passes by construction.

## 3. Why the deciding phase is the phase that owns no output

Section 2.2 puts posting in phase 30 and album page layout out of scope entirely. That could read as
a scoping convenience. It is not: it is the property that makes a taste-heavy feature safe to ship.

A curation engine that could apply its own B&W conversion to the delivered gallery would be a
product that decides a wedding is monochrome. A hero list that removed the frames it did not pick
would be a product that decides what a photographer's portfolio is. Section 6.1 says B&W is
"presented as suggestions, never applied automatically to the main gallery" and it is right, and
the same argument covers all five outputs.

So `CurateService` has no `apply`, `curation.toml` has no strength anywhere in it, and
`crates/aura-curate/tests/no_outputs.rs` - the ninth grep-as-a-test in this repository - fails the
build if this crate ever writes a recipe, opens a file, reaches a provider outside the one cloud
task, renders a pixel, or grows a field a delivery path could hang off. `Recipe.bw` already exists
from phase 14 and stays empty on every frame this phase touches; the mix travels as a *proposal*
until a person accepts it, and accepting is an IPC command with a photographer behind it.

The corollary is what makes the reordering acceptance criterion meetable. Because nothing was
delivered, every pick is a row that can be replaced, and "reordering is instant and remembered" is a
store property rather than a rendering problem.

## 4. The six places this contract carries more than section 5's shapes do

Section 5 freezes five shapes. Every field it names is in the frozen contract, in the order it names
them, with the type it names - and six of them carry something more. The reason is the same in all
six: the shape as written cannot express something the phase's own section 13 requires.

**`bw: Vec<(ImageId, BwMix, f32)>` becomes `Vec<BwPick>`.** A bare triple has nowhere to put the
reasons, and section 13's fourth criterion is "every pick is explained". `BwPick` is that triple -
`image_id`, `mix`, `score`, in that order - plus `terms`, `reasons`, `confidence`, the bands
somebody's skin was measured into, and what the photographer said. `terms` is what makes the panel
able to show *why* a frame suits monochrome rather than only *that* it does.

**`heroes: Vec<(ImageId, f32, Vec<Reason>)>` becomes `Vec<HeroPick>`.** Same reason, one step
further: a hero also needs its rank, its chapter, its shot scale and **which of the three diversity
constraints was binding** when it was chosen, because "why is this one a hero and that one not" is
answered by the constraint far more often than by the score. Two frames from the same kiss can
differ by 0.004.

**`chapter_map: Vec<(ChapterId, Range<usize>)>` becomes `Vec<ChapterSpan>`.** `std::ops::Range` is
not `Copy`, is not `Ord`, and does not round-trip the way the rest of this contract does; and a
chapter that received no spreads has to be expressible, which a `Range` can only do as an empty
half-open interval that reads like a bug. `ChapterSpan { chapter, first, len, target }` carries the
same information plus the number of spreads the allocator *wanted* to give the chapter, which is the
number the panel shows when a chapter came up short.

**`Spread` gains four fields.** `left`, `right` and `single` are unchanged. `id` is there because
section 13 requires that a photographer's reordering is remembered, and a spread identified by its
position renumbers the instant anybody drags a frame - so the accepted pairing, the note and phase
30's record of what was exported would all be pointing at a different spread after every edit.
`chapter` is there because chapter order is inviolable and a spread that did not know its own
chapter could not be checked. `pair` and `reasons` are the explanation.

**`hero: (ImageId, AspectVariant)` becomes `Option<SocialPick>`.** A project with no keepers has no
hero, and a tuple that must exist forces one to be fabricated - which is the failure phase 12
guarded against when it refused to invent coverage. `SocialPick` also carries the slot, the
legibility reading and the reasons, for the same explanation reason as the other two.

**`teaser: Vec<ImageId>` becomes `Vec<TeaserPick>`.** A bare id cannot say what a frame is doing in
the set, and section 6.4's teaser is a *slot list* - "hero, couple, ceremony peak, one family, one
detail, one dance" - so which slot a frame filled is the whole content of the pick.

`AspectVariant` resolves to phase 23's `Aspect` by re-export rather than by redefinition. There is no
second aspect vocabulary in this product: `GeometryService` decides which crops of a frame are safe,
and a curation surface that invented a sixth ratio would be offering a crop nobody had checked.

One thing went the other way. **`AlbumPlan` and `CurationResult` carry no `serde` derive**, because
`CoverageReport` has none - it is phase 12's frozen shape and adding a derive to it for a
convenience would be a sixth amendment to a frozen contract. That turned out to be the smaller half
of the reason: `CurateService::export` publishes the album as a specification another tool reads,
and a derived serialiser makes that format a consequence of Rust field names, so renaming
`rhythm_measurable` would silently change a published format under every album-design script that
had ever consumed one. `aura_curate::export` writes it by hand with a documented key order.

## 5. Why the B&W mix is solved against a measured skin locus

Section 6.1 asks for "a red-heavy mix for warm skin against green foliage". Written naively that is
a constant: *skin is orange, so weight the orange band*. That constant is exactly what
`docs/skin-fairness.md` says this product does not have, and it is wrong on its own terms - the
orange band is where most skin sits at most skin tones, and "most" is not a basis on which to
choose how bright somebody's face is in a photograph that has no colour left to correct it with.

So the band a mix protects is **looked up per identity** from `ToneService::skin_loci`, which is
phase 15's measurement of that person's own chromaticity across the wedding, converted from `u'v'`
to a hue and mapped onto phase 16's eight bands. A frame with two people whose loci fall in
different bands protects both.

Three consequences worth stating:

- There is no skin constant in `aura-curate`, in `curation.toml`, in migration 29 or in the contract.
  The phase gate scans the schema and the source for one on every run, as phases 15, 25 and 27 do.
- When no identity in the frame has a usable locus, the mix is solved on **separation alone** and
  `BwCode::SkinLocusUnavailable` says so. It is not solved against a default skin band, because a
  default skin band is the constant this decision exists to avoid.
- The objective is *separation*, never lightness. A mix may not raise or lower the skin band's
  luminance beyond `MAX_SKIN_BAND_SHIFT`; what it may do is move the bands the skin is competing
  with. Lightening somebody's face is the one thing section 11 of the operating manual forbids
  outright, and a monochrome conversion is the easiest place in the product to do it by accident.

The instrument is honest about its own resolution and the phase says so: the mix is solved from the
frame's stored 8x8x8 HSV histogram rather than from its pixels, because phase 05's rule is that
descriptors are computed once and this phase opens no photograph. Five hundred and twelve bins is a
coarse instrument for a boundary between a face and a hedge, and `docs/curation.md` says so in the
product's own words.

## 6. Why heroes are an arithmetic blend under a veto, and culling is a geometric mean

Phase 12 fuses its four sub-scores as a **geometric** mean so that no signal can rescue another, and
that rule has been right for four phases. This phase does not inherit it, and the difference is
worth writing down because the two look like the same problem.

Culling decides what is **delivered**. A gallery is the whole record of somebody's wedding, and a
frame that is out of focus does not belong in it however extraordinary the moment was - so a
multiplicative fusion, where a near-zero term drags the product to near zero whatever the other
three say, is the correct shape.

A portfolio is a **ranking among frames that already passed that test**. Every hero candidate is a
keeper, so every candidate has already cleared phase 12's vetoes and its geometric fusion. Applying
a second multiplicative penalty would re-rank a set that is already technically sound by technical
quality again - and the effect of that is a portfolio of the sharpest photographs rather than the
best ones, which is precisely the failure section 6.2's diversity constraints exist to prevent.

So: a **hard technical floor as a veto** (`HERO_TECHNICAL_FLOOR`, applied before any score is
computed, phase 12's rule and phase 23's and phase 24's), and above it a weighted arithmetic blend
of technical, emotion, composition, uniqueness and story importance. The weights live in
`curation.toml` with a written reason per row, as every PM-owned table since phase 10 has.

## 7. Why coverage is a filter here too, and what it is a filter over

Fifth application of the rule phase 12 wrote, phase 23 applied to crop safety and phase 24 made a
property of the type system. In this phase it takes a form the earlier four did not:
**the album's must-have frames are placed before the value ranking is consulted at all.**

The reason is structural rather than stylistic. An album is 60 to 120 images out of a gallery of
600 to 1,200, so it is a far tighter selection than the cull that produced the gallery - and the
frames a coverage rule protects are, disproportionately, frames that scored moderately. A coverage
term added to the objective would lose to two beautiful portraits every single time, and the album
would arrive without the ring exchange in it.

`album::allocate` therefore reserves a slot per satisfied must-have and per close-family identity
first, and the value ranking fills what is left. `CoverageReport` is reused verbatim from phase 12 -
there is no second coverage vocabulary in the product - but **the report is computed over the album
rather than over the gallery**, and that is the number `AlbumPlan::coverage` carries. A caller that
rendered phase 12's report beside an album would be answering a question nobody asked.

One thing this phase deliberately does not do: it does not force a must-have into the album when the
gallery has no frame for it. `Coverage::Missing` propagates from phase 12 and is reported. Phase 12
wrote that rule - the product cannot invent coverage - and an album is not the place to start.

## 8. Why chapter order is inviolable and rhythm is a bounded local search

The album sequence is chronological at the chapter level and nothing may change that: not the
optimiser, not the photographer's drag-and-drop, and not the cloud. `album::optimise` only proposes
swaps *within* a chapter's span, `curate_set_order` refuses an order that reorders chapters, and the
cloud task's `moves` are validated against the same rule before any of them is applied.

That is not a modelling convenience. A wedding album whose ceremony follows its reception is not an
album with an unusual sequence; it is an album that is wrong, and no rhythm score is worth it.

Inside a chapter, rhythm is measured as agreement with a per-chapter target pattern over the shot
scale of each frame - wide establishing, medium action, tight emotional - and improved by a bounded
number of adjacent-pair swaps, each accepted only when the combined rhythm-and-pairing objective
improves. Bounded rather than converged: the search is `MAX_SWAP_PASSES` passes over the sequence,
deterministic in order, so the same gallery produces the same album on every machine. Invariant 4.

**Shot scale is measured, and it is frequently unmeasurable.** It comes from the largest face's area
fraction where there are faces, and from the scene label where the scene is one whose scale is known
by definition - a detail is tight, a venue establishing shot is wide. Where neither applies it is
`ShotScale::Unknown`, and an unknown frame is **excluded from the rhythm score's denominator**
rather than counted as a miss. `AlbumPlan::rhythm_measurable` is the share that could be measured,
and on this build - where phase 06's detector finds no faces - it is low. A rhythm score of 1.000
over the 8 % of an album that could be scored is not a claim about the album, and the panel says so.

## 9. Why facing near-duplicates is a filter and tonal clash is a term

Section 10.1 asks for two spread properties and they are enforced by two different mechanisms,
deliberately.

**No facing near-duplicates is a hard constraint.** Two frames from the same moment, or within
`MAX_PAIR_SIMILARITY` of each other in the phase 05 index, are never placed on facing pages. A
photographer looking at a spread of the same photograph twice does not think the pairing objective
weighted something poorly; they think nobody looked at it. There is no weight at which that is
acceptable, so it is not a weight.

**Tonal clash is a term.** A spread whose two frames differ in tonal weight is worse than one whose
frames match, and considerably better than a spread that had to be left half-empty to avoid it. It
is scored, bounded by `MAX_PAIR_TONAL_GAP` above which the pair is refused, and reported per spread
so a photographer can see which pairs the optimiser was unhappy with.

`SpreadPair::facing_known` is the third piece and it is the phase-24 rule again: a spread whose
subjects' facing could not be measured is not a spread whose subjects face inward. It is a spread
nobody could check, it scores zero on that term rather than full marks, and the outline counts them.

## 10. Why a caption's vocabulary is closed

Section 6.4 asks that captions be "grounded - the model may not invent details about the couple",
and section 10.1 asks for an automated check that they contain "no invented names, places or
claims". Every implementation of that as a *filter over bad things* fails: a blocklist of names
cannot enumerate names, and a model asked politely not to invent a venue will occasionally invent a
venue.

So the check runs the other way. `caption::vocabulary` builds, for one project, the closed set of
content words a caption may contain: the chapter labels, the ritual names phase 07 resolved for
*this* wedding's traditions, the scene labels, the role words (`couple`, `family`, `guests` - never
a person's name, which this product does not store as a name anyway), plus a fixed list of function
words and neutral connectives that carries no facts. A caption is **accepted only when every content
word in it is in that set**. Anything else - a name, a place, a date, a claim about how anybody felt -
fails, and failing means the caption is replaced by the local template.

That makes grounding a property of the type system's edges rather than a hope about a prompt, and it
makes the same check apply to the local captions and the cloud ones. The local template passes by
construction, because it is assembled *from* the vocabulary.

The cost is real and worth stating: the captions this produces are plain. "The ceremony, and the
vows" is not copywriting. It is, however, a sentence the product can prove it is entitled to write,
and `docs/curation.md` tells a photographer to edit it.

## 11. Why the cloud can only be agreed with

Phase 24 established that a cloud call whose answer type cannot approve anything has no unsafe
failure mode. This phase's call *can* propose something - that is what a sequencing refinement is -
so the property has to be built differently, and it is built at the point of application rather than
in the answer type.

A proposed move is applied when, and only when, all four of these hold:

1. It stays inside one chapter's span, or moves a frame between two **adjacent** spreads of the same
   chapter. The system prompt asks for this; the validator enforces it.
2. The resulting sequence satisfies every hard constraint - coverage, no facing near-duplicates, the
   tonal gap ceiling.
3. The combined rhythm-and-pairing objective **improves**. The local optimiser is the judge.
4. Fewer than `MAX_MOVES` moves have been applied.

A proposed caption is applied when it passes section 10's vocabulary check and is inside
`CAPTION_MAX_WORDS`.

So an unreachable provider, a spent budget, a malformed answer, a hallucinated index and a model
that proposes twenty moves that all make the album worse produce **the same album**: the one the
deterministic optimiser produced. Invariant 6 and the operating manual's ninth cloud rule - cloud
proposes, deterministic code decides - as an executable property rather than a convention.

`CurationOutline::cloud_used` and `cloud_moves_applied` are both on the wire, because "the model was
asked and agreed with us" and "the model was never reached" are different facts about an album and a
photographer paying per call is entitled to tell them apart.

## 12. Alternatives considered

**A learned hero ranker, per section 8's implementation order.** Rejected for the reason phases 17,
23, 25, 26 and 27 each rejected one: there is nothing to train it on. Section 9's DATA row asks for
60 real album sequences, hero sets and B&W selections collected with permission, and this repository
has none. `ml/models/curate/` ships the training and evaluation code so that the first studio with a
consented archive can run it; nothing in this build consults a model, and `HERO_HEAD_TRAINED` and
`BW_HEAD_TRAINED` are both false. This is the ninth phase to ship no model and the fourth to do so
because the data is absent rather than because there is nothing to train.

**Simulated annealing or an integer program for the sequence.** Rejected on determinism. Invariant 4
requires the same gallery to produce the same album on every machine, and an annealer is
reproducible only with a pinned seed and a pinned iteration count, at which point it is a bounded
deterministic search with extra steps and a worse failure mode - a swap accepted because the
temperature was high is a swap nobody can explain, and every pick in this phase has to be
explainable.

**Scoring coverage into the album objective rather than filtering on it.** Rejected in section 7.

**A separate B&W set stored as its own gallery.** Section 6.1 mentions "a dedicated B&W set" and it
is tempting to make that a second `SelectionResult`. Rejected: two galleries is two answers to what
is being delivered, which is phase 12's rule. The B&W set is the set of frames whose suitability is
above the acceptance threshold and which a photographer has accepted, which is a query over
`curate_bw` rather than a second gallery.

**Making the album a `Recipe` per frame with `bw` filled in.** Rejected in section 3.

## 13. Consequences

- `CurateService` is the twenty-fifth frozen service and the first whose subject is a **deliverable**.
  Phase 30 exports these plans, posts these sets and reads the reorder rows as its learning signal.
  No phase may keep its own hero ranker, its own album sequencer or its own idea of what suits
  monochrome.
- `aura-curate` depends on `aura-core`, `aura-catalog`, `aura-index` and `aura-cloud`, and on none of
  the deciding crates. Everything it reads about a photograph arrives through the `Field` port that
  `aura-app` implements out of the frozen services - the same indirection phase 27 built, for the
  same reason: `aura-brain-photo` must not depend on the crate that curates it.
- Every number in this phase is measured against synthetic galleries this repository authored. There
  is no photographer agreement study, so the three headline gates of section 10.1 - hero agreement
  0.75, album reordering 15 %, B&W acceptance 70 % - are **unmeasured**, and the phase gate prints
  that on every run.
- The reordering a photographer does is stored, is never overwritten by a re-run, and is the input
  phase 30's learning loop reads. That is the one mechanism in this phase that can make a
  taste-heavy feature better over time, and it is why `curate_override` exists from the first
  release rather than being added when the loop lands.
