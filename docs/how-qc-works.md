# How AURA checks its own work, in the product's own words

> Before you deliver, something looks at every photograph again and tells you what it is not sure
> about.

AURA makes a lot of decisions about a wedding: which frames to keep, how bright each one should be,
what colour the light was, how much to smooth somebody's skin, where to crop. Every one of those is
a judgement, and judgements are sometimes wrong.

The quality check is the last thing that runs. It re-examines every photograph in the delivered
gallery, measures what AURA actually did against what AURA was aiming for, and produces a list of
findings — each one with the number behind it, so you can disagree.

This page is what it checks, what it will fix on its own, what it will never do, and — the part
that matters most on this build — **what it cannot check yet**.

---

## The one number to read first

Every quality report opens with two figures, and the second is the important one:

> AURA checked **800** delivered photographs, and **43 %** of the checks it wanted to run could not
> run — something they needed had not been measured.

A quality report is the only place in AURA where an empty result is genuinely ambiguous. "No
findings" means one of two opposite things:

- AURA inspected everything and your gallery is fine, or
- AURA could not inspect it.

**On this build, the second is the common case.** Face detection is not yet trained, so anything
that depends on knowing where a face is — skin consistency, crop safety, most of the retouch
checks — has nothing to measure and skips. A check that skipped is reported as **not checked**, in
grey, everywhere it appears. It is never rendered as a pass.

That is deliberate and it is the single most important design decision in the feature. A tool that
told you your gallery was clean when it had not looked would be worse than no tool at all.

---

## The ten things it checks

| What | What it means | What it measures against |
|---|---|---|
| **Matching the room** | This frame does not sit with the others shot under the same light | The lighting group's own tolerance, not a fixed number |
| **Skin** | Somebody's skin here looks unlike their skin in the rest of the wedding | That person's own appearance across your gallery |
| **Brightness** | The subject did not land where the scene wanted them, or the edit clipped something | The scene's brightness band, and the highlights before and after |
| **Detail** | The frame lost texture, gained ringing, or is soft where it should be sharp | What the restoration self-check measured on the rendered result |
| **Retouching** | Skin, teeth, eyes or hair were worked harder than they should have been | The texture floor and the naturalness readings from the retouch pass |
| **Edges** | An adjustment was applied through a boundary that was not good enough to carry it | How well the region's edge was actually determined |
| **Framing** | The crop cut a face, dropped below the resolution floor, or threw away too much | The crop safety report |
| **Tidying** | A removed distraction left a mark | The cleanup self-check's own artefact score |
| **Near-duplicates** | Two nearly identical frames both ended up in the gallery | The distance between them, and whether they came from one moment |
| **Coverage** | A guaranteed moment is not covered, or is covered weakly | The coverage rules for this wedding |

Every threshold above is **per scene**. A getting-ready frame and a dance-floor frame are held to
different standards, because they are different photographs. Twenty-three scenes have their own row
and every row carries a written reason.

**A studio can make any of those stricter and nobody can make one looser.** They live in
`crates/aura-qc/config/qc_thresholds.toml`, they are checked against the code's own ceilings every
time the file loads, and a file that loosens one is refused outright rather than quietly clamped.

---

## What it will fix on its own

Not much, and that is on purpose.

When AURA opens a finding it also proposes a **remedy** — usually "re-solve the white balance
against this frame's lighting group", or "run this adjustment at three quarters strength". It will
apply that remedy without asking you only when two things are both true: it is confident enough,
and the decision falls in a band that permits acting unattended.

Then it **re-inspects the frame** and keeps the change only if it worked:

- The problem must have shrunk by at least **half** of what the remedy promised. A change that
  barely moved anything is put back.
- Nothing else may have got measurably worse. If fixing the colour made the skin drift, the change
  is put back and the finding goes to you.
- It gets **two attempts**. After that it stops and hands the frame over, rather than pushing
  further and further on a frame that is not responding.

The report tells you both numbers — what was fixed, and what was tried and put back. Those are
separate columns, because a change that was reverted is not in your delivered files and counting it
as a fix would describe work that does not exist.

