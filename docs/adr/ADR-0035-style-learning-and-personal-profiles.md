# ADR-0035 - Style learning: a residual on the baseline, conditioned on scene and light

- **Status:** accepted
- **Date:** 2026-08-19
- **Phase:** 17 - Style Learning: Scene-Conditional Personal AI Profiles ("Teach My AI")
- **Supersedes:** nothing. Extends ADR-0029 (render pipeline), ADR-0031 (exposure, white
  balance and skin) and ADR-0033 (tone curves, HSL and skin protection).
- **Deciders:** CTO, ML Lead - Vision, Colour Scientist, Senior Engineer - Core Pipeline,
  Product Manager

## Context

Phases 15 and 16 decide what a photograph *should* look like. They decide it well, and they
decide it the same way for everybody. A photographer's style is the difference between that
consensus and what they would have done themselves, and phase 17 is where the product learns
that difference from weddings the photographer has already delivered.

The competitor being beaten - Imagen's Personal AI Profile - needs about two thousand images
and learns **one** style. Real photographers do not edit a candlelit ceremony the way they
edit a golden-hour portrait, and a single global profile is the reason a tool that has
"learned your style" still feels almost-right-but-wrong on half a wedding.

Nine decisions follow. Each was a real fork, and five of them are about what this phase
refuses to do.

## Decision 1 - The style is consulted *inside* phases 15 and 16's passes, so every guarantee re-runs after the shift

**Chosen:** `StyleService` is frozen in `aura-core`, and `TonePass` and `ColourPass` each
hold an optional `Arc<dyn StyleService>`. The shift is applied to the *solved* parameters,
before phase 15's skin-locus constraint and before phase 16's clipping guard and skin guard.

**Rejected:** applying the shift in `aura-app`, after both passes have stored their
decisions, on the way into `aura_recipe::schema::merge`.

The rejected design is one line of code shorter and it silently destroys the only guarantee
this product makes about people. Phase 16 section 6.3 promises that grading never moves
anybody's skin measurably, and that promise is a *post-condition measured on pixels* - it is
true of the parameters the guard returned and of nothing else. A style delta applied after
the guard is a set of parameters nobody measured, delivered under a sentence that says
somebody did. Phase 16's own exit report anticipated this and wrote the rule down: "the skin
guard runs last, and it runs after phase 17's shift too."

The dependency runs through the trait and not through a crate, which is phase 10's pattern
(`IntegrityPass::with_emotion`) for the same reason: `aura-brain-photo` and `aura-style`
depend on each other in **neither** direction, so "no phase may keep its own grader" and "no
phase may keep its own style profile" do not become a cycle.

The cost is that phase 16's `ANALYSIS_VER` goes 1 -> 2, because a stored grade that was
solved without a profile and a stored grade that was solved with one are not the same
measurement, and every row is re-graded when a profile is adopted. Phase 15's `ANALYSIS_VER`
goes 2 -> 3 for the same reason.

## Decision 2 - A style is a residual on the baseline, and the baseline is never re-derived

**Chosen:** `StyleDelta` is additive on the decisions phases 15 and 16 already made. The
model predicts *how far this photographer moves from the consensus*, not what the parameters
should be.

**Rejected:** learning absolute parameters per bucket from the photographer's finals.

This is the decision that makes three hundred pairs enough, and section 6.2 says so
explicitly. The absolute problem has to relearn white balance, exposure placement, highlight
recovery and clipping avoidance from scratch, per bucket, from a few dozen frames each - and
it relearns them *including the photographer's past mistakes*, because a delivered gallery is
a record of what was shipped and not of what was correct. The residual problem inherits
phases 15 and 16's correctness for free and has only taste left to fit, which is a smaller,
smoother and far better-conditioned function.

It also gives the phase its safety property: with an empty profile, or a bucket with no
samples, the answer is the baseline. There is no state of this system in which a photographer
gets a worse photograph than they would have got with the feature switched off.

