# Teach My AI: how AURA learns the way you edit

A photograph can be *correct* and still not be *yours*. AURA gets the exposure onto the face
and the white balance onto the light that was actually in the room, and then it grades the
frame the way most photographers would. This page is about the difference between that and
what you would have done — and about how AURA learns it from weddings you have already
delivered.

## The short version

You point AURA at folders of finished weddings. It matches each original to the photograph you
delivered, works out what you did to it, and learns the difference between your answer and its
own — **separately for each kind of photograph and each kind of light**.

Then it applies that difference to new work. Not as one look for the whole wedding: a
candlelit ceremony and a golden-hour portrait get the way *you* edit each of those.

## What it needs from you

**Both halves.** The camera originals and the finished photographs. AURA learns from the
difference between them, so a folder of JPEGs alone teaches it nothing — there is nothing to
compare them against.

**About three hundred pairs.** That is where AURA is confident. It will make a profile from
about a hundred and twenty and will tell you plainly that it is weak. It will not make one
from twenty, and the reason is in the next section.

**Your sidecars, if you kept them.** If your XMP files or your Lightroom catalogue are beside
the originals, AURA reads your settings exactly and for free. If they are not, it works them
out by reproducing your finished photograph, which is slower and very nearly as good.

## What it does with them

For each pair, AURA either reads your settings or reproduces your photograph. Then it sorts the
pair into one of eighty **buckets**: eight kinds of photograph — preparation, details, ceremony,
portraits, reception, dance, candid, other — across ten kinds of light — daylight, golden hour,
shade, overcast, tungsten, LED, flash, candlelight, stage light, and unknown.

Inside each bucket it works out how far you sit from the consensus. A bucket with four hundred
of your photographs in it gets almost entirely its own answer. A bucket with eight leans mostly
on the rest of your work, and a bucket with none uses your overall look. There is **no cliff**
anywhere in that: one more photograph never suddenly changes how a whole kind of frame is
edited.

## What it tells you afterwards

Not "profile ready". A number.

* **How close it gets to your own edits.** Measured by holding back one photograph in five,
  editing it with your new profile, and comparing that with what you actually delivered. The
  target is 2.5 dE00, which is roughly the point where two versions of the same frame stop
  looking like two edits.
* **How many photographs it used, and how many it left out.** A pair AURA cannot reproduce is
  left out on purpose — see below — and you are told how many.
* **Which buckets are weak, and what to shoot.** "Add one wedding with dance in flash light —
  this profile has only six of them." One thing, not a list.
* **A strength meter**, not a ready light. It combines how much evidence there is, how much of
  your work is covered, and how well it actually matched.

## Why some of your photographs are left out

If AURA cannot reproduce your finished photograph closely enough, it does not learn from it.

That is deliberate and it protects you. A photograph you dodged a face in, or composited, or
cropped heavily, contains work AURA cannot copy. If it learned from it anyway, the only way it
could reduce the difference would be to lift *every* shadow in the wedding — because the one
thing it can move is a global setting. Forty brightened faces would become a profile that
brightens everything.

So it leaves those out, counts them, and tells you. A run at 90 % is healthy. A run at 30 %
usually means the folder pairs are wrong, not that your editing is unusual.

## What it will never do

**It will never move somebody's skin.** This is the same promise the rest of AURA makes, and
your style does not get an exemption from it. Your profile is applied *before* the skin check,
not after — so if your usual warmth would have moved somebody's skin colour on a particular
frame, that frame gets less of it, and the panel says so. You can teach AURA your taste. You
cannot teach it to break that.

**It will never overwrite something you set.** A value you moved by hand stays where you put
it, on every frame, for ever.

**It will never make a photograph worse than switching the feature off would.** A style is a
*difference* from what AURA already decided, so an empty profile, an unpopulated bucket and a
half-finished training run all produce exactly what AURA would have done anyway.

**It will never upload your archive.** AURA reads your files where they are. Nothing is copied,
nothing is sent, and there is no code in this part of the product that could send it — the
part that talks to a network is not connected to the part that reads your weddings.

## Several looks, and which one a wedding gets

You can keep more than one: a personal look, a light-and-airy set for one client, a dark-and-
moody one, one for a second shooter. A wedding uses one of them, and you can override that for
a single chapter — a moody reception with an airy ceremony is a real thing photographers do and
the product supports it directly.

## Sharing a look with your team

A profile exports to a single signed `.auraprofile` file. Anyone with AURA can import it.

**What the signature proves is that the file has not changed since it was made.** It does not
prove who made it, and AURA does not claim otherwise: there is no directory of studios to check
a key against. The panel shows a short fingerprint and the words "unchanged since signing", and
it will refuse a file that has been altered, truncated or corrupted in transit.

## What is true about this build, said plainly

There are **no photographers' archives in this repository**, and there were none while this was
built. Everything above works, and everything above has been measured — against synthetic
archives where a look was chosen, applied through AURA's own renderer, and then recovered.

That proves the matching, the fitting, the bucketing, the maths and the file format. It does
not prove that a real photographer would look at the result and recognise their own work, and
nobody has run that test yet. It is written down as condition C1 in
`docs/progress/PHASE-17-EXIT.md`, and it will stay written down until five photographers have
tried it.

## Related

* `docs/tone-and-colour.md` — what AURA decides about tone and colour before your style shifts
  it.
* `docs/mixed-lighting.md` — how AURA works out what colour the light was.
* `docs/skin-fairness.md` — the skin promise, and exactly how far it has been tested.
