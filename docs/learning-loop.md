# When AURA learns from you

Every time you change something AURA decided, it notices. Not immediately, and not silently, and
never on its own.

This is what it watches, what it does with it, and the things it will never learn.

---

## The shape of it

1. You correct something — a keeper AURA rejected, an exposure it got wrong, a retouch strength that
   was too much.
2. AURA writes the correction down beside the decision it corrected, so it knows *what* you
   disagreed with rather than just what the value ended up being.
3. When it has enough corrections, from enough weddings, saying the same thing, it works out an
   adjustment.
4. **It shows you the adjustment and waits.** Nothing changes until you say yes.
5. If you say yes and the next wedding is worse, one click puts it back.

## Enough means enough

Two floors, and both have to be cleared.

**Twelve corrections.** Fewer than that and you are looking at a mood rather than a preference. The
day you were editing at two in the morning and pulled everything half a stop down is not a style.

**Two weddings.** This is the one that matters. Twelve corrections from one wedding is one venue,
one set of lights, one dress, one photographer's evening. AURA will not learn a permanent preference
from a single afternoon, no matter how consistent that afternoon was.

Corrections are sorted into buckets — this kind of decision, in this kind of scene, about this kind
of subject — and each bucket clears its own floors. So a preference for warmer ceremonies does not
become a preference for warmer everything.

## The one that disagrees

Inside a bucket, corrections that sit far away from the rest are dropped before anything is fitted.

Not because you were wrong. Because a single frame where you did something unusual — a deliberately
blown-out silhouette in a bucket of normally-exposed portraits — is a decision about *that
photograph*, and averaging it in makes every other photograph slightly wrong.

## Small steps

An adjustment can move a value by at most half of what your corrections were asking for, and never
past the ceiling that value has anyway.

Half, and not all of it, because the corrections AURA can see are the ones you *made*. The
photographs you left alone were fine, and they are not in the sample. Moving all the way to the
average of the corrections would shift a value that was already right on the frames nobody
complained about.

## It measures itself before it asks

A quarter of your corrections are held back and never used in the fit. The adjustment is then
measured against those — corrections it has never seen — and what you are shown is how much closer
it would have got on them.

If the improvement is smaller than two per cent, **you are not asked**. An update that cannot show
it helps is an update that is not worth the risk of adopting.

The split is decided by the correction's own identity, so it is the same on every machine and on
every run. It is not random, and it does not change if you look twice.

## What the offer looks like

A list of at most twenty-four lines, in plain language:

> In ceremonies, expose about a third of a stop brighter — from 22 corrections across 3 weddings.

Beside it: what your profile does now, what it would do, and a before-and-after on frames you have
already delivered.

Twenty-four lines because that is about what somebody reads before agreeing. An update that wanted
to change ninety things is an update nobody would actually read, and agreeing to it without reading
it is the same as it happening on its own.

## Going back

The last ten versions of your profile are kept. Roll back to any of them, in one click, and the
change is instant — nothing is re-rendered, because a profile is applied when a photograph is
edited rather than baked into it.

If you roll back an update, AURA does not offer you the same one again next week.

## Things AURA will never learn

This is the important list.

**It will never learn to skip a check.** The texture floor on retouching, the skin protection in the
grade, the identity constraint on restoration, the crop safety filter, the naturalness guard — these
are guarantees, and a guarantee that erodes because you kept overriding it is not a guarantee. If
you find yourself fighting one of them, that is a bug report, not a training signal.

**It will never learn from another photographer's corrections**, unless you explicitly opt in to
contributing to a shared dataset, which is off and stays off until you turn it on.

**It will never learn about a person's appearance.** There is no bucket for it, no value it could
move, and nothing in the product that could express "this person should look different".

**It will never learn silently.** There is no state of this system where your profile changed and
you were not asked.

## Where your corrections go

Nowhere.

They live in your catalog, on your machine, and the fit happens locally in under a second. Nothing
about a correction is sent anywhere unless you have turned on dataset contribution, which is a
separate, explicit, per-project switch with its own consent record.

## What this release can and cannot claim

Everything above is built and tested. What has **not** happened is a profile fitted from a real
photographer's real corrections — there is no consented archive in this repository — so the phase's
own target, a fifteen per cent improvement in style match after three corrected weddings, is
unmeasured.

The machinery refuses to pretend otherwise: the panel says whether a profile was fitted from real
corrections, and on this build it says no.