## Decision 3 - `BTreeMap`, not `HashMap` - a deviation from section 5's literal text

Section 5 writes `pub groups: HashMap<SceneGroup, StyleDelta>` and
`pub buckets: HashMap<StyleBucket, BucketModel>`.

**Chosen:** `BTreeMap` in both places.

`scripts/check-banned.sh` refuses `HashMap::new` outside tests, because invariant 4 requires
that identical inputs produce byte-identical output and a `HashMap`'s iteration order is not
a function of its contents. A profile is serialised into a signed bundle and hashed; two
exports of the same profile that differ only in map order would produce two different
signatures for one profile, and "tampered bundles are rejected" (section 10.1) would start
rejecting untampered ones. The phase document's `HashMap` is shorthand for "a map", and this
is the map this workspace uses. Recorded here because section 5 is frozen text and the
difference is visible.

## Decision 4 - The recipe fitter is coordinate descent over a bounded vector, and a pair it cannot fit is rejected and reported

**Chosen:** for a pair with no XMP, fit the phase 14 recipe to reproduce the final by
coordinate descent on a twelve-parameter vector, at 512 px, against a perceptual loss (mean
dE00 on a sub-sampled grid plus a luminance-histogram term). A pair whose final residual is
above `fit::REJECT_DE00` is **rejected**, counted, and named in the report.

**Rejected:** gradient descent through the renderer, and accepting every pair with a
residual-derived weight.

There is no automatic differentiation in this workspace and adding one to fit twelve
parameters would be a large piece of machinery for a problem that coordinate descent solves
in about a hundred renders. More importantly, the parameters are *not* all smooth in the
loss - a curve control point moves a plateau, and highlight recovery has a knee - and a
coordinate sweep with a shrinking step handles both without a Jacobian.

The rejection is the part that matters. A pair whose residual stays high is a pair that
contains work this phase cannot model: a local dodge, a composite, a heavy crop, a sky
replacement. Fitting it anyway and down-weighting it puts *unmodelled retouching* into a
global tone delta, which is how a style profile learns to lift every shadow in the wedding
because forty frames had a brightened face. Rejecting it and saying so out loud - "1,842 of
2,000 pairs used, 158 rejected, mostly in the reception bucket" - is what prevents the
support ticket that begins "why doesn't it look like me".

## Decision 5 - Shrinkage is James-Stein toward the parent, and the chain has four links

**Chosen:** every bucket's delta is shrunk toward its scene group's, the group's toward the
global, and the global toward zero, with a weight of `n / (n + k)` per level. The resolution
order at inference is bucket -> group -> global -> factory baseline, and the level that
answered is recorded on the decision and reported in telemetry.

**Rejected:** a minimum-sample cut-off, below which a bucket falls back entirely.

A cut-off is a cliff. A bucket with nine samples produces the group's answer and a bucket
with ten produces its own, and the photographer sees two visibly different looks either side
of a boundary that exists nowhere in their wedding. `n / (n + k)` is continuous: eight
samples move the bucket a little, four hundred let it dominate, and nothing anywhere is a
step. It is also the estimator that is actually right for this problem - a per-bucket mean
with a shared prior is the textbook case James-Stein was written for.

`k` is `shrink::PRIOR_STRENGTH` and it is a **product** decision rather than a fitted one:
it says how many frames of evidence it takes before AURA believes a photographer treats one
kind of light differently. It lives in one named constant with a written reason.

## Decision 6 - One wedding cannot move the profile more than a bounded amount, and the bound is per archive rather than per pair

**Chosen:** the per-bucket regression is fitted with Huber loss by iteratively reweighted
least squares, *and* every archive's total contribution to any delta is capped at
`shrink::MAX_ARCHIVE_INFLUENCE` of the fitted magnitude.

**Rejected:** Huber alone.

