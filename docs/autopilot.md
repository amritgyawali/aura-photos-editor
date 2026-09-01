# One button

You shot the wedding. You got home at one in the morning. There are four thousand RAWs on two
cards, and tomorrow there is another wedding.

Import them, press **Edit complete wedding**, and go to bed.

---

## What happens while you sleep

AURA works through twenty-five steps, in an order that is fixed because the steps depend on each
other. It looks at every photograph, works out who is in them and what part of the day they are
from, groups the frames you shot in one burst, checks focus and eyes, reads the moment, chooses the
gallery, and then edits what it chose — light, colour, your own look, retouching, noise, cropping —
before making the whole wedding look like one body of work and checking its own work at the end.

You can watch it if you want. It shows you which step it is on, how many photographs are left, how
long it thinks it needs, and a thumbnail of whichever frame it is looking at.

## Stopping is free

Close the lid. Pull the power. Kill the app. Your machine can crash.

Nothing is lost, and nothing is repeated. AURA commits each photograph's work and its own record of
that work in the same breath, so there is no state where one happened and the other did not. Press
the button again and it carries on from the photograph it was on — not from the beginning of the
step, and certainly not from the beginning of the wedding.

The **Stop** button works the same way. It finishes the photograph it is on and stops there, which
is why stopping never leaves a half-written gallery or a folder with half your delivered files in
it.

There is one thing that starts again rather than continuing: if something the step depends on has
changed since last time — you imported another card, or an update changed how something is measured
— that step runs again on its own. AURA tells you which one and why, rather than quietly doing it.

## Before it starts

AURA checks eight things before committing to a two-hour job, because finding out at 90 % that the
disk is full is the most expensive way to learn it.

Four of them will stop it:

- **The wedding will not open.** Open it once from the projects list, then try again.
- **There are no photographs.** Import them first.
- **There is not enough room on the disk.** It tells you how many gigabytes to free, and it asks for
  more than the files themselves — a run also writes previews, its own records and a report.
- **A model it cannot work without is missing.** Install the model pack from Settings.

Four are worth knowing and do not stop anything: what your machine can do, what is left of your AI
budget, whether you are on battery, and how much AURA is currently allowed to do on its own.

## Zero-Touch, honestly

Zero-Touch is the switch that says *work while I am away*.

It does not mean AURA does everything and asks nothing. AURA keeps a confidence on every decision it
makes, and how confident it has to be before acting on its own depends on how hard the decision is
to take back. Choosing which photographs go in the gallery is easy to undo — a rejection is a row,
nothing on your disk moves, and one drag of the size slider changes the whole gallery. Retouching
somebody's face is not.

**In this release AURA has not yet learned how often it is right.** Until it has, it is being
careful on purpose: it does the work, and everything it cannot take back goes in your review queue
rather than happening quietly. You will see more in that queue now than you will once it has
learned.

The panel says this before you start, and the summary says it again. It is not an error and there is
nothing to fix — it is AURA telling you how much of this run it expects you to look at.

## When a step cannot run

A wedding does not fail because one step could not happen.

If the retouching cannot run — a missing model, a machine that ran out of memory, three failed
attempts — AURA carries on with the rest of the wedding and finishes as **Finished, with some steps
skipped**. The summary lists every step that did not happen and says why, in a sentence.

Only four steps can end a run: importing, building previews, looking at every photograph, and
choosing the gallery. Without those there is no wedding to deliver — there are four thousand
unsorted files.

Two more steps do not exist yet in this release: building the album and writing the files. They are
in the list, they say so, and the run finishes as *skipped* rather than pretending an album was made.

## Your machine

AURA watches how hard it is pushing and backs off before your laptop does.

If the machine gets hot, it slows down and says *reducing speed to protect your machine* rather than
silently taking twice as long. If you start working in something else, it gets out of the way. On
battery, the heavy steps wait until you plug in — you can tell it to go ahead anyway, and it will use
the battery quickly. If the graphics card stops responding, it carries on using the processor and
updates the estimate honestly.

There is exactly one thing that makes it stop: a full disk. Everything else clears on its own — a
machine cools, a foreground app closes, a laptop gets plugged in — and a full disk does not.

Nothing AURA can read about your machine ever makes it go *faster*. A sensor it cannot read costs
nothing.

## The checklist

Every step except the four essential ones can be switched off before a run.

Two are **off unless you turn them on**, and both for the same reason: they are the two steps that
change a photograph in a way a measurement cannot check on its own.

- **Hair, teeth and eyes.** A stray hair and a twig look identical to a measurement, and how natural
  the results look has not been studied yet. Turn it on once you have looked at what it does.
- **Removing distractions.** In this release it removes nothing at all — it has no trained model, so
  it proposes nothing on a real photograph. It stays off so that the day it *can* remove things is a
  day you say yes to, rather than a switch you flipped years earlier when it was inert.

Everything else is on, because everything else either helps or costs nothing when there is nothing
to do.

## Afterwards

The summary leads with what did not happen, because that is what you actually came back for. Under
it: how many photographs were chosen, how many are waiting for you, what it spent on AI, where the
time went, and every time your machine asked it to slow down.

If a run took four hours when it said two, that last list is the answer.

---

## What this release cannot do yet

Said plainly, because you will notice:

- **It does not write your delivered files.** The exporter is the next phase. A run finishes with a
  gallery chosen and edited in the catalog, and nothing on disk.
- **It does not build albums.** Also the next phase.
- **The times it quotes are estimates from a specification, not from your machine** — until it has
  done about a tenth of the run, at which point it starts using what your machine is actually doing
  and says so.
- **It cannot read your machine's temperature, battery or memory in this release.** The policies are
  built and tested; nothing fills them yet, so on this build they never fire.
- **Nobody has measured how often you will need to step in.** The target is fewer than eight
  photographs in a hundred, and that number needs ten real weddings and a person with an opinion —
  neither of which exists yet.