---

## Swapping a frame for a better one

Sometimes the problem is not the edit, it is the photograph. When a frame has a runner-up from the
same moment, AURA can swap it — but only through four gates, in this order:

1. The finding must be about something a different frame could actually solve.
2. The alternative must be **measurably better on the metric that raised the finding**, by a clear
   margin, not by a rounding error.
3. AURA must be at least 85 % sure.
4. **If a coverage guarantee is holding the original frame in your gallery, the alternative must
   carry the same guarantee.**

That fourth gate runs *before* anything is scored, not as a penalty afterwards. A frame that is in
your gallery because it is the only photograph of the ring exchange cannot be swapped for a prettier
frame that is not.

Every swap is listed in the report with **both** frames' numbers — what the old one measured and
what the new one measures — never the difference. "0.3 better" tells you nothing about whether the
frame that went in is good.

---

## Your verdict beats everything

Two things you can say about a finding:

- **This is real.** AURA records that you agreed. You can also tell it to go ahead and apply the
  remedy, whatever band it fell in — you have looked, and that is the one direction that is safe.
- **This is not a problem.** AURA records that you disagreed, and **the finding does not come back
  on the next pass**.

Nothing can overwrite either of those. Not the next pass, not a threshold change, not an update.
The database itself refuses it.

You can select many findings at once and say either thing about all of them. What you **cannot** do
in bulk is authorise AURA to change forty photographs. Agreeing that forty findings are real is a
statement about the findings; letting AURA act on forty frames unattended is a statement about the
remedies, and those are different judgements made with different amounts of attention. Applying a
remedy lives on the individual finding, next to the before and after.

**How often you disagree is a number AURA keeps.** If more than a small share of the findings you
review turn out to be false alarms, that is a fault in the checking rather than in your gallery, and
it is meant to be visible.

---

## What it will never do

- **It cannot delete a photograph.** There is no path in this feature that removes a file or a
  frame; a swap exchanges one frame for another that was already shot.
- **It cannot loosen a limit.** Nothing on the panel, in the wire, or in the settings can raise a
  threshold or a ceiling. Every bound belongs to the code; a studio's file may only tighten one.
- **It cannot re-solve a photograph you edited by hand.** A frame you touched is still reported —
  you are entitled to know it sits outside its group — and AURA will not change it.
- **It cannot invent a diagnosis.** There is no free-text field anywhere in the feature that a
  sentence could be written into and stored. Every sentence you read is composed from the
  measurement, on the spot.
- **It does not use a model to find defects.** Every check is a comparison between numbers another
  part of AURA already recorded. That finds fewer problems than a trained detector would — and it
  does not invent them.

When AURA needs a second opinion — a photograph with several findings at once, where the cause is
not obvious — it can ask a reasoning model for a plan. That model can only ever suggest doing
**less**: it cannot approve an action, it never sees a photograph, and it is never told whose
wedding it is. If it is unreachable, the frame goes to you, which is where it would have gone
anyway.

---

## What this build cannot tell you

Honest limits, in the order they matter:

**Most checks skip.** Face detection, region segmentation and face recovery are not trained in this
build, so the checks that depend on them have nothing to measure. Read the completeness figure
before you read anything else.

**Nobody has measured whether photographers agree with the findings.** The thresholds were chosen by
argument and tested against galleries this repository authored — which cannot disagree with them.
Whether a real photographer looks at a finding and thinks "yes, that frame is wrong" is the headline
question of the whole feature, and it is unanswered.

**The re-edit loop has never run against a wedding.** Every remedy in every test was applied to
authored readings rather than to photographs.

**The planner has never reached a real model.** Its refusals are tested; its answers are not.

---

## Turning it off

Run the check without letting it fix anything: it inspects, reports, and changes nothing. That is
the safe thing to run before a delivery and it is what the button does by default. The button that
lets AURA act is separate and says so.

If you never open the panel, nothing in this feature ever runs.
