# ADR-0036 - The style IPC surface

- **Status:** accepted
- **Date:** 2026-08-19
- **Phase:** 17 - Style Learning: Scene-Conditional Personal AI Profiles ("Teach My AI")
- **Extends:** ADR-0030 (develop IPC surface), ADR-0032 (tone IPC surface), ADR-0034 (colour
  IPC surface)
- **Deciders:** CTO, Senior Frontend Engineer, ML Lead - Vision, Security & Privacy Engineer,
  Product Manager

## Context

Phase 17 adds ten commands and fourteen wire shapes.
`crates/aura-app/src/contract/ipc.rs` and `ui/src/ipc/types.ts` are frozen contracts checked
by `cargo xtask contracts --check`, so every addition needs an ADR and a re-lock, in that
order.

This is the first surface in the product that takes a **folder of the photographer's own
finished work** as input, and the first that produces an artefact meant to leave the machine.
Six decisions follow.

## Decision 1 - The surface carries paths *in* and never carries imagery *out*

`ScanArchiveInput` takes a directory. `TrainProfileInput` takes a project and a profile name.
Nothing on this surface returns a pixel, a thumbnail, a crop or a base64 blob, and
`StylePairDto` carries the two file names, the bucket, the fit residual and the verdict - not
the photograph.

Section 9 gives SEC a single task, "ensure archives are never uploaded", and section 13's
fifth criterion is "never uploads imagery". Both are properties of the shapes rather than of
the code that fills them: there is no field on this surface that could hold an image, so
"AURA did not upload your archive" is checkable by reading fourteen struct definitions.
`aura-style` also depends on no cloud crate at all (ADR-0035 decision 9), which is the second
half of the same statement.

## Decision 2 - `overallDe00` and `perBucket` are always sent together, and a bucket with no held-out pairs sends `null` rather than zero

`ProfileReportDto::perBucket` is a list of `{ bucket, samples, matchDe00, level }`, and
`matchDe00` is `number | null`.

Zero would read as a perfect match. A bucket that was trained on eleven pairs and evaluated on
none has no measurement at all, and the report's whole purpose is to be *honest about where
the profile is weak* - section 6.3's "this is the number shown in the UI, not a vague 'profile
ready'". `null` renders as "not measured yet" in `ProfileReport.tsx` and a test asserts it
never renders as a number.

`level` is on the wire for the same reason: a bucket whose answer came from its parent group
is not a bucket the photographer taught, and the matrix draws the two differently.

## Decision 3 - Adoption is an explicit command, and the A/B comparison is a read

`train_profile` produces a profile at `status: "candidate"`. `adopt_profile` is a separate
command that a photographer invokes after looking at `compare_profiles`.

Section 6.3 asks for exactly this: "A/B compare: side-by-side of old profile, new profile and
the photographer's own edit before adoption; adoption is an explicit action." Training that
adopted on completion would be a product that changes how every future photograph looks as a
side effect of a progress bar finishing.

`compare_profiles` returns three parameter sets per sampled frame - baseline, current profile,
candidate profile - and the photographer's own values when the frame came from a pair. It
returns no pixels; `AbCompare.tsx` renders the numbers and asks the develop surface for the
previews, which is the surface that already owns turning a recipe into an image.

## Decision 4 - `weakBuckets` ships with a sentence, and the sentence is generated from the gap

`ProfileReportDto::recommendation` is a string like "add one indoor flash reception - the
dance-floor bucket has 6 pairs and 2.5 dE00 of unexplained variation".

A list of bucket slugs is a list a photographer has to interpret. Section 6.3 asks for "a
concrete recommendation ('add one indoor flash reception to improve dance-floor accuracy')",
and generating it on the Rust side rather than in the panel means the CLI report, the exit
report and the UI all say the same sentence. It is assembled from the reason registry rather
than free text, so it can be translated later.

## Decision 5 - Export writes a file the user names; import takes a path and returns a fingerprint

`export_profile` takes a destination path and returns the bytes written and the key
fingerprint. `import_profile` takes a source path and returns the profile it read, its
fingerprint, and whether the signature verified.

`signatureValid` is a boolean and it is **not** called `verified`. What it means is that the
document has not changed since it was signed by whoever holds the embedded key - integrity,
not provenance (ADR-0035 decision 8). A bundle whose signature does not verify is refused with
`AURA-ML-5076` before it is parsed into a profile, so a tampered document never reaches the
tree builder. The panel shows the fingerprint and the word "unchanged since signing"; a test
asserts it never shows the word "verified".

## Decision 6 - Profile selection is per project and per chapter, and the chapter overrides live on the project

`set_project_profile` takes a project and a profile. `set_chapter_profile` takes a project, a
chapter and an optional profile - `null` clears the override and the chapter falls back to the
project's.

Section 6.4's real studio practice is "a moody reception with an airy ceremony", so the
chapter is the unit and phase 07's nine-chapter vocabulary is already the right one. The
overrides are rows on the project rather than fields on a profile, because a profile is a
portable artefact that a studio distributes and a chapter assignment is a decision about one
wedding. A profile carrying chapter assignments would be a profile that rearranges somebody
else's catalog when they import it.

## Consequences

- Ten commands: `style_status`, `list_profiles`, `profile_report`, `scan_archive`,
  `train_profile`, `adopt_profile`, `compare_profiles`, `export_profile`, `import_profile`,
  `set_project_profile` (with `set_chapter_profile` folded into it as an optional chapter).
  All of them run off the renderer thread; two of them can run for twenty minutes.
- Fourteen wire shapes, all `camelCase`, in both frozen files, re-locked in the same commit.
- `TrainProfileDto` carries `cancelled` and the pass is resumable, because a photographer who
  cancels at 60 % and restarts must not re-fit 1,200 pairs. The resume unit is the pair, and
  the state is a row in `style_pairs` rather than a journal.
- **Nothing on this surface can write a recipe.** Style reaches the pixels through
  `ColourPass` and `TonePass`, which reach them through `aura_recipe::schema::merge`, which is
  phase 14's rule for the fourth phase running.

## What this surface deliberately cannot do

- **Return imagery.** See decision 1.
- **Adopt on training.** See decision 3.
- **Learn from an in-app correction.** That is phase 30's learning loop, which updates these
  same profiles through a different door. Section 2.2 puts it out of scope and there is no
  command here that takes a correction.
- **Set a retouch strength.** Phase 20 has its own strength learning and section 2.2 says so.
- **Normalise a gallery.** Phase 25's, still.
