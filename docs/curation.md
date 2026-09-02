# How AURA curates a wedding, in the product's own words

> Your gallery is already chosen and edited. Curation is the part that decides which twenty
> photographs are the ones you show, what goes in the album and in what order, which frames are
> better in black and white, and what you post tonight.

Culling answers "what am I delivering". Curation answers the four questions that come after it, and
they are different questions with different right answers. The best photograph of a wedding is not
the same as the best opening spread, and neither of them is the frame that works on a phone screen.

This page is what AURA proposes, why, what it will never do, and — the part that matters most on
this build — **what it cannot yet judge**.

---

## Nothing here changes a single photograph

Curation produces four **proposals** and one order. It does not edit, crop, convert, export or
delete anything.

The monochrome suggestion is the clearest case. AURA can tell you that a frame would be stronger in
black and white and can show you exactly the conversion it has in mind — but it never applies it.
Converting a wedding to monochrome is a decision about what the day looked like, and it belongs to
the person who was there.

The same is true everywhere else in this feature. The album is a **draft sequence**, the portfolio
is a **shortlist**, the social sets are **suggestions**, and nothing about any of them is written
into a photograph's edit unless you accept it.

---

## The four things it proposes

### The portfolio — twenty photographs

Not the twenty highest-scoring frames. A top twenty made purely by score is twenty frames of
whatever the best-lit ten minutes of the day were, and it is useless: you cannot show it to a client
and you cannot put it on a website.

So three rules shape it, and AURA tells you which one applied to each pick:

- **At most four from any one chapter.** A portfolio that is six frames of the first dance is a
  portfolio about the first dance.
- **One frame per moment.** Two photographs of the same kiss, half a second apart, are one
  photograph as far as a portfolio is concerned.
- **A mix of distances.** Twenty tight head-and-shoulders frames read as a catalogue.

When a photograph you expected is missing, the answer is almost always one of those three rather
than its score — so each pick records **which constraint was binding** when it was made. Two frames
of the same moment can differ by 0.004, and what decided between them was a rule, not a judgement.

A frame that is out of focus cannot enter the portfolio at all, whatever else is true of it. That is
a floor rather than a term: no amount of emotion outvotes it.

### The album — sixty to a hundred and twenty photographs, in spreads

The album is built in **chapter order, always**. Getting ready, then the ceremony, then the
portraits, then the reception. An album that opens with the exit is not a stylistic choice and AURA
will not produce one — including when you reorder it yourself, where a drag that would move a
photograph into a different chapter is refused rather than accepted quietly.

Within that, the sequence is chronological with one exception: **pairing**. Two photographs across
a gutter have to work together, and three things stop a pair:

- they are the same shot (AURA will not face two near-duplicates at each other);
- one is much brighter than the other;
- they are from the same moment.

When the next photograph in the day cannot face the current one, AURA looks a little further ahead
inside the same chapter for one that can. That is the only reason an album ever departs from the
order the day happened in, and it never crosses a chapter to do it.

Every guaranteed moment your gallery covers is **also** in the album — the ring exchange, the first
kiss, the cake, the exit, the family formals — and so is every close family member, at least twice.
Those are applied before anything is scored, so a beautiful frame never displaces the only
photograph of somebody's grandmother.

### Monochrome — a short list, with a mix for each

AURA offers a frame for black and white when four things are true of it: the tones stay far apart
without the colour, the colour was a distraction rather than the subject, the noise would read as
grain, and — the one that matters most — **the picture is not the colour**.

That last one is the whole difference between a good suggestion and an insulting one. A red lehenga
against green foliage, a chuppah's drape, a bright sari at a reception: those photographs have their
subject *in the colour*, and converting them loses the picture. When AURA can see two substantial
saturated regions that would print as the same grey, it does not offer the frame at all.

The mix that comes with each suggestion is solved for **that photograph**, not chosen from a preset.
It looks at which colours are in the frame, which ones would collapse into the same grey, and moves
them apart.

### Social and the teaser

