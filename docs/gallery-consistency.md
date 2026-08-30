# Gallery consistency: how AURA makes a wedding look like one body of work

This is the part of AURA that stops looking at photographs and starts looking at **your gallery**.

Every other part of the product answers a question about one frame: what colour was the light, how
should this be graded, is this face sharp. This part answers a question about four hundred of them
at once - *do these look like they were taken by the same person on the same day* - and that turns
out to be a different question with a different answer.

---

## Why a gallery can be wrong when every photograph in it is right

Suppose AURA gets the white balance of every frame in your ceremony within 200 K of what you would
have chosen. That is a good result on any individual frame; you would not notice 200 K on a
photograph you were looking at on its own.

Now put two of those frames next to each other. One is 200 K warm, the next is 200 K cool, and the
step between them is 400 K - which you *will* notice, because a person scrolling a gallery is not
looking at photographs, they are looking at the differences between photographs.

That is the whole problem. Every per-frame check in this product can be green while the thing your
client actually looks at reads as amateur. Photographers know this, which is why so much editing
time goes on sync and consistency rather than on individual frames.

---

## What AURA does about it

### It divides the wedding into parts that should look like each other

Not chapters - you already have those, and you can rename and re-cut them in the story panel. These
are **lighting groups**: the frames of one chapter that were shot under one light.

A ceremony is usually one group. A two-hour reception is not: its first hour and its last do not
describe the same room, so it becomes several. And a chapter where the light genuinely changed
part-way through - a flash going on, the sun setting, everyone moving indoors - becomes two groups
with a boundary at the change.

That last one is the piece that makes the rest safe. A candle-lit vow inside a bright ceremony has
exactly two possible outcomes if it shares a group with the ceremony: the vow gets flattened toward
the ceremony, or the ceremony gets dragged toward the vow. Neither is acceptable, and no amount of
gentleness avoids both - being gentle just makes both happen a little. So AURA looks for the moment
the light actually changed and treats the two sides as two different rooms, because they are.

### It anchors each part to its best frames, not to their average

Averaging a ceremony includes the ceremony's mistakes at their true weight. If a quarter of your
frames are half a stop dark because you were shooting into a window, the average is an eighth of a
stop dark - and matching everything to that makes the other three quarters worse.

So AURA picks **three to five reference frames** per part: the ones it is most confident about, with
a well-exposed subject, with somebody recognisable in them, and without two different lights fighting
in the same frame. Everything else in that part is matched toward those.

**You can override this.** Pin a frame you trust and it becomes a reference whatever AURA scored it -
you looked at the photograph, and you know something a confidence number does not. Reject one and
AURA will never pick it again. Both survive every re-run.

### It moves each frame part of the way, and never past a limit

Each frame moves toward its part's reference by a fraction of the distance - between 40 % and 90 %,
depending on the kind of photograph. Not all the way, ever. Moving every frame exactly onto a target
is how a gallery gets *flattened*, and the natural variation between two frames of the same room is
part of what makes a wedding look photographed rather than manufactured.

And there are hard limits it will never exceed, whatever the target says:

| What moves | Furthest it can move |
|---|---|
| Colour temperature | 450 K |
| Tint | 12 |
| Brightness | 0.35 EV |
| Contrast | 8 |
| Saturation | 6 |

These are ceilings. A studio can set them **lower** in the settings file; nobody can set them
higher, and AURA refuses to start if the file asks. A frame that would need to move further than
450 K to match its chapter is a frame whose own white balance is wrong, and that is fixed in the
Basic panel rather than here.

### It leaves alone what is meant to be different

Four kinds of frame come out of this untouched, and AURA tells you which:

* **Frames whose light is meant to be that colour.** A purple dance floor stays purple. Candlelight
  stays warm. AURA already decided, back when it worked out the white balance, that these lights are
  intentional - it does not get a second opinion here.
* **Frames you set yourself.** Anything you have adjusted by hand.
* **Frames you switched off.** There is a per-photograph switch.
* **Frames with more than one light in them.** These still join their chapter, but only half as far,
  because a single correction is right for the subject and wrong for the room.

### It matches how a person looks across the whole day

For each person in the wedding, AURA builds a picture of how their skin actually looks - from
**their own well-lit frames**, never from a reference. There is no ideal-skin value anywhere in this
product: not in the settings, not in the database, not in the code. That is deliberate, and
[`docs/skin-fairness.md`](skin-fairness.md) explains why at length. The short version is that a
fixed reference is how an editor lightens dark skin while believing they are correcting a colour
cast, and a system with nothing to compare a person against cannot do it.

Then it brings each frame of that person toward how they look across the rest of the wedding, inside
their skin region only, capped so the mood of the room survives. A candle-lit face may stay warm. It
may not go magenta.

The promise is measurable: **the same person's skin varies by no more than 2.0 dE00 across your
whole gallery.** AURA stores that number, so it is something you can check rather than something we
say.

### It tells you what would not come

Some frames cannot be brought into line - they are too far from everything else in their part, and
the limits above stop AURA moving them the whole way. Those frames are listed, worst first, with
exactly how far out they still are: *"+310 K warmer than the references, skin cast 4.2 dE00"*.

