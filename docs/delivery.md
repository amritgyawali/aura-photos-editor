# Getting the wedding out

The edit is finished. Now somebody has to be able to open it.

This is what AURA does when you press **Export**, what it promises about the files it writes, and
what it will not do on its own.

---

## Every file is read back before AURA says it wrote it

This is the part worth knowing first, because it is the part nobody else does.

AURA writes a file, flushes it, tells the operating system to commit it to the drive, then **opens
it again, reads every byte back and hashes what it read**. The digest it stores is the digest of
what is on the disk, not of what it meant to put there.

That matters because the failure it catches is silent. A card reader with a loose connector, a NAS
that dropped a packet, a drive that is on its way out — none of these announce themselves. They give
you a folder of files that look right in a file browser and are wrong in the middle. You find out
when the client does.

If a file does not read back the same, **the export stops**. It does not skip that frame and carry
on, because a destination that corrupted one file is a destination you should not send three
thousand more to. What has already been written is listed and unharmed.

You can switch verification off. It costs about two per cent of the time an export takes, so there
is very little reason to.

## Sets

An export is up to eight **sets**, and each one is a complete answer to "what does this gallery look
like as files".

| Set | What it is usually for |
|---|---|
| gallery | The full delivery. Full size, high quality, sRGB. |
| album | The printer's copy. TIFF, 16-bit, Adobe RGB. |
| social | Long edge 2,048, sharpened for a screen. |
| teaser | A handful of frames the night of the wedding. |
| bw | The monochrome conversions, if you took them. |
| hand-off | XMP sidecars beside the originals, for Lightroom. |

They are written in one pass, from one render each, so the album and the gallery cannot disagree
about what a photograph looks like. You can edit any of them, and you can add your own.

## Naming

A template of tokens: `{seq}`, `{original}`, `{date}`, `{couple}`, `{chapter}`, `{camera}`, `{set}`.

`{date}_{couple}_{seq}` gives you `2026-05-16_alex-and-sam_0001.jpg`.

Three things about it are deliberate.

**Every name is planned before anything is written.** If two frames would produce the same name —
which happens the moment you use `{original}` across two cards that both start at `DSC_0001` — the
second one is suffixed, and AURA tells you before the export starts rather than after. You can see
the whole list of names a template would produce without writing a file.

**A template cannot name a folder.** `{date}/{seq}` is refused. A naming template that could
contain a path separator is a naming template that could write outside the folder you chose, and
there is no version of that which is a feature.

**A name is tidied, not trusted.** Anything a filesystem would object to is replaced, runs of dots
are collapsed, and a name that ends up empty gets the sequence number instead.

## Colour

Every file carries an ICC profile — sRGB, Adobe RGB or Display P3 — written into the file itself, so
a browser, a print lab and Photoshop all agree about what the numbers mean. A JPEG with no profile
is a JPEG that looks different everywhere.

Resizing happens in linear light. Sharpening happens after it, on the encoded values, at a strength
that suits the size being written. That order is not arbitrary: resizing in the encoded domain
darkens a photograph slightly and does it worst in the highlights, and sharpening before a resize
sharpens detail that is about to be thrown away.

## What travels with the files, and what does not

**Location is removed by default.** A wedding photograph's EXIF knows where the wedding was, and a
gallery uploaded to a public link is a map of somebody's home and their family's church. You can
switch it back on.

Your copyright, your contact details, your name and your keywords are written in. The camera body's
serial number can be stripped.

Nothing else is copied forward. AURA **builds** the metadata block rather than copying the
original's and editing it, so a tag it has never heard of cannot survive into a delivered file by
accident.

## The manifest

Every delivery seals a `aura-delivery-manifest.json` beside the files. It lists every file, its
size, its digest, which set it came from, what was removed from it generatively, and which versions
of AURA made it.

It is written once and can never be edited — the database refuses the statement — because a record
of what you delivered is only worth anything if it cannot be quietly changed afterwards.

If you ever need to prove a file is the file you delivered, or work out which of four thousand
frames a client's complaint is about, this is the document.

## Backups

Point AURA at a second drive or a NAS and it copies the delivery there, then compares digests file
by file.

Three answers, and they mean different things:

- **Matched** — every file on the backup is the file you delivered.
- **Missing** — files that are not there yet. AURA copies them.
- **Diverged** — a file that exists on both and is *different*. AURA **stops** and tells you which
  one. It does not overwrite, because the copy on the backup might be the good one.

## Client galleries

AURA can upload a delivery to a client gallery service, per set, with the sets mapped to whatever
that service calls its folders.

Uploads resume. If a wedding stops at 60 % because the wifi dropped, the next attempt asks the far
end how much it already has and sends the rest, rather than starting again.

**This release ships no network transport.** The provider interface, the per-set mapping, the resume
and the digest comparison are all built and all tested against a folder on your own machine; what is
not here is the code that opens a socket. An upload to Pic-Time or SmugMug from this build is not
possible, and the panel says so rather than failing halfway.

## Lightroom and Photoshop

XMP sidecars beside the originals, so Lightroom reads AURA's develop settings on import. The
Lightroom plugin sends a selection back the other way.

The Photoshop hand-off writes a TIFF with the retouch and mask work as separate layers where the
operation can be expressed as one.

Neither is a round trip that preserves everything. AURA's masks, its local light and its retouch are
richer than a develop preset, and a sidecar carries what the format can carry. What it does carry is
enough that a photographer who wants to finish in Lightroom is not starting from the RAW.

## What AURA will not do here

- **It will not delete an original.** Nothing in this product has a path that removes a file you
  imported.
- **It will not pick a destination for you.** If you press "edit complete wedding" and go to bed,
  the export step runs only when this wedding already has an export you set up. Otherwise it stops
  there and tells you, rather than writing three thousand files somewhere you did not choose.
- **It will not upload without being asked**, and it will not send an original anywhere. Only the
  files an export wrote.