Three sets sized for where they go — a grid, a stories set, and a small hero set — plus the
overnight teaser, fifteen to thirty frames covering the whole day, so you can post something the
same night.

A frame is picked for a set partly on whether it survives being small. A wide frame with a lot going
on is a beautiful print and an illegible thumbnail.

---

## What it will never do

- **It will never write a caption that says something it was not told.** Every caption is assembled
  from words this wedding actually supplied: the chapters AURA identified, the scenes it recognised,
  the rituals named in your tradition settings. No names, no venue, no relationships, no claims
  about how anybody felt. If a suggested caption ever contained a name, it would be a name AURA
  invented — so it cannot contain one.
- **It will never use a gendered role word.** Not "bride", not "groom". Which of two people is the
  bride is not a photographic fact.
- **It will never reorder an album you have ordered.** Once you have dragged a spread, that order is
  yours. A later run rebuilds the spreads around it and tells you what it would have done instead,
  and the order survives adding two hundred photographs to the gallery.
- **It will never delete, move or convert anything.** See the top of this page.

---

## What it cannot judge on this build

Three things, and they are worth knowing before you read a proposal.

**Faces are not detected yet.** Face detection is not trained in this build, and three parts of
curation lean on it:

- *Which way people are facing* is the term album designers spend the most time on, and it cannot be
  measured. The spread view shows it in grey — "not checked" — rather than claiming the subjects
  face outward. A spread nobody could check is not a spread that passed.
- *How close the photographer was* is measured from the scene where it can be and is otherwise
  unknown, so the rhythm score is reported over the share of the album it could actually be measured
  on. A rhythm of 1.000 over a tenth of an album is not a statement about the album, and the panel
  shows both numbers.
- *The skin rule* below has nothing to protect.

**The skin protection is real and currently has nothing to apply to.** Where a person's skin tone
has been measured, the monochrome mix never moves the band it sits in — not a little, not in a safe
direction, not at all. That is a hard bound rather than a preference, and it is the concrete sense
in which a solved mix beats a preset: a preset is a set of numbers chosen before your photograph
existed, so it moves whatever band somebody's skin happens to fall in. But the measurement it needs
comes from face detection, so on this build every mix is solved as though the frame had nobody in
it. There is no ideal skin tone anywhere in AURA to compare a person against — see
[the skin fairness statement](skin-fairness.md) — and this rule is the reason there does not need to
be one.

**Nobody has checked the proposals against photographers.** AURA's own targets are that its top
twenty overlaps a photographer's by three quarters, that fewer than fifteen per cent of an album
gets reordered, and that seven in ten monochrome suggestions are accepted. **None of those three has
been measured.** They need real weddings and real photographers, and this build has been tested
against synthetic weddings whose answers were written down in advance. That proves the arithmetic
does what it says. It is not evidence that you would agree with it.

---

## Everything says why

Every hero, every spread, every monochrome suggestion and every social pick carries up to four
reasons and a confidence. The reasons name what was measured — "an emotional peak", "the strongest
composition in this chapter", "the chapter quota was full" — and the ones that say AURA *could not
check something* are never dropped to make room for the ones that sound better.

That is the difference between a tool that suggests and a tool that decides. A proposal you can
interrogate is one you can disagree with in ten seconds. A proposal you cannot is one you have to
re-check by eye, which is the work this feature exists to save.

---

## Where the decisions live

| Thing | Where |
|---|---|
| The reasoning behind all of it | [ADR-0059](adr/ADR-0059-curation-selection-and-album-composition.md) |
| The panel and its commands | [ADR-0060](adr/ADR-0060-curate-ipc-surface.md) |
| Album sizes, rhythm, formats, weights | `crates/aura-curate/config/curation.toml` |
| What is not yet proved | [the phase 29 exit report](progress/PHASE-29-EXIT.md) |
| How AURA chose the gallery in the first place | [how AURA culls](how-aura-culls.md) |
| Why there is no ideal skin tone in this product | [skin fairness](skin-fairness.md) |