Huber bounds the influence of one *pair*. Section 10.1's robustness gate is about one
*wedding*: "one outlier wedding cannot shift the global profile beyond a bounded amount".
Those are different claims, and Huber does not make the second one - four hundred consistent
pairs from a single very cold, very flat archive are not outliers to a robust loss, they are
a mode. The archive cap is what makes the gate a property rather than a hope, and it is the
only place in this phase where a photographer's own data is deliberately not fully believed.

## Decision 7 - `SkinBias` is a deviation from the baseline, never a skin target, and the schema cannot express one

**Chosen:** `SkinBias { warmth, chroma }` is measured as the mean difference between the
photographer's own skin rendering and the baseline's, per pair, in the photographer's own
frames. It is applied as a bounded shift to the same parameters phase 16 already owns, and
phase 16's skin guard then runs and can withdraw it.

**Rejected:** storing a preferred skin chromaticity per profile.

This is the third time this product has faced the same trap and the third time it has
answered structurally rather than by policy. Phase 15 has no ideal-skin constant, phase 16
has no skin-target column, and migration 17 has no skin chromaticity anywhere in it - there
are two bounded scalars that say "this photographer runs skin a little warmer than the
consensus", relative to whatever the consensus said about *that frame's own people*. A stored
target is how an editor lightens dark skin while believing it is honouring a style, and the
defence is that there is nothing here that could hold one. The phase gate scans the schema
and the bundle format for one on every run, exactly as phases 15 and 16's do.

And the ordering is not negotiable: `SkinBias` is applied before the guard, so a style that
would move somebody's skin past phase 16's ceilings is a style the guard attenuates or
withdraws. A photographer can teach AURA their taste. They cannot teach it to break the
guarantee.

## Decision 8 - The bundle signature proves integrity, not provenance, and the product says so

**Chosen:** an exported `.auraprofile` is a canonical JSON document plus a detached ed25519
signature made with a per-installation key, with the public key embedded in the bundle. Import
verifies the signature over the canonical bytes and refuses a bundle whose signature does not
verify, whose digest does not match, or whose schema is from a future build.

**Rejected:** claiming this is an authenticity guarantee.

With the public key inside the bundle, a signature proves that the document has not been
altered since it was signed and that it was signed by whoever holds that key. It does **not**
prove who that was, because there is no key distribution in this product and nothing to check
a key against. That is enough for the threat this phase actually has - a profile corrupted in
transit, or edited by hand to carry parameters no solver would produce - and it is not enough
for "this look really came from that studio". The import panel says the fingerprint and does
not say the word "verified"; `docs/style-profiles.md` says the same thing in the product's own
words. A studio PKI is a real feature and it belongs to whatever phase actually builds
distribution.

## Decision 9 - No cloud call, and the archive walk cannot upload anything

Section 7 is unambiguous: "No cloud AI call in this phase. The phase must work with the
network cable unplugged." Nothing in `aura-style` depends on `aura-cloud`, there is no
`CloudTask` in this phase, and the crate's manifest records the omission as a decision rather
than an oversight. Section 9's SEC task - "ensure archives are never uploaded" - is therefore
a property of the dependency graph, and `crates/aura-style/tests/no_network.rs` is a grep as a
test that fails the build if this crate ever grows a cloud dependency.

## Consequences

- `aura-core` gains one frozen contract file and one typed id (`ProfileId`). It still depends
  on no workspace crate.
- `aura-brain-photo` gains an optional `Arc<dyn StyleService>` on two passes and two
  `ANALYSIS_VER` bumps. It gains no dependency.
- `aura-style` depends on `aura-core`, `aura-catalog`, `aura-raw`, `aura-recipe`,
  `aura-render` and `aura-preview`. It depends on no brain crate and on no cloud crate.
- Migration 17 adds three tables and one view. It stores no pixels and no skin chromaticity.
- This phase ships **no model**. Ridge regression with James-Stein shrinkage and a Huber
  reweighting is arithmetic with a closed form; there is nothing to sign, nothing to pin and
  no card to write, and `models.lock` is untouched. The third phase since 08 to ship none, and
  the first where that is a statement about the *method* rather than about the data.
