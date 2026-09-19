# Matching a look

You have seen an account whose photographs look the way you want yours to look. This is how AURA
gets you there, and what it will and will not do on the way.

## The short version

Point AURA at photographs whose colour and tone you admire. It measures what they do - how bright
they sit, which way the light leans, how much contrast there is, how strong the colours are, what
the shadows and the highlights are tinted with - and it measures the same things about **your own
photographs, as AURA has already decided to render them**. The difference between those two is the
look. AURA then shifts your photographs by that difference and measures how far they actually
moved.

## What AURA cannot do: go and get the photographs

**AURA does not download photographs from a page.** You paste the address, AURA writes down which
page the look is from, and then it asks you for a folder.

There are two reasons and both of them are real.

The first is about this build. AURA is built so that exactly one part of it can reach the network
at all - the part that talks to AI providers, when you have given it a key - and everything else is
prevented from opening a connection by a check that fails the build. That restriction is why AURA
can honestly tell you your photographs never leave your machine. Carving an exception into it so a
gallery feature could fetch some JPEGs would make that sentence false for the sake of saving you a
drag-and-drop.

The second is about Instagram. Downloading a page's photographs in bulk is something Instagram
allows through its own tools, to the person who owns the account. The unofficial ways of doing it
are against its terms, stop working without warning, and can get an account restricted. We are not
going to put your account at risk to save you a folder.

## What to do instead

**Save the photographs into a folder.** Anything you have the right to keep - images you have
saved, a mood board a client sent you, your own earlier work. Point AURA at the folder.

**Or ask Instagram for your data export**, if the account is yours, and point AURA at what arrives.
AURA understands the export's own layout and reads the posts inside it rather than your profile
picture.

**Paste the address anyway.** It costs nothing and it means every report says which page this look
came from, instead of saying "a folder".

## What AURA needs

**At least eight photographs**, and it will tell you the look is rough until there are
twenty-four. This is not an arbitrary floor. A look measured from four photographs is not a vague
claim about a page - it is a *confident* claim about four photographs, and confident and wrong is
the worst thing a tool like this can be.

**Some of your own photographs already analysed.** A look is a difference, and the thing it is a
difference from is what AURA would have done to *your* wedding. Run Autopilot, or the tone and
colour passes, first. The card tells you how many of your photographs are ready and how many there
are, so you can see which of the two numbers is the problem.

## What a look actually contains

Two things.

**An overall lean** - the difference that applies to every photograph.

**One answer per kind of light.** AURA sorts the reference photographs by what light they look like
they were made in: daylight, golden hour, open shade, overcast, tungsten, candlelight, stage light.
A photographer who runs their receptions warm and their portraits neutral is expressing two
decisions, and a single average of the two is a third look that belongs to nobody.

What a look does **not** contain is an answer per *kind of photograph*. A JPEG on a page does not
say whether it is a ceremony or a reception, so AURA does not pretend to know, and it says so in
the report. The look you get for a ceremony in candlelight is the look that page has in
candlelight, whatever it was photographing.

## What AURA will not change

**Anybody's skin.** AURA learns nothing about skin from a reference. It cannot: finding skin in a
stranger's photograph would mean deciding in advance what colour skin is, and AURA does not have
that number anywhere - not in this feature, not in its settings, not in its database. See
`docs/skin-fairness.md`.

What protects skin instead is the same thing that protects it everywhere else in AURA. A look is
applied *before* the skin guard runs, and the guard measures what actually happened to the skin in
**your** photograph, through the real renderer. If matching a look would move it, the guard pulls
the colour part of the look back or drops it entirely. You get the look; the people in your
photographs stay the colour they were.

**Any colour's hue.** AURA will match how *strong* the colours are, band by band, and how bright.
It will not rotate one. Getting a colour's strength slightly wrong makes it slightly too rich.
Getting a hue wrong makes a person a different colour - and most of a face, at every skin tone,
sits in the same band as the flowers.

**Anything you edited by hand.** A photograph you have adjusted yourself is measured and left
exactly as you made it. It still counts in the report, because the report is about the gallery you
are going to deliver rather than about a sample that flattered the number.

## How far a look will go

Not as far as your own trained profile will. If you teach AURA from your own archive - the "Teach
my AI" feature - it will move a photograph by up to two thirds of a stop. A matched look will move
it by half a stop, and its colour temperature by 600 kelvin rather than 800.

That difference is deliberate. When you teach AURA from your own work, you are telling it about
decisions you actually made. When you point at a page you admire, you are expressing a preference -
about photographs somebody else made, somewhere you have never been. A page that reads bright might
be bright because that photographer shoots in Greece.

You can also apply less of a look, with the slider. There is no way to apply more than the
reference. That is not an oversight; matching a look and exaggerating one are different things, and
only the first is what you asked for.

## What the report tells you

**How much of the gap closed.** Not whether a threshold was met - how far your photographs actually
moved toward the reference, as a percentage of how far apart they were. A look that closed ninety
per cent of a big difference did its job. A look that "passed" because there was nothing to close
did not do anything at all, and AURA says so in those words.

**How many of your photographs it was measured over**, and how many of them you had already edited
by hand.

**What it could not learn.** Every look carries the same three notes, whatever else is true of it:
there is no scene axis, nothing was learned about skin, and no hue was rotated. They are there on a
look that worked perfectly, because a note that only appeared when something went wrong would let
you assume the opposite every other time.

## What has not been proved

Nobody has yet sat a photographer in front of a gallery AURA matched and the page it was matched
to, and asked whether it worked. Every number in this feature was measured against photographs this
repository generated, carrying looks it applied itself.

The arithmetic is real and it is tested. Whether the result is the thing you meant when you pointed
at that account is, for now, your call rather than a measurement. It is condition C2 in
`docs/progress/PHASE-31-EXIT.md` and it is the first thing that should close.
