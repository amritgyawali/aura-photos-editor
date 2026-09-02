# What AURA knows, and where it goes

Wedding photographs are among the most personal files anybody hands to software. Faces, families,
homes, religious ceremonies, and where all of it happened.

This is the whole picture: what AURA stores, what leaves your machine, and what it will not do.

---

## The short version

**Nothing leaves your machine unless you turn something on.** Not photographs, not faces, not
locations, not corrections, not crash reports.

There are exactly four things that can send anything anywhere, and all four are off until you
switch them on:

1. **Cloud AI** — off. When on, sends small crops or text, never an original.
2. **Client gallery upload** — off, and needs a destination you set up.
3. **Crash reporting** — off. When on, sends a stack trace and no filenames.
4. **Dataset contribution** — off. When on, shares corrections and needs its own consent record.

## Faces

AURA recognises people so it can tell you who is in a photograph and make sure the bride's father is
not missing from the gallery.

The face templates it computes for that live in a **sealed store** in your own catalog. They cannot
leave it. There is no export path, no cloud task that accepts one, and no field on any API that
could carry one. The crate that computes them is checked by a source scan on every build.

A face template is not a photograph and cannot be turned back into one, but it is biometric data
under most people's law and all of anybody's expectations, and it is treated that way.

**AURA never infers anything about a person.** Not gender, not ethnicity, not age, not religion, not
any relationship beyond "these two are the couple", "this person is close family" and "this person
is a guest". This is not a setting. The types that carry a conclusion about a person have no room to
express one.

Which of two people is the bride is not a photographic fact, and automation never assigns it.

## Location

Every photograph's EXIF knows where it was taken. A wedding gallery uploaded to a public link is
therefore a map of a church, a venue and often somebody's home.

**Location is stripped from every delivered file by default.** You can switch it back on per export.

It stays in your catalog, because it is your photograph and you may want it. It just does not travel
with the copy the client gets unless you say so.

## Cloud AI

Off by default, and when it is on, four rules hold:

- **An original never leaves.** The code that builds what gets sent cannot express an original file;
  it builds small crops and text.
- **Every call is priced before it is made**, against a budget you set.
- **Every call is recorded**, including the ones that never reached a model, so "what has AURA sent"
  is a query rather than a guess.
- **The cloud proposes and local code decides.** No cloud answer overrules a confident local
  decision, and the one AI call in the whole delivery path — a judgement about whether a removal is
  safe — has an answer type that cannot approve anything. It can only make AURA do less.

Your API key lives in your operating system's credential store. It is never written to the catalog,
a config file, a log, telemetry, or a prompt. A source scan fails the build if it ever is.

## Corrections and the learning loop

Off by default. When on, your corrections stay on your machine, and the fit happens locally.

Turning on **dataset contribution** is a separate switch with its own per-project consent record,
and it is what makes anything shareable at all.

`docs/learning-loop.md` has the detail, including the things AURA will never learn — among them,
anything about a person's appearance.

## Crash reports and telemetry

Off by default. When on, a crash sends the stack trace, the build, the operating system and which
stage was running. Not which photograph, not which wedding, not which client, not any filename.

`ops/crash/telemetry.toml` lists every field that can be sent. It is short, and it is meant to be
read rather than trusted.

## Support bundles

When you ask for help, AURA can produce a bundle of what it decided and why: reasons, confidences,
versions, and the shape of the evidence.

Every identifier is replaced by a handle, and **there are no pixels in it**. That is a property of
how evidence is represented rather than a promise about the exporter — the only things a piece of
evidence can be are a rectangle, a list of frame handles, or a list of named parameter changes.

## Deleting things

Deleting a project deletes everything derived from it — its face templates, its decisions, its
corrections, its export records — because every one of those rows is tied to the project by a
foreign key that cascades.

**Your original files are never touched.** Nothing in this product has a code path that deletes a
photograph you imported. A rejected frame is a row that says it was rejected, and it stays exactly
where you put it.

## Two things worth knowing about this build

**The face and scene models shipped here are placeholders.** The detector finds no faces and the
recogniser's templates carry no identity information. Every protection above is real and is being
applied to something that is not yet a claim about a person.

**No network transport ships.** The upload path is built and cannot open a socket. Whatever else is
true of this build, it has not sent your wedding anywhere.