This is a short list on purpose. A frame that started a long way off and was fully corrected is not
on it. Only the ones that are *still* wrong are, because a queue full of things that have already
been fixed is a queue nobody opens twice.

---

## Reading the panel

**Two numbers in the header, and the second is the one to read when it is low.**

*Photographs matched* is how much of the wedding AURA placed into a part. *Parts anchored* is how
many of those parts it could find three frames confident enough to anchor.

A wedding at 100 % matched and 20 % anchored has had **almost nothing done to it**. Every frame has
a row saying it was considered; four fifths of those rows say "AURA could not judge this part of the
wedding, so it left every frame in it exactly as it was". That looks identical, in a summary, to a
wedding that was already perfectly consistent - so the panel shows both numbers and says which it
is.

**The spread is shown before and after, in kelvin and in stops**, rather than as a percentage. "77 %
closer" cannot tell 500 K → 115 K from 20 K → 4.6 K, and only the first is worth knowing about.

**The strips are numbers, not thumbnails.** Two frames whose thumbnails look identical can be 400 K
apart, and the strip is there to show you the thing your eye cannot catch in a grid.

---

## What AURA will not do here

**It will not move a photograph further than the limits above.** Not with a slider, not with a
setting, not on request. There is no strength control on this panel and there is no field on the
wire that could carry one.

**It will not change your chapters.** A lighting group is a measurement; a chapter is your
narrative. Renaming, splitting and merging chapters is the story panel's job, and having two
editable structures would mean having two answers to what your wedding's shape is.

**It will not overwrite anything you set.** A frame you have adjusted keeps your values through
every re-run, and AURA keeps its own suggestion beside yours so you can see where you disagreed.

**It sends nothing anywhere.** This whole feature runs with the network cable unplugged.

---

## What this build cannot do yet

Two honest limitations.

**Nothing about anybody's skin was measured.** The part of AURA that works out which pixels are a
person's skin is not trained in this build, so the skin matching above has not run on a single
photograph. The panel says so rather than showing a zero, because "everybody's skin is consistent"
is a promise about people and we will not make it on evidence nobody gathered.

**No photographer has looked at a before-and-after gallery from this build.** Everything above is
measured against galleries whose drift we created on purpose so we would know the right answer. That
proves the arithmetic works. It does not prove that you would call the result consistent, and that
study has not been done.

Both are recorded in [`docs/progress/PHASE-25-EXIT.md`](progress/PHASE-25-EXIT.md) as conditions C2
and C3.

---

## Every reason AURA can give

These are the exact sentences the panel shows, in the order it ranks them: what AURA declined to do
first, then what it did, then how the part was built.

| Code | What it means |
|---|---|
| `node_unanchored` | AURA could not find three frames it was confident enough about to anchor this part of the wedding, so it left every frame here exactly as it was. |
| `anchors_disagree` | The frames AURA picked as anchors disagree with each other, so it has not matched anything to them. |
| `mood_preserved` | The light here is meant to be this colour, so AURA left it alone. |
| `user_edited` | You set this frame yourself, so AURA has not touched it. |
| `disabled` | Gallery matching is switched off for this frame. |
| `bounded_by_policy` | This frame was a long way from the rest, so AURA moved it as far as it is allowed to and no further. |
| `mixed_light_skipped` | There is more than one kind of light in this frame, so a single correction would have been wrong somewhere in it. |
| `tone_estimate_absent` | AURA has not worked out the light in this frame yet, so there is nothing to match. |
| `colour_decision_absent` | AURA has not graded this frame yet, so its contrast and colour were left alone. |
| `skin_mask_absent` | AURA cannot yet tell which pixels are this person's skin, so it has not adjusted any of them. |
| `skin_target_absent` | AURA has not seen this person in enough well-lit frames to know how they should look, so it has left their skin alone. |
| `segment_absent` | This frame is not part of any chapter yet, so there is nothing to match it to. |
| `outlier_after_normalisation` | This frame is still noticeably different from the rest of this part. |
| `skin_outlier` | This person's skin still looks different here from how it looks elsewhere. |
| `warmth_normalised` | The warmth was brought into line with the rest of this part. |
| `exposure_normalised` | The brightness was brought into line with the rest of this part. |
| `skin_normalised` | This person's skin was brought into line with how they look across the rest of the wedding. |
| `grade_harmonised` | The contrast and colour character were brought into line with the rest of this part. |
| `already_consistent` | This frame already matched the rest of this part. |
| `node_split_by_change_point` | The light genuinely changed part-way through, so the two halves are matched separately. |
| `split_too_small` | The light seemed to change here, but there were too few frames on one side to treat it as a separate look. |
| `anchor_pinned` | You pinned one of the anchors for this part of the wedding. |
| `anchor_rejected` | You rejected an anchor AURA had chosen here. |
| `robust_target` | One anchor disagreed with the others, so the target came from the middle of them rather than the average. |
| `node_sub_clustered` | This chapter was long enough to be matched in parts rather than as one block. |
| `node_anchored` | This part of the wedding is anchored to its best frames. |
